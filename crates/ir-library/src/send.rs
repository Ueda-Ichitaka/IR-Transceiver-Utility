//! Turns a saved code and the global defaults into what is sent.
//!
//! Exists to apply design decisions 12 and 13 in one place: a code's own
//! carrier frequency and repeat mode override the global defaults, and
//! "first frame" mode sends only the first frame of a capture, repeated
//! `count` times with its own trailing gap.

use elk_proto::frames;
use elk_proto::nec;
use elk_proto::pulse::{self, Pulse};

use crate::model::{Code, Payload, RepeatMode};

/// Global defaults (user settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Defaults {
    pub carrier_hz: u32,
    pub repeat: RepeatMode,
}

/// What to hand to the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prepared {
    Pulses { pulses: Vec<Pulse>, carrier_hz: u32 },
    /// Raw bytes for the USB OUT endpoint; carrier and repeat do not apply.
    Raw(Vec<u8>),
}

/// Why a code cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareError {
    /// Learned bytes that do not decode into pulses.
    UndecodableLearnedData,
    /// No pulses or bytes to send.
    Empty,
}

/// Prepares `code` for sending.
pub fn prepare(code: &Code, defaults: &Defaults) -> Result<Prepared, PrepareError> {
    let all: Vec<Pulse> = match &code.payload {
        Payload::Raw { data } if data.is_empty() => return Err(PrepareError::Empty),
        Payload::Raw { data } => return Ok(Prepared::Raw(data.clone())),
        Payload::Learned { data } => {
            pulse::decode_learned(data).map_err(|_| PrepareError::UndecodableLearnedData)?
        }
        Payload::Pulses { pulses } => pulses.iter().map(|&[on_us, off_us]| Pulse { on_us, off_us }).collect(),
        Payload::Nec { address, command, extended } => nec::encode_command(*address, *command, *extended),
    };
    if all.is_empty() {
        return Err(PrepareError::Empty);
    }
    let pulses = match code.repeat.unwrap_or(defaults.repeat) {
        RepeatMode::AsLearned => all,
        RepeatMode::FirstFrame { count } => {
            let first = frames::split(&all, frames::DEFAULT_GAP_US)[0];
            first.repeat(count.max(1) as usize)
        }
    };
    Ok(Prepared::Pulses { pulses, carrier_hz: code.carrier_hz.unwrap_or(defaults.carrier_hz) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: Defaults = Defaults { carrier_hz: 38000, repeat: RepeatMode::AsLearned };

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    fn code(payload: Payload) -> Code {
        Code { id: 1, name: "x".into(), tags: vec![], carrier_hz: None, repeat: None, payload, notes: String::new() }
    }

    fn pulses(prepared: Result<Prepared, PrepareError>) -> (Vec<Pulse>, u32) {
        match prepared {
            Ok(Prepared::Pulses { pulses, carrier_hz }) => (pulses, carrier_hz),
            other => panic!("expected pulses, got {other:?}"),
        }
    }

    /// Two frames separated by a 40 ms gap, in learned (16 us unit) bytes:
    /// frame 1 = 560/560, 560/40000; frame 2 = 560/0.
    fn two_frame_learned() -> Vec<u8> {
        // 40000 us = 9 * 4080 + 3280 -> nine 0xFF then 205 (205 * 16 = 3280)
        let mut b = vec![0x23, 0x23, 0x23];
        b.extend([0xFF; 9]);
        b.extend([205, 0x23]);
        b
    }

    #[test]
    fn learned_as_learned_sends_everything_at_default_carrier() {
        let (pulses, carrier) = pulses(prepare(&code(Payload::Learned { data: two_frame_learned() }), &DEFAULTS));
        assert_eq!(pulses, vec![p(560, 560), p(560, 40000), p(560, 0)]);
        assert_eq!(carrier, 38000);
    }

    #[test]
    fn first_frame_mode_repeats_only_the_first_frame() {
        let mut c = code(Payload::Learned { data: two_frame_learned() });
        c.repeat = Some(RepeatMode::FirstFrame { count: 2 });
        let (pulses, _) = pulses(prepare(&c, &DEFAULTS));
        assert_eq!(pulses, vec![p(560, 560), p(560, 40000), p(560, 560), p(560, 40000)]);
    }

    #[test]
    fn global_repeat_default_applies_without_override() {
        let defaults = Defaults { repeat: RepeatMode::FirstFrame { count: 1 }, ..DEFAULTS };
        let (pulses, _) = pulses(prepare(&code(Payload::Learned { data: two_frame_learned() }), &defaults));
        assert_eq!(pulses.len(), 2);
    }

    #[test]
    fn code_overrides_win_over_defaults() {
        let defaults = Defaults { carrier_hz: 36000, repeat: RepeatMode::FirstFrame { count: 1 } };
        let mut c = code(Payload::Learned { data: two_frame_learned() });
        c.carrier_hz = Some(40000);
        c.repeat = Some(RepeatMode::AsLearned);
        let (pulses, carrier) = pulses(prepare(&c, &defaults));
        assert_eq!((pulses.len(), carrier), (3, 40000));
    }

    #[test]
    fn zero_count_sends_once() {
        let mut c = code(Payload::Learned { data: two_frame_learned() });
        c.repeat = Some(RepeatMode::FirstFrame { count: 0 });
        assert_eq!(pulses(prepare(&c, &DEFAULTS)).0.len(), 2);
    }

    #[test]
    fn nec_payload_is_encoded_and_repeated() {
        let mut c = code(Payload::Nec { address: 0x04, command: 0x08, extended: false });
        let (one, _) = pulses(prepare(&c, &DEFAULTS));
        assert_eq!(one, nec::encode_command(0x04, 0x08, false));
        c.repeat = Some(RepeatMode::FirstFrame { count: 3 });
        assert_eq!(pulses(prepare(&c, &DEFAULTS)).0.len(), 3 * one.len());
    }

    #[test]
    fn pulse_payload_is_sent_as_is() {
        let c = code(Payload::Pulses { pulses: vec![[9000, 4500], [560, 0]] });
        assert_eq!(pulses(prepare(&c, &DEFAULTS)).0, vec![p(9000, 4500), p(560, 0)]);
    }

    #[test]
    fn raw_payload_bypasses_encoding() {
        let mut c = code(Payload::Raw { data: vec![0xFC; 4] });
        c.repeat = Some(RepeatMode::FirstFrame { count: 5 });
        assert_eq!(prepare(&c, &DEFAULTS), Ok(Prepared::Raw(vec![0xFC; 4])));
    }

    #[test]
    fn bad_or_empty_payloads_are_errors() {
        assert_eq!(
            prepare(&code(Payload::Learned { data: vec![0x23, 0xFF] }), &DEFAULTS),
            Err(PrepareError::UndecodableLearnedData)
        );
        assert_eq!(prepare(&code(Payload::Pulses { pulses: vec![] }), &DEFAULTS), Err(PrepareError::Empty));
        assert_eq!(prepare(&code(Payload::Raw { data: vec![] }), &DEFAULTS), Err(PrepareError::Empty));
    }
}
