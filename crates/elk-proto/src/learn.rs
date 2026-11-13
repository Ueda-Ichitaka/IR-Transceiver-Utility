//! Reassembles a learned IR code from the dongle's bulk IN transfers.
//!
//! Exists because a learn reply arrives in several 64-byte transfers:
//! `FE FE FE FE <u16 BE announced length> <data...>` followed by more data
//! transfers. The 0132 additionally sends a `FF FF FF FF` acknowledgement
//! before the reply, which is skipped here. The announced length does not
//! always match what arrives, so a caller that times out can still take the
//! partial code. Memory is bounded by the u16 length field: a code completes
//! at most one 64-byte transfer past 65535 bytes.

use crate::command;

/// Acknowledgement the 0132 sends before a learn reply.
pub const LEARN_ACK: [u8; 4] = [0xFF; 4];

/// A learned code as received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedCode {
    /// Length announced in the reply header.
    pub announced_len: usize,
    /// Data bytes after the header (may differ in length from `announced_len`).
    pub data: Vec<u8>,
}

/// Result of feeding one transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// The transfer was only the learn acknowledgement.
    Acknowledged,
    /// Header seen, more data expected.
    Receiving { have: usize, announced: usize },
    /// The code is complete; the assembler is ready for the next learn.
    Complete(LearnedCode),
}

/// Why a transfer could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnError {
    /// First transfer is not a learn reply header; carries the bytes.
    UnexpectedResponse(Vec<u8>),
}

/// Stateful reassembler for one learn reply at a time.
#[derive(Debug, Default)]
pub struct LearnAssembler {
    announced: Option<usize>,
    data: Vec<u8>,
}

impl LearnAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one bulk IN transfer.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Progress, LearnError> {
        let announced = match self.announced {
            Some(announced) => {
                self.data.extend_from_slice(chunk);
                announced
            }
            None => {
                let rest = chunk.strip_prefix(&LEARN_ACK[..]).unwrap_or(chunk);
                if rest.is_empty() {
                    return Ok(Progress::Acknowledged);
                }
                if rest.len() < 6 || rest[..4] != command::LEARN {
                    return Err(LearnError::UnexpectedResponse(chunk.to_vec()));
                }
                let announced = usize::from(u16::from_be_bytes([rest[4], rest[5]]));
                self.announced = Some(announced);
                self.data.extend_from_slice(&rest[6..]);
                announced
            }
        };
        if self.data.len() >= announced {
            Ok(Progress::Complete(self.take_partial().expect("header was seen")))
        } else {
            Ok(Progress::Receiving { have: self.data.len(), announced })
        }
    }

    /// Takes whatever was received after a header, for use after a timeout.
    /// Returns `None` if no header was seen. Resets the assembler.
    pub fn take_partial(&mut self) -> Option<LearnedCode> {
        let announced_len = self.announced.take()?;
        Some(LearnedCode { announced_len, data: std::mem::take(&mut self.data) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACK: [u8; 4] = LEARN_ACK;

    fn header(len: u16) -> Vec<u8> {
        let mut h = vec![0xFE, 0xFE, 0xFE, 0xFE];
        h.extend_from_slice(&len.to_be_bytes());
        h
    }

    fn cat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn ack_is_skipped() {
        let mut a = LearnAssembler::new();
        assert_eq!(a.push(&ACK), Ok(Progress::Acknowledged));
    }

    #[test]
    fn single_transfer_completes() {
        let mut a = LearnAssembler::new();
        let r = a.push(&cat(&[&header(3), &[1, 2, 3]]));
        assert_eq!(
            r,
            Ok(Progress::Complete(LearnedCode { announced_len: 3, data: vec![1, 2, 3] }))
        );
    }

    #[test]
    fn ack_then_header_in_same_transfer() {
        let mut a = LearnAssembler::new();
        let r = a.push(&cat(&[&ACK, &header(2), &[7, 8]]));
        assert_eq!(
            r,
            Ok(Progress::Complete(LearnedCode { announced_len: 2, data: vec![7, 8] }))
        );
    }

    #[test]
    fn split_transfers_reassemble_with_progress() {
        let mut a = LearnAssembler::new();
        assert_eq!(a.push(&ACK), Ok(Progress::Acknowledged));
        assert_eq!(
            a.push(&cat(&[&header(5), &[1, 2]])),
            Ok(Progress::Receiving { have: 2, announced: 5 })
        );
        assert_eq!(a.push(&[3]), Ok(Progress::Receiving { have: 3, announced: 5 }));
        assert_eq!(
            a.push(&[4, 5]),
            Ok(Progress::Complete(LearnedCode { announced_len: 5, data: vec![1, 2, 3, 4, 5] }))
        );
    }

    #[test]
    fn overshoot_keeps_all_received_bytes() {
        let mut a = LearnAssembler::new();
        a.push(&cat(&[&header(3), &[1]])).unwrap();
        assert_eq!(
            a.push(&[2, 3, 4]),
            Ok(Progress::Complete(LearnedCode { announced_len: 3, data: vec![1, 2, 3, 4] }))
        );
    }

    #[test]
    fn zero_announced_length_completes_immediately() {
        let mut a = LearnAssembler::new();
        assert_eq!(
            a.push(&header(0)),
            Ok(Progress::Complete(LearnedCode { announced_len: 0, data: vec![] }))
        );
    }

    #[test]
    fn non_learn_header_is_unexpected() {
        let mut a = LearnAssembler::new();
        let bad = [0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01];
        assert_eq!(a.push(&bad), Err(LearnError::UnexpectedResponse(bad.to_vec())));
    }

    #[test]
    fn header_shorter_than_six_bytes_is_unexpected() {
        let mut a = LearnAssembler::new();
        let bad = [0xFE, 0xFE, 0xFE, 0xFE, 0x00];
        assert_eq!(a.push(&bad), Err(LearnError::UnexpectedResponse(bad.to_vec())));
    }

    #[test]
    fn take_partial_returns_received_data_and_resets() {
        let mut a = LearnAssembler::new();
        a.push(&cat(&[&header(10), &[1, 2, 3]])).unwrap();
        assert_eq!(
            a.take_partial(),
            Some(LearnedCode { announced_len: 10, data: vec![1, 2, 3] })
        );
        assert_eq!(a.take_partial(), None);
    }

    #[test]
    fn take_partial_before_header_is_none() {
        let mut a = LearnAssembler::new();
        a.push(&ACK).unwrap();
        assert_eq!(a.take_partial(), None);
    }

    #[test]
    fn assembler_is_reusable_after_complete() {
        let mut a = LearnAssembler::new();
        a.push(&cat(&[&header(1), &[9]])).unwrap();
        assert_eq!(
            a.push(&cat(&[&header(1), &[8]])),
            Ok(Progress::Complete(LearnedCode { announced_len: 1, data: vec![8] }))
        );
    }
}
