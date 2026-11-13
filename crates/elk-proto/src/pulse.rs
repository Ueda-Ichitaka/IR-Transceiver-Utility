//! Decodes learned IR bytes into on/off pulse durations.
//!
//! Exists to turn the dongle's "plain" learned format into microsecond timings
//! for display and analysis. Mirrors `decode_learned_direct()` in
//! elksmart-comm.c: each byte is a duration in 16 us units, every 0xFF byte
//! adds 4080 us and continues the same duration, bytes alternate on/off.

/// One IR pulse: carrier on for `on_us`, then off for `off_us` microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulse {
    pub on_us: u32,
    pub off_us: u32,
}

/// Why learned bytes could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// No bytes, or the data ends inside a run of 0xFF bytes.
    EndsUnexpectedly,
}

/// Decodes plain learned bytes into pulses. A final on-time without an
/// off-time yields a pulse with `off_us == 0`.
pub fn decode_learned(bytes: &[u8]) -> Result<Vec<Pulse>, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::EndsUnexpectedly);
    }
    let mut iter = bytes.iter().copied();
    let mut pulses = Vec::new();
    while let Some(on_us) = read_duration(&mut iter)? {
        let off_us = read_duration(&mut iter)?.unwrap_or(0);
        pulses.push(Pulse { on_us, off_us });
    }
    Ok(pulses)
}

/// Reads one duration: any number of 0xFF bytes (4080 us each) closed by a
/// non-0xFF byte (16 us units). `Ok(None)` when no bytes are left.
fn read_duration(iter: &mut impl Iterator<Item = u8>) -> Result<Option<u32>, DecodeError> {
    let Some(mut byte) = iter.next() else {
        return Ok(None);
    };
    let mut total = 0u32;
    while byte == 0xFF {
        total += 4080;
        byte = iter.next().ok_or(DecodeError::EndsUnexpectedly)?;
    }
    Ok(Some(total + u32::from(byte) * 16))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    #[test]
    fn bytes_are_16us_units_alternating_on_off() {
        assert_eq!(decode_learned(&[0x23, 0x69]), Ok(vec![p(560, 1680)]));
    }

    #[test]
    fn ff_runs_add_4080us_each() {
        // 0xFF 0xFF 0x34 -> 4080 + 4080 + 52*16 = 8992 us
        assert_eq!(decode_learned(&[0xFF, 0xFF, 0x34, 0xFF, 0x1A]), Ok(vec![p(8992, 4496)]));
    }

    #[test]
    fn trailing_on_without_off_has_zero_off() {
        assert_eq!(decode_learned(&[0x23, 0x23, 0x23]), Ok(vec![p(560, 560), p(560, 0)]));
    }

    #[test]
    fn data_ending_in_ff_is_an_error() {
        assert_eq!(decode_learned(&[0x23, 0xFF]), Err(DecodeError::EndsUnexpectedly));
        assert_eq!(decode_learned(&[0xFF]), Err(DecodeError::EndsUnexpectedly));
    }

    #[test]
    fn empty_input_is_an_error() {
        assert_eq!(decode_learned(&[]), Err(DecodeError::EndsUnexpectedly));
    }

    #[test]
    fn zero_byte_is_zero_duration() {
        assert_eq!(decode_learned(&[0x00, 0x00]), Ok(vec![p(0, 0)]));
    }
}
