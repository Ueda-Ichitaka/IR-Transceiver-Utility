//! Pronto hex, learned type (`0000`).
//!
//! Exists to read and write the most widely shared IR code notation. Layout:
//! type word `0000`, frequency word N (carrier = 1 000 000 / (N * 0.241246)
//! Hz), number of burst pairs in the once and the repeat sequence, then the
//! pairs as counts of carrier periods; all words are four hex digits.
//! Durations are rounded to the nearest microsecond or period. Other Pronto
//! types (unmodulated, RC5, RC6, ...) are rejected.

use std::fmt;

use elk_proto::pulse::Pulse;

/// Length of one Pronto clock tick in microseconds.
pub const CLOCK_US: f64 = 0.241_246;

/// A decoded Pronto code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pronto {
    pub carrier_hz: u32,
    /// Sent once.
    pub once: Vec<Pulse>,
    /// Sent repeatedly while a button is held.
    pub repeat: Vec<Pulse>,
}

/// Why Pronto text could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProntoError {
    /// A word is not four hex digits; carries the word index.
    BadWord(usize),
    /// Fewer than six words (header plus one pair).
    TooShort,
    /// The pair counts do not match the number of words.
    InconsistentLength,
    /// A type other than learned (`0000`); carries the type word.
    UnsupportedType(u16),
    /// Frequency word 0, or a carrier frequency of 0 when writing.
    BadFrequency,
}

impl fmt::Display for ProntoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProntoError::BadWord(i) => write!(f, "word {} is not four hex digits", i + 1),
            ProntoError::TooShort => write!(f, "Pronto code is too short"),
            ProntoError::InconsistentLength => write!(f, "pair counts do not match the length"),
            ProntoError::UnsupportedType(t) => write!(f, "Pronto type {t:04X} is not supported (only 0000)"),
            ProntoError::BadFrequency => write!(f, "invalid carrier frequency"),
        }
    }
}

impl std::error::Error for ProntoError {}

/// Parses Pronto hex (words separated by whitespace, any case).
pub fn parse(text: &str) -> Result<Pronto, ProntoError> {
    let words = text
        .split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            if w.len() == 4 { u16::from_str_radix(w, 16).map_err(|_| ProntoError::BadWord(i)) } else { Err(ProntoError::BadWord(i)) }
        })
        .collect::<Result<Vec<u16>, _>>()?;
    if words.len() < 6 {
        return Err(ProntoError::TooShort);
    }
    if words[0] != 0 {
        return Err(ProntoError::UnsupportedType(words[0]));
    }
    if words[1] == 0 {
        return Err(ProntoError::BadFrequency);
    }
    let (once_pairs, repeat_pairs) = (usize::from(words[2]), usize::from(words[3]));
    if words.len() != 4 + 2 * (once_pairs + repeat_pairs) {
        return Err(ProntoError::InconsistentLength);
    }
    let period = f64::from(words[1]) * CLOCK_US;
    let pulses: Vec<Pulse> = words[4..]
        .chunks_exact(2)
        .map(|pair| Pulse {
            on_us: (f64::from(pair[0]) * period).round() as u32,
            off_us: (f64::from(pair[1]) * period).round() as u32,
        })
        .collect();
    let (once, repeat) = pulses.split_at(once_pairs);
    Ok(Pronto {
        carrier_hz: (1_000_000.0 / period).round() as u32,
        once: once.to_vec(),
        repeat: repeat.to_vec(),
    })
}

/// Formats as Pronto hex (uppercase, single spaces).
pub fn format(code: &Pronto) -> Result<String, ProntoError> {
    if code.carrier_hz == 0 {
        return Err(ProntoError::BadFrequency);
    }
    let n = (1_000_000.0 / (f64::from(code.carrier_hz) * CLOCK_US)).round().clamp(1.0, 65535.0);
    let period = n * CLOCK_US;
    let count = |us: u32| (f64::from(us) / period).round().min(65535.0) as u16;
    let len = |pulses: &[Pulse]| u16::try_from(pulses.len()).unwrap_or(u16::MAX);
    let mut words = vec![0, n as u16, len(&code.once), len(&code.repeat)];
    for p in code.once.iter().take(usize::from(words[2])).chain(code.repeat.iter().take(usize::from(words[3]))) {
        words.push(count(p.on_us));
        words.push(count(p.off_us));
    }
    Ok(words.iter().map(|w| format!("{w:04X}")).collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    #[test]
    fn parses_frequency_and_durations() {
        // N = 0x6D = 109 -> period 26.295814 us -> 38029 Hz
        // 0x157 = 343 periods -> 9019 us, 0xAC = 172 -> 4523 us
        let code = parse("0000 006D 0001 0000 0157 00AC").unwrap();
        assert_eq!(code, Pronto { carrier_hz: 38029, once: vec![p(9019, 4523)], repeat: vec![] });
    }

    #[test]
    fn splits_once_and_repeat_sequences() {
        let code = parse("0000 006D 0001 0001 0157 00AC 0157 0056").unwrap();
        assert_eq!(code.once.len(), 1);
        assert_eq!(code.repeat, vec![p(9019, 2261)]);
    }

    #[test]
    fn accepts_lowercase_and_extra_whitespace() {
        assert!(parse("  0000 006d\n0001 0000\t0157 00ac ").is_ok());
    }

    #[test]
    fn format_parse_round_trip_is_exact() {
        let text = "0000 006C 0002 0002 015B 00AD 0016 0041 015B 0057 0016 0E6C";
        assert_eq!(format(&parse(text).unwrap()).unwrap(), text);
    }

    #[test]
    fn formats_from_microseconds() {
        let code = Pronto { carrier_hz: 38000, once: vec![p(9000, 4500), p(560, 560)], repeat: vec![] };
        // N = round(1e6 / (38000 * 0.241246)) = 109; period 26.2958 us
        assert_eq!(format(&code).unwrap(), "0000 006D 0002 0000 0156 00AB 0015 0015");
    }

    #[test]
    fn long_durations_are_clamped_to_ffff() {
        let code = Pronto { carrier_hz: 38000, once: vec![p(560, 10_000_000)], repeat: vec![] };
        assert!(format(&code).unwrap().ends_with("0015 FFFF"));
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse("0000 006D 0001"), Err(ProntoError::TooShort));
        assert_eq!(parse("0000 006D 0001 0000 0157"), Err(ProntoError::TooShort));
        assert_eq!(parse("0000 006D 0002 0000 0157 00AC"), Err(ProntoError::InconsistentLength));
        assert_eq!(parse("0000 6D 0001 0000 0157 00AC"), Err(ProntoError::BadWord(1)));
        assert_eq!(parse("0000 006D 0001 0000 0157 00AG"), Err(ProntoError::BadWord(5)));
        assert_eq!(parse("0000 0000 0001 0000 0157 00AC"), Err(ProntoError::BadFrequency));
        assert_eq!(parse("5000 0073 0000 0001 0000 0001"), Err(ProntoError::UnsupportedType(0x5000)));
    }

    #[test]
    fn zero_carrier_cannot_be_written() {
        let code = Pronto { carrier_hz: 0, once: vec![p(1, 1)], repeat: vec![] };
        assert_eq!(format(&code), Err(ProntoError::BadFrequency));
    }
}
