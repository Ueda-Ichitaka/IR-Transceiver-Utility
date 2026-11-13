//! NEC infrared format: encoding address/command pairs into pulses and
//! decoding received frames.
//!
//! Exists so codes can be sent from an address and command without a learned
//! capture, and learned captures can be shown as address/command. Encoding
//! timings follow `encode_nec()` in elksmart-comm.c: leader 8500/4250 us,
//! bits as 550 us on then 550 (0) or 1650 (1) us off, bytes sent as addr,
//! !addr, cmd, !cmd, then a 550/25000 us stop. Standard NEC sends each byte
//! LSB first; elksmart-comm sends MSB first, so the bit order is selectable.
//! Decoding uses the standard order and accepts extended (16-bit) addresses.

use crate::pulse::Pulse;

/// Order in which the bits of each byte are sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOrder {
    /// Standard NEC (remotes, Linux, IRremote).
    LsbFirst,
    /// What elksmart-comm's `-n` option sends.
    MsbFirst,
}

/// Why NEC input was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NecError {
    /// Input must be (address, command) pairs.
    OddLength,
}

/// A decoded NEC frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NecFrame {
    /// A full frame. `extended` is set when the second address byte is not
    /// the complement of the first; `address` then holds both bytes
    /// (second byte in the high half).
    Command { address: u16, command: u8, extended: bool },
    /// The short repeat frame sent while a button is held.
    Repeat,
}

/// Allowed deviation from nominal timings, in percent.
pub const TOLERANCE_PERCENT: u32 = 30;

/// Encodes one frame per (address, command) pair in `pairs`.
pub fn encode(pairs: &[u8], order: BitOrder) -> Result<Vec<Pulse>, NecError> {
    if pairs.len() % 2 != 0 {
        return Err(NecError::OddLength);
    }
    let mut pulses = Vec::with_capacity(pairs.len() / 2 * 34);
    for pair in pairs.chunks_exact(2) {
        let (addr, cmd) = (pair[0], pair[1]);
        pulses.push(Pulse { on_us: 8500, off_us: 4250 });
        for byte in [addr, !addr, cmd, !cmd] {
            let byte = match order {
                BitOrder::MsbFirst => byte,
                BitOrder::LsbFirst => byte.reverse_bits(),
            };
            for bit in (0..8).rev() {
                let off_us = if byte >> bit & 1 == 1 { 1650 } else { 550 };
                pulses.push(Pulse { on_us: 550, off_us });
            }
        }
        pulses.push(Pulse { on_us: 550, off_us: 25000 });
    }
    Ok(pulses)
}

/// Decodes one frame (as split by [`crate::frames::split`]). The off-time of
/// the last pulse is the gap after the frame and is not checked.
pub fn decode(frame: &[Pulse]) -> Option<NecFrame> {
    let (leader, rest) = frame.split_first()?;
    if !near(leader.on_us, 9000) {
        return None;
    }
    match rest {
        [stop] if near(leader.off_us, 2250) && near(stop.on_us, 560) => Some(NecFrame::Repeat),
        [bits @ .., stop] if bits.len() == 32 && near(leader.off_us, 4500) && near(stop.on_us, 560) => {
            let mut bytes = [0u8; 4];
            for (i, bit) in bits.iter().enumerate() {
                if !near(bit.on_us, 560) {
                    return None;
                }
                let one = if near(bit.off_us, 1690) {
                    true
                } else if near(bit.off_us, 560) {
                    false
                } else {
                    return None;
                };
                bytes[i / 8] |= u8::from(one) << (i % 8);
            }
            let [a0, a1, command, command_inv] = bytes;
            if command ^ command_inv != 0xFF {
                return None;
            }
            let extended = a0 ^ a1 != 0xFF;
            let address = if extended { u16::from_le_bytes([a0, a1]) } else { u16::from(a0) };
            Some(NecFrame::Command { address, command, extended })
        }
        _ => None,
    }
}

/// True when `value` is within [`TOLERANCE_PERCENT`] of `nominal`.
fn near(value: u32, nominal: u32) -> bool {
    let (value, nominal) = (u64::from(value) * 100, u64::from(nominal));
    value >= nominal * u64::from(100 - TOLERANCE_PERCENT) && value <= nominal * u64::from(100 + TOLERANCE_PERCENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    fn bytes_msb(pulses: &[Pulse]) -> [u8; 4] {
        let bits: Vec<u8> = pulses[1..33].iter().map(|p| u8::from(p.off_us == 1650)).collect();
        let byte = |k: usize| bits[k * 8..k * 8 + 8].iter().fold(0u8, |acc, b| acc << 1 | b);
        [byte(0), byte(1), byte(2), byte(3)]
    }

    #[test]
    fn frame_has_leader_32_bits_and_stop() {
        let pulses = encode(&[0x00, 0x59], BitOrder::MsbFirst).unwrap();
        assert_eq!(pulses.len(), 34);
        assert_eq!(pulses[0], p(8500, 4250));
        assert_eq!(pulses[33], p(550, 25000));
    }

    #[test]
    fn msb_first_bits_with_complements() {
        let pulses = encode(&[0x80, 0x01], BitOrder::MsbFirst).unwrap();
        assert_eq!(bytes_msb(&pulses), [0x80, 0x7F, 0x01, 0xFE]);
    }

    #[test]
    fn lsb_first_reverses_each_byte_on_the_wire() {
        let pulses = encode(&[0x80, 0x01], BitOrder::LsbFirst).unwrap();
        assert_eq!(bytes_msb(&pulses), [0x01, 0xFE, 0x80, 0x7F]);
    }

    #[test]
    fn several_pairs_give_several_frames() {
        assert_eq!(encode(&[1, 2, 3, 4], BitOrder::LsbFirst).unwrap().len(), 68);
    }

    #[test]
    fn odd_input_is_rejected() {
        assert_eq!(encode(&[1], BitOrder::LsbFirst), Err(NecError::OddLength));
    }

    #[test]
    fn decode_round_trips_standard_encoding() {
        let pulses = encode(&[0x00, 0x59], BitOrder::LsbFirst).unwrap();
        assert_eq!(
            decode(&pulses),
            Some(NecFrame::Command { address: 0x00, command: 0x59, extended: false })
        );
    }

    #[test]
    fn msb_encoding_decodes_bit_reversed() {
        let pulses = encode(&[0x00, 0x59], BitOrder::MsbFirst).unwrap();
        assert_eq!(
            decode(&pulses),
            Some(NecFrame::Command { address: 0x00, command: 0x9A, extended: false })
        );
    }

    /// Builds a standard frame from four raw bytes with the given bit timings.
    fn raw_frame(bytes: [u8; 4], mark: u32, zero: u32, one: u32) -> Vec<Pulse> {
        let mut f = vec![p(9000, 4500)];
        for b in bytes {
            for i in 0..8 {
                f.push(p(mark, if b >> i & 1 == 1 { one } else { zero }));
            }
        }
        f.push(p(mark, 40000));
        f
    }

    #[test]
    fn decode_tolerates_dongle_skew() {
        // Timings measured on the 0132: marks 608 us, spaces 512/1616 us.
        let f = raw_frame([0x00, 0xFF, 0x59, 0xA6], 608, 512, 1616);
        assert_eq!(decode(&f), Some(NecFrame::Command { address: 0x00, command: 0x59, extended: false }));
    }

    #[test]
    fn decode_extended_address() {
        let f = raw_frame([0x12, 0x34, 0x59, 0xA6], 560, 560, 1690);
        assert_eq!(decode(&f), Some(NecFrame::Command { address: 0x3412, command: 0x59, extended: true }));
    }

    #[test]
    fn decode_rejects_bad_command_complement() {
        let f = raw_frame([0x00, 0xFF, 0x59, 0xA7], 560, 560, 1690);
        assert_eq!(decode(&f), None);
    }

    #[test]
    fn decode_repeat_frame() {
        assert_eq!(decode(&[p(9056, 2208), p(608, 96096)]), Some(NecFrame::Repeat));
    }

    #[test]
    fn decode_rejects_wrong_shapes() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[p(9000, 4500), p(560, 560)]), None);
        let mut f = raw_frame([0, 0xFF, 1, 0xFE], 560, 560, 1690);
        f[0] = p(4500, 4500);
        assert_eq!(decode(&f), None, "leader too short");
        let mut f = raw_frame([0, 0xFF, 1, 0xFE], 560, 560, 1690);
        f[5] = p(560, 1100);
        assert_eq!(decode(&f), None, "space between 0 and 1");
    }
}
