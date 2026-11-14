//! Philips RC6 (all modes).
//!
//! 36 kHz, t = 444 us; leader 6t mark and 2t space, start bit 1, 3 mode bits,
//! trailer (toggle) bit of double width, then the data bits; Manchester with
//! a logic 1 as mark then space, MSB first (SB-Projects). Mode 0 has 16 data
//! bits, mode 6 (6A, MCE) 20, 24 or 32 (Linux RC docs).

use super::Decoded;
use super::manchester;
use crate::pulse::Pulse;

pub const T_US: u32 = 444;

pub fn decode(frame: &[Pulse]) -> Option<Decoded> {
    use crate::timing::near;
    let runs = manchester::runs(frame);
    let [(true, leader_mark), (false, leader_space), rest @ ..] = runs.as_slice() else {
        return None;
    };
    if !near(*leader_mark, 6 * T_US) || !near(*leader_space, 2 * T_US) {
        return None;
    }
    let mut h = manchester::halves(rest, T_US, 3)?;
    if h.len() % 2 == 1 {
        h.push(false); // a final logic 1 ends with space, merged into the gap
    }
    let bit = |pair: &[bool]| match pair {
        [true, false] => Some(1u32),
        [false, true] => Some(0u32),
        _ => None,
    };
    if h.len() < 14 || bit(&h[0..2])? != 1 {
        return None;
    }
    let mode = h[2..8].chunks(2).map(bit).try_fold(0u32, |acc, b| Some(acc << 1 | b?))?;
    let toggle = match &h[8..12] {
        [true, true, false, false] => true,
        [false, false, true, true] => false,
        _ => return None,
    };
    let data_halves = &h[12..];
    if data_halves.len() > 64 {
        return None;
    }
    let data = data_halves.chunks(2).map(bit).try_fold(0u32, |acc, b| Some(acc << 1 | b?))?;
    Some(Decoded::Rc6 { mode: mode as u8, toggle, bits: (data_halves.len() / 2) as u8, data })
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::*;

    #[test]
    fn decodes_mode_0() {
        for (toggle, data) in [(false, 0x0C0F), (true, 0x0000), (false, 0xFFFF)] {
            assert_eq!(
                decode(&rc6_frame(0, toggle, &bits_msb(data, 16))),
                Some(Decoded::Rc6 { mode: 0, toggle, bits: 16, data })
            );
        }
    }

    #[test]
    fn decodes_mode_6_with_32_bits() {
        assert_eq!(
            decode(&rc6_frame(6, false, &bits_msb(0x800F_0410, 32))),
            Some(Decoded::Rc6 { mode: 6, toggle: false, bits: 32, data: 0x800F_0410 })
        );
    }

    #[test]
    fn rejects_other_shapes() {
        assert_eq!(decode(&rc5_frame(true, false, 1, 2)), None);
        let mut frame = rc6_frame(0, false, &bits_msb(0x1234, 16));
        frame[0].on_us = 1000;
        assert_eq!(decode(&frame), None, "short leader");
    }
}
