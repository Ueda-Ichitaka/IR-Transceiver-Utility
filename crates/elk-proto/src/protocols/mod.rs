//! Decoders for common remote control protocols.
//!
//! Exists to show learned codes as protocol, address and command (design
//! decision 21). Timings and layouts: RC5, RC6 and Sony SIRC from
//! SB-Projects, Samsung32 and Kaseikyo from the Flipper Zero firmware
//! decoders, scancode layouts from the Linux kernel RC documentation. NEC
//! (including the extended addresses used by LED strip remotes) lives in
//! [`crate::nec`]. Every decoder takes one frame as split by
//! [`crate::frames::split`]; [`encode`] builds frames from decoded values.

pub mod describe;
pub mod encode;
pub mod kaseikyo;
pub mod manchester;
pub mod rc5;
pub mod rc6;
pub mod samsung;
pub mod sirc;
#[cfg(test)]
pub(crate) mod testutil;

use crate::nec::{self, NecFrame};
use crate::pulse::Pulse;
use crate::timing::near;

/// A recognised frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded {
    Nec(NecFrame),
    /// RC5 / RC5X: `command` has 7 bits (bit 6 is the inverted second start bit).
    Rc5 { address: u8, command: u8, toggle: bool },
    /// RC6 in any mode: `bits` data bits after the trailer, MSB first in `data`.
    Rc6 { mode: u8, toggle: bool, bits: u8, data: u32 },
    /// Sony SIRC with 12, 15 or 20 bits; `extended` is 0 unless 20 bits.
    Sirc { bits: u8, command: u8, address: u16, extended: u8 },
    Samsung32 { address: u8, command: u8 },
    /// Kaseikyo (Panasonic and others).
    Kaseikyo { vendor: u16, genre1: u8, genre2: u8, data: u16, id: u8 },
}

/// Tries every decoder on one frame.
pub fn decode(frame: &[Pulse]) -> Option<Decoded> {
    nec::decode(frame)
        .map(Decoded::Nec)
        .or_else(|| samsung::decode(frame))
        .or_else(|| kaseikyo::decode(frame))
        .or_else(|| sirc::decode(frame))
        .or_else(|| rc6::decode(frame))
        .or_else(|| rc5::decode(frame))
}

/// Reads a pulse-distance frame: leader, `N * 8` bits LSB first (each a
/// `mark` and a `zero` or `one` space), stop mark.
pub(crate) fn distance_bytes<const N: usize>(
    frame: &[Pulse],
    leader: (u32, u32),
    mark: u32,
    zero: u32,
    one: u32,
) -> Option<[u8; N]> {
    let (first, rest) = frame.split_first()?;
    let (stop, bits) = rest.split_last()?;
    if bits.len() != N * 8 || !near(first.on_us, leader.0) || !near(first.off_us, leader.1) || !near(stop.on_us, mark) {
        return None;
    }
    let mut bytes = [0u8; N];
    for (i, p) in bits.iter().enumerate() {
        if !near(p.on_us, mark) {
            return None;
        }
        let bit = if near(p.off_us, one) {
            1
        } else if near(p.off_us, zero) {
            0
        } else {
            return None;
        };
        bytes[i / 8] |= bit << (i % 8);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    #[test]
    fn dispatches_to_every_protocol() {
        let cases: Vec<(Vec<Pulse>, Decoded)> = vec![
            (
                nec::encode_command(0x04, 0x08, false),
                Decoded::Nec(NecFrame::Command { address: 0x04, command: 0x08, extended: false }),
            ),
            (rc5_frame(true, false, 0x05, 0x35), Decoded::Rc5 { address: 0x05, command: 0x35, toggle: false }),
            (rc6_frame(0, true, &bits_msb(0x0C0F, 16)), Decoded::Rc6 { mode: 0, toggle: true, bits: 16, data: 0x0C0F }),
            (sirc_frame(12, 0x15, 0x01, 0), Decoded::Sirc { bits: 12, command: 0x15, address: 0x01, extended: 0 }),
            (samsung_frame(0x07, 0x02), Decoded::Samsung32 { address: 0x07, command: 0x02 }),
            (
                kaseikyo_frame(0x2002, 0x0, 0x8, 0x03D, 0),
                Decoded::Kaseikyo { vendor: 0x2002, genre1: 0, genre2: 8, data: 0x03D, id: 0 },
            ),
        ];
        for (frame, expected) in cases {
            assert_eq!(decode(&frame), Some(expected), "{expected:?}");
            assert_eq!(decode(&skew(&frame)), Some(expected), "skewed {expected:?}");
        }
    }

    #[test]
    fn led_strip_remote_is_extended_nec() {
        let frame = nec::encode_command(0xEF00, 0x03, true);
        assert_eq!(decode(&frame), Some(Decoded::Nec(NecFrame::Command { address: 0xEF00, command: 0x03, extended: true })));
    }

    #[test]
    fn noise_is_not_decoded() {
        let p = |on_us, off_us| Pulse { on_us, off_us };
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[p(100, 100), p(5000, 30000)]), None);
        assert_eq!(decode(&[p(3000, 3000); 20]), None);
    }
}
