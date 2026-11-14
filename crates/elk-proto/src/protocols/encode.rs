//! Encoders for the recognised protocols: the inverse of [`super::decode`].
//!
//! Exists so codes imported as protocol, address and command (Flipper parsed
//! signals, typed codes) can be sent. Frames use the nominal timings of the
//! decoders; the final space is the gap to the next frame, derived from the
//! published repeat periods: RC5 114 ms, SIRC 45 ms (start to start), Samsung
//! 46 ms pause, Kaseikyo 130 ms, NEC as in elksmart-comm (25 ms) with repeat
//! frames every 108 ms. RC6 publishes only a minimum signal-free time (6t =
//! 2.7 ms); it gets the frame splitter's 10 ms so repeated frames stay
//! separable.

use super::{Decoded, kaseikyo, rc5, rc6};
use crate::frames;
use crate::nec::{self, NecFrame};
use crate::pulse::Pulse;

/// Encodes one frame.
pub fn encode(frame: &Decoded) -> Vec<Pulse> {
    match *frame {
        Decoded::Nec(NecFrame::Command { address, command, extended }) => {
            nec::encode_command(address, command, extended)
        }
        Decoded::Nec(NecFrame::Repeat) => {
            with_period(vec![Pulse { on_us: 9000, off_us: 2250 }, Pulse { on_us: 560, off_us: 0 }], 108_000)
        }
        Decoded::Rc5 { address, command, toggle } => {
            let mut bits = vec![true, command & 0x40 == 0, toggle];
            bits.extend(msb(u32::from(address), 5));
            bits.extend(msb(u32::from(command & 0x3F), 6));
            let levels: Vec<(bool, u32)> =
                bits.iter().flat_map(|&b| [(!b, rc5::HALF_BIT_US), (b, rc5::HALF_BIT_US)]).collect();
            // The idle first half of S1 belongs to the 114 ms period.
            with_period(manchester_pulses(&levels), 114_000 - rc5::HALF_BIT_US)
        }
        Decoded::Rc6 { mode, toggle, bits, data } => {
            let t = rc6::T_US;
            let mut levels = vec![(true, 6 * t), (false, 2 * t)];
            let mut bit = |b: bool, len: u32| levels.extend([(b, len), (!b, len)]);
            bit(true, t);
            msb(u32::from(mode), 3).for_each(|b| bit(b, t));
            bit(toggle, 2 * t);
            msb(data, bits).for_each(|b| bit(b, t));
            let mut pulses = manchester_pulses(&levels);
            if let Some(last) = pulses.last_mut() {
                last.off_us = frames::DEFAULT_GAP_US;
            }
            pulses
        }
        Decoded::Sirc { bits, command, address, extended } => {
            let address_bits = if bits == 15 { 8 } else { 5 };
            let mut value = u32::from(command & 0x7F) | u32::from(address) << 7;
            if bits == 20 {
                value |= u32::from(extended) << 12;
            }
            value &= (1 << (7 + address_bits + if bits == 20 { 8 } else { 0 })) - 1;
            let mut pulses = vec![Pulse { on_us: 2400, off_us: 600 }];
            pulses.extend((0..bits).map(|i| Pulse { on_us: if value >> i & 1 == 1 { 1200 } else { 600 }, off_us: 600 }));
            with_period(pulses, 45_000)
        }
        Decoded::Samsung32 { address, command } => {
            let mut pulses = distance((4500, 4500), 550, 550, 1650, &[address, address, command, !command]);
            if let Some(last) = pulses.last_mut() {
                last.off_us = 46_000;
            }
            pulses
        }
        Decoded::Kaseikyo { vendor, genre1, genre2, data, id } => {
            let [v0, v1] = vendor.to_le_bytes();
            let vendor_parity = ((v0 ^ v1) & 0x0F) ^ ((v0 ^ v1) >> 4);
            let b2 = vendor_parity | (genre1 & 0x0F) << 4;
            let b3 = (genre2 & 0x0F) | ((data & 0x0F) as u8) << 4;
            let b4 = ((data >> 4) & 0x3F) as u8 | (id & 0x03) << 6;
            let u = kaseikyo::UNIT_US;
            let pulses = distance((8 * u, 4 * u), u, u, 3 * u, &[v0, v1, b2, b3, b4, b2 ^ b3 ^ b4]);
            with_period(pulses, 130_000)
        }
    }
}

/// Bits of `value`, MSB first.
fn msb(value: u32, count: u8) -> impl Iterator<Item = bool> {
    (0..count).rev().map(move |i| value >> i & 1 == 1)
}

/// Pulses from half-bit level runs; leading spaces (idle line) are dropped.
fn manchester_pulses(levels: &[(bool, u32)]) -> Vec<Pulse> {
    let mut runs: Vec<(bool, u32)> = Vec::new();
    for &(level, d) in levels.iter().skip_while(|(level, _)| !level) {
        match runs.last_mut() {
            Some((l, total)) if *l == level => *total += d,
            _ => runs.push((level, d)),
        }
    }
    runs.chunks(2).map(|r| Pulse { on_us: r[0].1, off_us: r.get(1).map_or(0, |space| space.1) }).collect()
}

/// Pulse-distance frame: leader, bytes LSB first, stop mark (gap 0).
fn distance(leader: (u32, u32), mark: u32, zero: u32, one: u32, bytes: &[u8]) -> Vec<Pulse> {
    let mut pulses = vec![Pulse { on_us: leader.0, off_us: leader.1 }];
    for &byte in bytes {
        pulses.extend((0..8).map(|i| Pulse { on_us: mark, off_us: if byte >> i & 1 == 1 { one } else { zero } }));
    }
    pulses.push(Pulse { on_us: mark, off_us: 0 });
    pulses
}

/// Sets the final gap so the frame lasts `period_us` from start to start.
fn with_period(mut pulses: Vec<Pulse>, period_us: u32) -> Vec<Pulse> {
    let busy: u32 = pulses.iter().map(|p| p.on_us + p.off_us).sum::<u32>() - pulses.last().map_or(0, |p| p.off_us);
    if let Some(last) = pulses.last_mut() {
        last.off_us = period_us.saturating_sub(busy).max(frames::DEFAULT_GAP_US);
    }
    pulses
}

#[cfg(test)]
mod tests {
    use super::super::decode;
    use super::*;

    fn round_trip(d: Decoded) {
        let pulses = encode(&d);
        assert_eq!(decode(&pulses), Some(d), "{d:?}");
    }

    #[test]
    fn nec_round_trips() {
        round_trip(Decoded::Nec(NecFrame::Command { address: 0x04, command: 0x08, extended: false }));
        round_trip(Decoded::Nec(NecFrame::Command { address: 0xEF00, command: 0x03, extended: true }));
        round_trip(Decoded::Nec(NecFrame::Repeat));
    }

    #[test]
    fn rc5_round_trips() {
        for address in [0, 5, 31] {
            for command in [0, 0x35, 0x3F, 0x40, 0x7F] {
                for toggle in [false, true] {
                    round_trip(Decoded::Rc5 { address, command, toggle });
                }
            }
        }
    }

    #[test]
    fn rc6_round_trips() {
        for (mode, bits, data) in [(0, 16, 0x0C0F), (0, 16, 0), (6, 32, 0x800F_0410), (6, 20, 0xFFFFF), (6, 24, 0x123456)] {
            for toggle in [false, true] {
                round_trip(Decoded::Rc6 { mode, toggle, bits, data });
            }
        }
    }

    #[test]
    fn sirc_round_trips() {
        round_trip(Decoded::Sirc { bits: 12, command: 0x15, address: 0x01, extended: 0 });
        round_trip(Decoded::Sirc { bits: 15, command: 0x7F, address: 0xA4, extended: 0 });
        round_trip(Decoded::Sirc { bits: 20, command: 0x0B, address: 0x1A, extended: 0xC3 });
    }

    #[test]
    fn samsung_and_kaseikyo_round_trip() {
        round_trip(Decoded::Samsung32 { address: 0x07, command: 0x02 });
        round_trip(Decoded::Kaseikyo { vendor: 0x2002, genre1: 5, genre2: 0xA, data: 0x3FF, id: 3 });
    }

    #[test]
    fn sirc_frames_repeat_every_45_ms() {
        let pulses = encode(&Decoded::Sirc { bits: 12, command: 0x15, address: 0x01, extended: 0 });
        let total: u32 = pulses.iter().map(|p| p.on_us + p.off_us).sum();
        assert_eq!(total, 45_000);
    }

    #[test]
    fn rc5_frames_repeat_every_114_ms() {
        let pulses = encode(&Decoded::Rc5 { address: 5, command: 0x35, toggle: false });
        let total: u32 = pulses.iter().map(|p| p.on_us + p.off_us).sum();
        // The idle first half of S1 is not part of the pulses.
        assert_eq!(total + 889, 114_000);
    }
}
