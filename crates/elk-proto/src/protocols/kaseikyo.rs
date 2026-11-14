//! Kaseikyo (Panasonic and other Japanese vendors).
//!
//! Unit 432 us; leader 8 and 4 units, 48 bits LSB first as a 1-unit mark and
//! 1 (0) or 3 (1) unit space, stop mark. Bytes: vendor id (16 bits), vendor
//! parity and genre1, genre2 and data low nibble, data high bits and id,
//! parity of bytes 2-4 (Flipper Zero firmware decoder).

use super::Decoded;
use crate::pulse::Pulse;

pub const UNIT_US: u32 = 432;

pub fn decode(frame: &[Pulse]) -> Option<Decoded> {
    let u = UNIT_US;
    let b = super::distance_bytes::<6>(frame, (8 * u, 4 * u), u, u, 3 * u)?;
    let vendor_xor = b[0] ^ b[1];
    if b[2] & 0x0F != (vendor_xor & 0x0F) ^ (vendor_xor >> 4) || b[5] != b[2] ^ b[3] ^ b[4] {
        return None;
    }
    Some(Decoded::Kaseikyo {
        vendor: u16::from_le_bytes([b[0], b[1]]),
        genre1: b[2] >> 4,
        genre2: b[3] & 0x0F,
        data: u16::from(b[3] >> 4) | u16::from(b[4] & 0x3F) << 4,
        id: b[4] >> 6,
    })
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn decodes_all_fields() {
        assert_eq!(
            decode(&kaseikyo_frame(0x2002, 0x5, 0xA, 0x3FF, 3)),
            Some(Decoded::Kaseikyo { vendor: 0x2002, genre1: 5, genre2: 0xA, data: 0x3FF, id: 3 })
        );
    }

    #[test]
    fn rejects_wrong_parity() {
        let mut frame = kaseikyo_frame(0x2002, 0, 8, 0x3D, 0);
        // Flip the last parity bit (bit 47).
        let bit = frame.len() - 2;
        frame[bit].off_us = if frame[bit].off_us > 1000 { 432 } else { 1296 };
        assert_eq!(decode(&frame), None);
    }
}
