//! Dongle operations built on `Transport`: identify, learn and transmit.
//!
//! Exists to implement the command/response sequences of elksmart-comm.c
//! (`send_identify`, `send_learn`/`recv_learn`, `send_transmit`) once,
//! against the trait, so
//! the probe tool and later the app session share tested logic. Waiting is
//! expressed as counts of read timeouts so tests are deterministic.

use std::fmt;
use std::time::Duration;

use elk_proto::command;
use elk_proto::identify::{self, Protocol};
use elk_proto::learn::{LearnAssembler, LearnError, LearnedCode, Progress};
use elk_proto::pulse::Pulse;
use elk_proto::transmit::{self, TransmitError};

use crate::transport::{Transport, TransportError};

/// Timeout for each bulk write.
pub const WRITE_TIMEOUT: Duration = Duration::from_millis(100);
/// Timeout for each read while flushing stale data before identify.
pub const FLUSH_TIMEOUT: Duration = Duration::from_millis(10);
/// Upper bound on stale transfers drained before identify.
pub const FLUSH_LIMIT: usize = 64;
/// Timeout for the identify reply.
pub const IDENTIFY_TIMEOUT: Duration = Duration::from_millis(500);
/// Total identify attempts before giving up.
pub const IDENTIFY_ATTEMPTS: usize = 4;
/// Timeout for each read while learning.
pub const LEARN_READ_TIMEOUT: Duration = Duration::from_millis(100);
/// Pause after each transmit chunk.
pub const CHUNK_PAUSE: Duration = Duration::from_millis(2);

/// How long `learn` waits, in units of [`LEARN_READ_TIMEOUT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LearnConfig {
    /// Consecutive empty reads allowed before a learn reply header arrives.
    pub wait_reads: u32,
    /// Consecutive empty reads allowed once a reply is being received.
    pub gap_reads: u32,
}

/// How a learn ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnOutcome {
    /// The announced length was reached.
    Complete(LearnedCode),
    /// A reply started but stopped before the announced length.
    Short(LearnedCode),
    /// No reply arrived within `wait_reads`. The dongle may now ignore
    /// commands until it sees an IR signal.
    NothingReceived,
}

/// Failure of an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpError {
    Transport(TransportError),
    /// The identify reply was not recognised; carries the reply.
    UnknownIdentify(Vec<u8>),
    Learn(LearnError),
    Transmit(TransmitError),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::Transport(e) => write!(f, "{e}"),
            OpError::UnknownIdentify(b) => write!(f, "unknown identify reply ({} bytes)", b.len()),
            OpError::Learn(LearnError::UnexpectedResponse(b)) => {
                write!(f, "unexpected learn response ({} bytes)", b.len())
            }
            OpError::Transmit(TransmitError::Empty) => write!(f, "nothing to transmit"),
            OpError::Transmit(TransmitError::TooLong(n)) => {
                write!(f, "code too long to transmit ({n} bytes compressed)")
            }
        }
    }
}

impl std::error::Error for OpError {}

impl From<TransportError> for OpError {
    fn from(e: TransportError) -> Self {
        OpError::Transport(e)
    }
}

/// Drains stale input, sends identify (retrying on timeout) and classifies
/// the reply.
pub fn identify(t: &mut impl Transport) -> Result<Protocol, OpError> {
    for _ in 0..FLUSH_LIMIT {
        match t.read(FLUSH_TIMEOUT) {
            Ok(_) => continue,
            Err(TransportError::Timeout) => break,
            Err(e) => return Err(e.into()),
        }
    }
    let mut attempt = 0;
    let reply = loop {
        t.write(&command::IDENTIFY, WRITE_TIMEOUT)?;
        match t.read(IDENTIFY_TIMEOUT) {
            Ok(reply) => break reply,
            Err(TransportError::Timeout) if attempt + 1 < IDENTIFY_ATTEMPTS => attempt += 1,
            Err(e) => return Err(e.into()),
        }
    };
    identify::classify(&reply).ok_or(OpError::UnknownIdentify(reply))
}

/// Arms learn mode and collects one code.
pub fn learn(t: &mut impl Transport, cfg: LearnConfig) -> Result<LearnOutcome, OpError> {
    t.write(&command::LEARN, WRITE_TIMEOUT)?;
    let mut assembler = LearnAssembler::new();
    let mut receiving = false;
    let mut idle = 0;
    loop {
        match t.read(LEARN_READ_TIMEOUT) {
            Ok(chunk) => {
                idle = 0;
                match assembler.push(&chunk).map_err(OpError::Learn)? {
                    Progress::Acknowledged => {}
                    Progress::Receiving { .. } => receiving = true,
                    Progress::Complete(code) => {
                        t.write(&command::STOP, WRITE_TIMEOUT)?;
                        return Ok(LearnOutcome::Complete(code));
                    }
                }
            }
            Err(TransportError::Timeout) => {
                idle += 1;
                let limit = if receiving { cfg.gap_reads } else { cfg.wait_reads };
                if idle >= limit {
                    return Ok(match assembler.take_partial() {
                        Some(code) => LearnOutcome::Short(code),
                        None => LearnOutcome::NothingReceived,
                    });
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Sends pulses at the given carrier frequency. `pause` is called after
/// every chunk with [`CHUNK_PAUSE`] (pass `std::thread::sleep` in production).
pub fn transmit(
    t: &mut impl Transport,
    frequency_hz: u32,
    pulses: &[Pulse],
    pause: &mut dyn FnMut(Duration),
) -> Result<(), OpError> {
    let chunks = transmit::transmit_chunks(frequency_hz, pulses).map_err(OpError::Transmit)?;
    for chunk in &chunks {
        t.write(chunk, WRITE_TIMEOUT)?;
        pause(CHUNK_PAUSE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{MockTransport, Step};

    const PLAIN_REPLY: [u8; 6] = [0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01];

    fn header(len: u16, data: &[u8]) -> Vec<u8> {
        let mut h = vec![0xFE; 4];
        h.extend_from_slice(&len.to_be_bytes());
        h.extend_from_slice(data);
        h
    }

    fn w(b: [u8; 4]) -> Step {
        Step::ExpectWrite(b.to_vec())
    }

    #[test]
    fn identify_flushes_stale_data_then_classifies() {
        let mut m = MockTransport::new([
            Step::Reply(vec![0x12, 0x34]),
            Step::Timeout,
            w(command::IDENTIFY),
            Step::Reply(PLAIN_REPLY.to_vec()),
        ]);
        assert_eq!(identify(&mut m), Ok(Protocol::Plain));
        assert!(m.is_done());
    }

    #[test]
    fn identify_retries_on_timeout() {
        let mut m = MockTransport::new([
            Step::Timeout,
            w(command::IDENTIFY),
            Step::Timeout,
            w(command::IDENTIFY),
            Step::Timeout,
            w(command::IDENTIFY),
            Step::Timeout,
            w(command::IDENTIFY),
            Step::Reply(PLAIN_REPLY.to_vec()),
        ]);
        assert_eq!(identify(&mut m), Ok(Protocol::Plain));
        assert!(m.is_done());
    }

    #[test]
    fn identify_gives_up_after_four_timeouts() {
        let mut script = vec![Step::Timeout];
        for _ in 0..IDENTIFY_ATTEMPTS {
            script.push(w(command::IDENTIFY));
            script.push(Step::Timeout);
        }
        let mut m = MockTransport::new(script);
        assert_eq!(identify(&mut m), Err(OpError::Transport(TransportError::Timeout)));
        assert!(m.is_done());
    }

    #[test]
    fn identify_flush_is_bounded() {
        let mut script: Vec<Step> = (0..FLUSH_LIMIT).map(|_| Step::Reply(vec![0])).collect();
        script.push(w(command::IDENTIFY));
        script.push(Step::Reply(PLAIN_REPLY.to_vec()));
        let mut m = MockTransport::new(script);
        assert_eq!(identify(&mut m), Ok(Protocol::Plain));
        assert!(m.is_done());
    }

    #[test]
    fn identify_unknown_reply_is_reported() {
        let mut m = MockTransport::new([
            Step::Timeout,
            w(command::IDENTIFY),
            Step::Reply(vec![0xFF; 4]),
        ]);
        assert_eq!(identify(&mut m), Err(OpError::UnknownIdentify(vec![0xFF; 4])));
    }

    #[test]
    fn learn_completes_after_ack_and_split_reply() {
        let mut m = MockTransport::new([
            w(command::LEARN),
            Step::Timeout,
            Step::Reply(vec![0xFF; 4]),
            Step::Reply(header(3, &[1, 2])),
            Step::Reply(vec![3]),
            w(command::STOP),
        ]);
        let cfg = LearnConfig { wait_reads: 5, gap_reads: 5 };
        assert_eq!(
            learn(&mut m, cfg),
            Ok(LearnOutcome::Complete(LearnedCode { announced_len: 3, data: vec![1, 2, 3] }))
        );
        assert!(m.is_done());
    }

    #[test]
    fn learn_reports_nothing_received_after_wait_reads() {
        let mut m = MockTransport::new([w(command::LEARN), Step::Timeout, Step::Timeout, Step::Timeout]);
        let cfg = LearnConfig { wait_reads: 3, gap_reads: 1 };
        assert_eq!(learn(&mut m, cfg), Ok(LearnOutcome::NothingReceived));
        assert!(m.is_done());
    }

    #[test]
    fn learn_reports_short_code_after_gap_reads() {
        let mut m = MockTransport::new([
            w(command::LEARN),
            Step::Reply(header(10, &[1, 2])),
            Step::Timeout,
            Step::Timeout,
        ]);
        let cfg = LearnConfig { wait_reads: 50, gap_reads: 2 };
        assert_eq!(
            learn(&mut m, cfg),
            Ok(LearnOutcome::Short(LearnedCode { announced_len: 10, data: vec![1, 2] }))
        );
        assert!(m.is_done());
    }

    #[test]
    fn learn_data_resets_idle_counter() {
        let mut m = MockTransport::new([
            w(command::LEARN),
            Step::Timeout,
            Step::Reply(vec![0xFF; 4]),
            Step::Timeout,
            Step::Reply(header(1, &[9])),
            w(command::STOP),
        ]);
        let cfg = LearnConfig { wait_reads: 2, gap_reads: 2 };
        assert_eq!(
            learn(&mut m, cfg),
            Ok(LearnOutcome::Complete(LearnedCode { announced_len: 1, data: vec![9] }))
        );
    }

    #[test]
    fn transmit_writes_all_chunks_with_pauses() {
        let pulses: Vec<Pulse> = elk_proto::nec::encode(&[0x00, 0x59, 0x00, 0x59]).unwrap();
        let chunks = transmit::transmit_chunks(38000, &pulses).unwrap();
        assert!(chunks.len() > 1);
        let mut m = MockTransport::new(chunks.iter().cloned().map(Step::ExpectWrite));
        let mut pauses = Vec::new();
        transmit(&mut m, 38000, &pulses, &mut |d| pauses.push(d)).unwrap();
        assert!(m.is_done());
        assert_eq!(pauses, vec![CHUNK_PAUSE; chunks.len()]);
    }

    #[test]
    fn transmit_empty_is_error_and_writes_nothing() {
        let mut m = MockTransport::new([]);
        assert_eq!(
            transmit(&mut m, 38000, &[], &mut |_| {}),
            Err(OpError::Transmit(TransmitError::Empty))
        );
    }

    #[test]
    fn learn_unexpected_response_is_error() {
        let mut m = MockTransport::new([w(command::LEARN), Step::Reply(vec![0x01, 0x02])]);
        let cfg = LearnConfig { wait_reads: 5, gap_reads: 5 };
        assert_eq!(
            learn(&mut m, cfg),
            Err(OpError::Learn(LearnError::UnexpectedResponse(vec![0x01, 0x02])))
        );
    }
}
