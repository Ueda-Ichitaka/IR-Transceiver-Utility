//! Test-only builders for protocol frames from published nominal timings.
//!
//! Exists so decoder tests describe frames by their fields instead of raw
//! durations, and can add the 0132's measured skew.

use crate::pulse::Pulse;

/// Gap after a frame.
pub const GAP: u32 = 100_000;

/// Merges (level, duration) runs into pulses: leading spaces are dropped
/// (idle line), a trailing space becomes the gap.
pub fn levels_to_pulses(levels: &[(bool, u32)]) -> Vec<Pulse> {
    let mut runs: Vec<(bool, u32)> = Vec::new();
    for &(level, d) in levels.iter().skip_while(|(level, _)| !level) {
        match runs.last_mut() {
            Some((l, total)) if *l == level => *total += d,
            _ => runs.push((level, d)),
        }
    }
    let mut pulses: Vec<Pulse> = runs
        .chunks(2)
        .map(|r| Pulse { on_us: r[0].1, off_us: r.get(1).map_or(GAP, |space| space.1) })
        .collect();
    if let Some(last) = pulses.last_mut() {
        last.off_us = GAP;
    }
    pulses
}

/// Bits of `value`, MSB first.
pub fn bits_msb(value: u32, count: u8) -> Vec<bool> {
    (0..count).rev().map(|i| value >> i & 1 == 1).collect()
}

/// Bits of `value`, LSB first.
pub fn bits_lsb(value: u32, count: u8) -> Vec<bool> {
    (0..count).map(|i| value >> i & 1 == 1).collect()
}

/// RC5: S1, S2, toggle, 5 address bits, 6 command bits; 1 = space then mark.
pub fn rc5_frame(s2: bool, toggle: bool, address: u8, command: u8) -> Vec<Pulse> {
    const HALF: u32 = 889;
    let mut bits = vec![true, s2, toggle];
    bits.extend(bits_msb(u32::from(address), 5));
    bits.extend(bits_msb(u32::from(command), 6));
    let levels: Vec<(bool, u32)> =
        bits.iter().flat_map(|&b| [(!b, HALF), (b, HALF)]).collect();
    levels_to_pulses(&levels)
}

/// RC6: leader 6t/2t, start bit 1, 3 mode bits, double-width trailer
/// (toggle), data bits; 1 = mark then space; t = 444 us.
pub fn rc6_frame(mode: u8, toggle: bool, data: &[bool]) -> Vec<Pulse> {
    const T: u32 = 444;
    let mut levels = vec![(true, 6 * T), (false, 2 * T)];
    let bit = |levels: &mut Vec<(bool, u32)>, b: bool, len: u32| {
        levels.push((b, len));
        levels.push((!b, len));
    };
    bit(&mut levels, true, T);
    for b in bits_msb(u32::from(mode), 3) {
        bit(&mut levels, b, T);
    }
    bit(&mut levels, toggle, 2 * T);
    for &b in data {
        bit(&mut levels, b, T);
    }
    levels_to_pulses(&levels)
}

/// Sony SIRC: header 2400/600, bits LSB first, 1 = 1200 mark, 0 = 600 mark,
/// 600 space.
pub fn sirc_frame(bits: u8, command: u8, address: u16, extended: u8) -> Vec<Pulse> {
    let address_bits = if bits == 15 { 8 } else { 5 };
    let mut data = bits_lsb(u32::from(command), 7);
    data.extend(bits_lsb(u32::from(address), address_bits));
    if bits == 20 {
        data.extend(bits_lsb(u32::from(extended), 8));
    }
    let mut pulses = vec![Pulse { on_us: 2400, off_us: 600 }];
    pulses.extend(data.iter().map(|&b| Pulse { on_us: if b { 1200 } else { 600 }, off_us: 600 }));
    pulses.last_mut().unwrap().off_us = 25_000;
    pulses
}

/// Pulse-distance frame: leader, bytes LSB first, stop mark.
pub fn distance_frame(leader: (u32, u32), mark: u32, zero: u32, one: u32, bytes: &[u8]) -> Vec<Pulse> {
    let mut pulses = vec![Pulse { on_us: leader.0, off_us: leader.1 }];
    for &byte in bytes {
        for b in bits_lsb(u32::from(byte), 8) {
            pulses.push(Pulse { on_us: mark, off_us: if b { one } else { zero } });
        }
    }
    pulses.push(Pulse { on_us: mark, off_us: GAP });
    pulses
}

/// Samsung32: 4500/4500, address, address, command, !command; 550 / 550|1650.
pub fn samsung_frame(address: u8, command: u8) -> Vec<Pulse> {
    distance_frame((4500, 4500), 550, 550, 1650, &[address, address, command, !command])
}

/// Kaseikyo: 3456/1728, 48 bits, 432 / 432|1296, byte layout of the Flipper
/// decoder with both parities computed.
pub fn kaseikyo_frame(vendor: u16, genre1: u8, genre2: u8, data: u16, id: u8) -> Vec<Pulse> {
    let [v0, v1] = vendor.to_le_bytes();
    let vendor_parity = (v0 ^ v1) & 0x0F ^ (v0 ^ v1) >> 4;
    let b2 = vendor_parity | genre1 << 4;
    let b3 = genre2 | ((data & 0x0F) as u8) << 4;
    let b4 = ((data >> 4) & 0x3F) as u8 | id << 6;
    distance_frame((3456, 1728), 432, 432, 1296, &[v0, v1, b2, b3, b4, b2 ^ b3 ^ b4])
}

/// The 0132's skew: marks 48 us longer, spaces 48 us shorter (not the gap).
pub fn skew(frame: &[Pulse]) -> Vec<Pulse> {
    let last = frame.len().saturating_sub(1);
    frame
        .iter()
        .enumerate()
        .map(|(i, p)| Pulse { on_us: p.on_us + 48, off_us: if i == last { p.off_us } else { p.off_us - 48 } })
        .collect()
}
