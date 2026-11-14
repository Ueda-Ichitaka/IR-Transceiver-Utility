//! Sony SIRC with 12, 15 or 20 bits.
//!
//! 40 kHz; header 2400 us mark and 600 us space; bits LSB first as a 1200 us
//! (1) or 600 us (0) mark and a 600 us space; 7 command bits, then 5 address
//! bits (12-bit), 8 address bits (15-bit) or 5 address and 8 extended bits
//! (20-bit) (SB-Projects).

use super::Decoded;
use crate::pulse::Pulse;
use crate::timing::near;

pub fn decode(frame: &[Pulse]) -> Option<Decoded> {
    let (header, bits) = frame.split_first()?;
    let count = bits.len();
    if !matches!(count, 12 | 15 | 20) || !near(header.on_us, 2400) || !near(header.off_us, 600) {
        return None;
    }
    let mut value: u32 = 0;
    for (i, p) in bits.iter().enumerate() {
        let one = if near(p.on_us, 1200) {
            1
        } else if near(p.on_us, 600) {
            0
        } else {
            return None;
        };
        if i + 1 < count && !near(p.off_us, 600) {
            return None;
        }
        value |= one << i;
    }
    let (address, extended) = match count {
        15 => ((value >> 7) & 0xFF, 0),
        20 => ((value >> 7) & 0x1F, (value >> 12) & 0xFF),
        _ => ((value >> 7) & 0x1F, 0),
    };
    Some(Decoded::Sirc { bits: count as u8, command: (value & 0x7F) as u8, address: address as u16, extended: extended as u8 })
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn decodes_all_lengths() {
        for (bits, command, address, extended) in [(12, 0x15, 0x01, 0), (15, 0x7F, 0xA4, 0), (20, 0x0B, 0x1A, 0xC3)] {
            assert_eq!(
                decode(&sirc_frame(bits, command, address, extended)),
                Some(Decoded::Sirc { bits, command, address, extended })
            );
        }
    }

    #[test]
    fn rejects_other_lengths_and_protocols() {
        let mut frame = sirc_frame(12, 1, 1, 0);
        frame.pop();
        assert_eq!(decode(&frame), None, "11 bits");
        assert_eq!(decode(&samsung_frame(1, 2)), None);
    }
}
