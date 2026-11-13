//! Builds the bulk OUT chunks that make the dongle transmit an IR code.
//!
//! Exists to port the transmit path of elksmart-comm.c byte for byte:
//! `compress_pulses()` (two most common pulses as a dictionary, other values
//! as 16 us units or 7-bit varints), the message header (`FF FF FF FF`,
//! mangled carrier frequency and length), and chunking into 62-byte pieces
//! where every full piece gets a checksum byte. Only the plain protocol is
//! covered (no Huffman stage).

use std::collections::HashMap;

use crate::command;
use crate::pulse::Pulse;

/// Largest compressed payload the 16-bit length field can describe.
pub const MAX_PAYLOAD: usize = u16::MAX as usize;
/// Payload bytes per OUT chunk before the checksum byte.
pub const CHUNK_DATA: usize = 62;

/// Why a transmit message could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransmitError {
    /// No pulses to send.
    Empty,
    /// The compressed payload exceeds [`MAX_PAYLOAD`] bytes.
    TooLong(usize),
}

/// Reverses the bits of a byte and inverts them.
pub fn mangle(value: u8) -> u8 {
    !value.reverse_bits()
}

/// Checksum byte appended to every full 62-byte chunk.
pub fn checksum(bytes: &[u8]) -> u8 {
    let sum: u32 = bytes.iter().map(|&b| u32::from(b)).sum();
    mangle(((sum & 0xF0) | ((sum >> 8) & 0x0F)) as u8)
}

/// Compresses pulses into the dongle's dictionary format.
pub fn compress_pulses(pulses: &[Pulse]) -> Vec<u8> {
    let mut counts: HashMap<Pulse, usize> = HashMap::new();
    for &p in pulses {
        *counts.entry(p).or_default() += 1;
    }
    // Ties go to the earliest pulse, as in the reference.
    let mut p1: Option<Pulse> = None;
    for &p in pulses {
        if p1.is_none_or(|best| counts[&p] > counts[&best]) {
            p1 = Some(p);
        }
    }
    let mut p2: Option<Pulse> = None;
    for &p in pulses {
        if Some(p) != p1 && p2.is_none_or(|best| counts[&p] > counts[&best]) {
            p2 = Some(p);
        }
    }
    let mut out = Vec::new();
    let (Some(p1), Some(p2)) = (p1, p2.or(p1)) else {
        return out;
    };
    for value in [p2.on_us, p2.off_us, p1.on_us, p1.off_us] {
        compress_value(value, &mut out);
    }
    out.extend_from_slice(&[0xFF; 3]);
    for &p in pulses {
        if p == p1 {
            out.push(0);
        } else if p == p2 {
            out.push(1);
        } else {
            compress_value(p.on_us, &mut out);
            compress_value(p.off_us, &mut out);
        }
    }
    out
}

/// One duration: 16 us units rounded half up (at least 2) up to 2032 us,
/// otherwise little-endian 7-bit groups with 0x80 as continuation flag.
fn compress_value(mut value: u32, out: &mut Vec<u8>) {
    if value <= 2032 {
        out.push((((value + 8) / 16) as u8).max(2));
        return;
    }
    loop {
        let mut v = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            v |= 0x80;
        }
        out.push(v);
        if value == 0 {
            break;
        }
    }
}

/// Builds the complete message: header, mangled frequency and length, payload.
pub fn build_message(frequency_hz: u32, compressed: &[u8]) -> Result<Vec<u8>, TransmitError> {
    if compressed.len() > MAX_PAYLOAD {
        return Err(TransmitError::TooLong(compressed.len()));
    }
    let f = frequency_hz.wrapping_add(0x7FFFF);
    let len = compressed.len();
    let mut msg = Vec::with_capacity(9 + len);
    msg.extend_from_slice(&command::TRANSMIT);
    msg.extend([(f >> 8) as u8, (f >> 16) as u8, f as u8].map(mangle));
    msg.extend([(len >> 8) as u8, len as u8].map(mangle));
    msg.extend_from_slice(compressed);
    Ok(msg)
}

/// Splits a message into OUT chunks; full 62-byte chunks get a checksum byte.
pub fn chunk_message(message: &[u8]) -> Vec<Vec<u8>> {
    message
        .chunks(CHUNK_DATA)
        .map(|chunk| {
            let mut out = chunk.to_vec();
            if chunk.len() == CHUNK_DATA {
                out.push(checksum(chunk));
            }
            out
        })
        .collect()
}

/// Compresses, frames and chunks pulses for one transmission.
pub fn transmit_chunks(frequency_hz: u32, pulses: &[Pulse]) -> Result<Vec<Vec<u8>>, TransmitError> {
    if pulses.is_empty() {
        return Err(TransmitError::Empty);
    }
    Ok(chunk_message(&build_message(frequency_hz, &compress_pulses(pulses))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    #[test]
    fn mangle_reverses_and_inverts() {
        assert_eq!(mangle(0x00), 0xFF);
        assert_eq!(mangle(0x01), 0x7F);
        assert_eq!(mangle(0x80), 0xFE);
        assert_eq!(mangle(0xFF), 0x00);
    }

    #[test]
    fn small_values_round_to_16us_units_min_2() {
        // Single pulse: dictionary is p2 = p1 = 560/0; off 0 is stored as 2.
        assert_eq!(compress_pulses(&[p(560, 0)]), vec![0x23, 0x02, 0x23, 0x02, 0xFF, 0xFF, 0xFF, 0x00]);
    }

    #[test]
    fn rounding_is_half_up() {
        // Dictionary is p2 (1000/1000 -> 62.5 -> 63) then p1 (24/40).
        // 24 us = 1.5 units -> 2; 40 us = 2.5 units -> 3.
        let out = compress_pulses(&[p(24, 40), p(24, 40), p(1000, 1000)]);
        assert_eq!(&out[..4], &[0x3F, 0x3F, 0x02, 0x03]);
    }

    #[test]
    fn values_above_2032_are_7bit_varints() {
        // 70000 = 0x11170 -> 0xF0 0xA2 0x04 (little-endian 7-bit groups).
        let out = compress_pulses(&[p(1, 1), p(1, 1), p(2, 2), p(3000, 70000)]);
        assert!(out.ends_with(&[0xB8, 0x17, 0xF0, 0xA2, 0x04]));
    }

    #[test]
    fn empty_pulses_are_rejected() {
        assert_eq!(transmit_chunks(38000, &[]), Err(TransmitError::Empty));
    }

    #[test]
    fn too_long_payload_is_rejected() {
        let payload = vec![0u8; MAX_PAYLOAD + 1];
        assert_eq!(build_message(38000, &payload), Err(TransmitError::TooLong(MAX_PAYLOAD + 1)));
    }

    #[test]
    fn chunking_adds_checksum_only_to_full_chunks() {
        let msg: Vec<u8> = (0..130u8).collect();
        let chunks = chunk_message(&msg);
        assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), vec![63, 63, 6]);
        assert_eq!(chunks[0][62], checksum(&msg[..62]));
        assert_eq!(&chunks[2][..], &msg[124..]);
    }

    #[test]
    fn exactly_62_bytes_is_one_full_chunk() {
        let msg = vec![7u8; 62];
        let chunks = chunk_message(&msg);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 63);
    }
}
