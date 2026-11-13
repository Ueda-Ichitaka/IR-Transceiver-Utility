//! Scripted `Transport` for tests.
//!
//! Exists so protocol operations can be tested without hardware: a script of
//! expected writes, canned read results and failures is replayed in order,
//! and any deviation panics with a description of what was expected.

use std::collections::VecDeque;
use std::time::Duration;

use crate::transport::{Transport, TransportError};

/// One scripted step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The next call must be a write of exactly these bytes.
    ExpectWrite(Vec<u8>),
    /// The next call must be a read; it returns these bytes.
    Reply(Vec<u8>),
    /// The next call must be a read; it times out.
    Timeout,
    /// The next call (read or write) fails with this error.
    Fail(TransportError),
}

/// Replays a script. Reads after the script ends time out.
#[derive(Debug, Default)]
pub struct MockTransport {
    script: VecDeque<Step>,
}

impl MockTransport {
    pub fn new(script: impl IntoIterator<Item = Step>) -> Self {
        Self { script: script.into_iter().collect() }
    }

    /// True when every scripted step was consumed.
    pub fn is_done(&self) -> bool {
        self.script.is_empty()
    }
}

impl Transport for MockTransport {
    fn write(&mut self, data: &[u8], _timeout: Duration) -> Result<(), TransportError> {
        match self.script.pop_front() {
            Some(Step::ExpectWrite(expected)) if expected == data => Ok(()),
            Some(Step::Fail(e)) => Err(e),
            other => panic!("unexpected write of {data:02X?}, script expected {other:?}"),
        }
    }

    fn read(&mut self, _timeout: Duration) -> Result<Vec<u8>, TransportError> {
        match self.script.pop_front() {
            Some(Step::Reply(bytes)) => Ok(bytes),
            Some(Step::Timeout) | None => Err(TransportError::Timeout),
            Some(Step::Fail(e)) => Err(e),
            Some(other) => panic!("unexpected read, script expected {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_millis(1);

    #[test]
    fn replays_script_in_order() {
        let mut m = MockTransport::new([
            Step::ExpectWrite(vec![1, 2]),
            Step::Reply(vec![3]),
            Step::Timeout,
        ]);
        assert_eq!(m.write(&[1, 2], T), Ok(()));
        assert_eq!(m.read(T), Ok(vec![3]));
        assert_eq!(m.read(T), Err(TransportError::Timeout));
        assert!(m.is_done());
    }

    #[test]
    fn read_after_script_end_times_out() {
        let mut m = MockTransport::new([]);
        assert_eq!(m.read(T), Err(TransportError::Timeout));
    }

    #[test]
    fn is_done_false_while_steps_remain() {
        let m = MockTransport::new([Step::Timeout]);
        assert!(!m.is_done());
    }

    #[test]
    fn fail_step_errors_reads_and_writes() {
        let mut m = MockTransport::new([
            Step::Fail(TransportError::Disconnected),
            Step::Fail(TransportError::Usb("x".into())),
        ]);
        assert_eq!(m.read(T), Err(TransportError::Disconnected));
        assert_eq!(m.write(&[1], T), Err(TransportError::Usb("x".into())));
    }

    #[test]
    #[should_panic(expected = "unexpected write")]
    fn wrong_write_bytes_panic() {
        let mut m = MockTransport::new([Step::ExpectWrite(vec![1])]);
        let _ = m.write(&[2], T);
    }

    #[test]
    #[should_panic(expected = "unexpected write")]
    fn write_when_read_expected_panics() {
        let mut m = MockTransport::new([Step::Reply(vec![1])]);
        let _ = m.write(&[1], T);
    }

    #[test]
    #[should_panic(expected = "unexpected read")]
    fn read_when_write_expected_panics() {
        let mut m = MockTransport::new([Step::ExpectWrite(vec![1])]);
        let _ = m.read(T);
    }
}
