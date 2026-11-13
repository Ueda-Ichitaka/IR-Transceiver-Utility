//! NEC infrared format: encoding address/command pairs into pulses.
//!
//! Exists so codes can be sent from an address and command without a learned
//! capture. Timings and bit order follow `encode_nec()` in elksmart-comm.c:
//! leader 8500/4250 us, bits MSB first as 550 us on then 550 (0) or 1650 (1)
//! us off, bytes sent as addr, !addr, cmd, !cmd, then a 550/25000 us stop.

use crate::pulse::Pulse;

/// Why NEC input was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NecError {
    /// Input must be (address, command) pairs.
    OddLength,
}

/// Encodes one frame per (address, command) pair in `pairs`.
pub fn encode(pairs: &[u8]) -> Result<Vec<Pulse>, NecError> {
    if !pairs.len().is_multiple_of(2) {
        return Err(NecError::OddLength);
    }
    let mut pulses = Vec::with_capacity(pairs.len() / 2 * 34);
    for &[addr, cmd] in pairs.as_chunks::<2>().0 {
        pulses.push(Pulse { on_us: 8500, off_us: 4250 });
        for byte in [addr, !addr, cmd, !cmd] {
            for bit in (0..8).rev() {
                let off_us = if byte >> bit & 1 == 1 { 1650 } else { 550 };
                pulses.push(Pulse { on_us: 550, off_us });
            }
        }
        pulses.push(Pulse { on_us: 550, off_us: 25000 });
    }
    Ok(pulses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_has_leader_32_bits_and_stop() {
        let pulses = encode(&[0x00, 0x59]).unwrap();
        assert_eq!(pulses.len(), 34);
        assert_eq!(pulses[0], Pulse { on_us: 8500, off_us: 4250 });
        assert_eq!(pulses[33], Pulse { on_us: 550, off_us: 25000 });
    }

    #[test]
    fn bits_are_msb_first_with_complements() {
        let pulses = encode(&[0x80, 0x01]).unwrap();
        let bits: Vec<u8> = pulses[1..33].iter().map(|p| u8::from(p.off_us == 1650)).collect();
        let byte = |k: usize| bits[k * 8..k * 8 + 8].iter().fold(0u8, |acc, b| acc << 1 | b);
        assert_eq!([byte(0), byte(1), byte(2), byte(3)], [0x80, 0x7F, 0x01, 0xFE]);
    }

    #[test]
    fn several_pairs_give_several_frames() {
        assert_eq!(encode(&[1, 2, 3, 4]).unwrap().len(), 68);
    }

    #[test]
    fn odd_input_is_rejected() {
        assert_eq!(encode(&[1]), Err(NecError::OddLength));
    }
}
