//! Classifies the dongle's reply to the identify command.
//!
//! Exists because the reply decides how transmit payloads are encoded
//! (plain or Huffman). Mirrors `classify_device()` in elksmart-comm.c.

/// Protocol variant the dongle speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Plain,
    Huffman,
}

/// Returns the protocol for a raw identify reply, or `None` if the reply is
/// not recognised (the reference driver reports "device busy or not supported").
pub fn classify(reply: &[u8]) -> Option<Protocol> {
    match reply {
        [0xFC, 0xFC, 0xFC, 0xFC, 0x02, 0xAA] => Some(Protocol::Huffman),
        [0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01] => Some(Protocol::Plain),
        [0xFA, 0xFA, 0xFA, 0xFA, _, _] => Some(Protocol::Plain),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fc_70_01_is_plain() {
        assert_eq!(classify(&[0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01]), Some(Protocol::Plain));
    }

    #[test]
    fn fc_02_aa_is_huffman() {
        assert_eq!(classify(&[0xFC, 0xFC, 0xFC, 0xFC, 0x02, 0xAA]), Some(Protocol::Huffman));
    }

    #[test]
    fn fa_prefix_is_plain_regardless_of_tail() {
        assert_eq!(classify(&[0xFA, 0xFA, 0xFA, 0xFA, 0x12, 0x34]), Some(Protocol::Plain));
    }

    #[test]
    fn fc_with_unknown_tail_is_unknown() {
        assert_eq!(classify(&[0xFC, 0xFC, 0xFC, 0xFC, 0x00, 0x00]), None);
    }

    #[test]
    fn length_must_be_exactly_six() {
        assert_eq!(classify(&[0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01, 0x00]), None);
        assert_eq!(classify(&[0xFC, 0xFC, 0xFC, 0xFC, 0x70]), None);
        assert_eq!(classify(&[]), None);
    }
}
