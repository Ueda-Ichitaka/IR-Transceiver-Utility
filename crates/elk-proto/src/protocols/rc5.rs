//! Philips RC5 / RC5X.
//!
//! 36 kHz, bit time 1778 us, Manchester with a logic 1 as space then mark;
//! 14 bits MSB first: S1 (1), S2 (inverted command bit 6 in RC5X), toggle,
//! 5 address bits, 6 command bits (SB-Projects).

use super::Decoded;
use super::manchester;
use crate::pulse::Pulse;

pub const HALF_BIT_US: u32 = 889;
pub const BITS: usize = 14;

pub fn decode(frame: &[Pulse]) -> Option<Decoded> {
    let mut levels = vec![false]; // first half of S1 is idle
    levels.extend(manchester::halves(&manchester::runs(frame), HALF_BIT_US, 2)?);
    if levels.len() > 2 * BITS {
        return None;
    }
    levels.resize(2 * BITS, false); // the last half may merge into the gap
    let bits: Vec<u8> = levels
        .chunks(2)
        .map(|half| match half {
            [false, true] => Some(1),
            [true, false] => Some(0),
            _ => None,
        })
        .collect::<Option<_>>()?;
    if bits[0] != 1 {
        return None;
    }
    let value = |range: std::ops::Range<usize>| bits[range].iter().fold(0u8, |acc, b| acc << 1 | b);
    Some(Decoded::Rc5 { address: value(3..8), command: value(8..14) | (1 - bits[1]) << 6, toggle: bits[2] == 1 })
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn decodes_address_command_and_toggle() {
        for (toggle, address, command) in [(false, 0x05, 0x35), (true, 0x1F, 0x00), (false, 0x00, 0x3F)] {
            assert_eq!(
                decode(&rc5_frame(true, toggle, address, command)),
                Some(Decoded::Rc5 { address, command, toggle })
            );
        }
    }

    #[test]
    fn inverted_second_start_bit_is_command_bit_6() {
        assert_eq!(
            decode(&rc5_frame(false, false, 0x05, 0x35)),
            Some(Decoded::Rc5 { address: 0x05, command: 0x75, toggle: false })
        );
    }

    #[test]
    fn rejects_other_shapes() {
        let mut frame = rc5_frame(true, false, 0x05, 0x35);
        frame.pop();
        assert_eq!(decode(&frame), None, "truncated");
        assert_eq!(decode(&rc6_frame(0, false, &bits_msb(0x1234, 16))), None);
        assert_eq!(decode(&samsung_frame(1, 2)), None);
    }
}
