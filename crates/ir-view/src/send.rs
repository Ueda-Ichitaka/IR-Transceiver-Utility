//! The Send page form.
//!
//! Exists to turn what the user typed (design decision 10: learned bytes,
//! pulse timings, NEC address and command, or raw USB bytes; decision 11: raw
//! bytes always offered) into a session command, applying the global carrier
//! and repeat defaults unless the form overrides them (decisions 12, 13).
//! Parsing errors are returned as messages for the page.

use ir_codec::Encoding;
use ir_library::model::{Code, Payload, RepeatMode};
use ir_library::send::{self, Defaults, PrepareError, Prepared};
use ir_session::session::Command;

/// Highest carrier the transmit header can hold (24 bits of `f + 0x7FFFF`).
pub const MAX_CARRIER_HZ: u32 = 0xFF_FFFF - 0x7_FFFF;

/// What the form sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    Learned,
    Pulses,
    Nec,
    Raw,
}

impl PayloadKind {
    pub const ALL: [PayloadKind; 4] = [PayloadKind::Learned, PayloadKind::Pulses, PayloadKind::Nec, PayloadKind::Raw];

    pub fn label(self) -> &'static str {
        match self {
            PayloadKind::Learned => "Learned code",
            PayloadKind::Pulses => "Pulse timings",
            PayloadKind::Nec => "NEC",
            PayloadKind::Raw => "Raw USB bytes",
        }
    }
}

/// The form fields as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendForm {
    pub kind: PayloadKind,
    /// Learned or raw bytes, in `encoding`.
    pub bytes_text: String,
    pub encoding: Encoding,
    /// Pulse timings as `on/off` pairs in microseconds.
    pub pulses_text: String,
    /// NEC address and command in hex (optional `0x`).
    pub nec_address: String,
    pub nec_command: String,
    pub nec_extended: bool,
    /// Carrier in Hz; empty uses the default.
    pub carrier_text: String,
    /// `None` uses the default.
    pub repeat: Option<RepeatMode>,
}

impl Default for SendForm {
    fn default() -> Self {
        Self {
            kind: PayloadKind::Learned,
            bytes_text: String::new(),
            encoding: Encoding::Hex,
            pulses_text: String::new(),
            nec_address: String::new(),
            nec_command: String::new(),
            nec_extended: false,
            carrier_text: String::new(),
            repeat: None,
        }
    }
}

/// Parses `on/off` pairs separated by spaces, commas or line breaks.
pub fn parse_pulses(text: &str) -> Result<Vec<[u32; 2]>, String> {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|item| !item.is_empty())
        .enumerate()
        .map(|(i, item)| {
            let pair = item.split_once('/').and_then(|(on, off)| Some([on.parse().ok()?, off.parse().ok()?]));
            pair.ok_or_else(|| format!("pulse {} ({item:?}) is not on/off", i + 1))
        })
        .collect()
}

/// Parses a hex number (optional `0x`) of at most `max`.
pub fn parse_hex(text: &str, max: u32) -> Result<u32, String> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")).unwrap_or(trimmed);
    if digits.is_empty() {
        return Err("a hex number is missing".into());
    }
    let value = u32::from_str_radix(digits, 16).map_err(|_| format!("{trimmed:?} is not a hex number"))?;
    if value > max {
        return Err(format!("0x{value:X} is larger than 0x{max:X}"));
    }
    Ok(value)
}

impl SendForm {
    /// The payload described by the form.
    pub fn payload(&self) -> Result<Payload, String> {
        let bytes = || ir_codec::parse(&self.bytes_text, self.encoding).map_err(|e| e.to_string());
        Ok(match self.kind {
            PayloadKind::Learned => Payload::Learned { data: bytes()? },
            PayloadKind::Raw => Payload::Raw { data: bytes()? },
            PayloadKind::Pulses => Payload::Pulses { pulses: parse_pulses(&self.pulses_text)? },
            PayloadKind::Nec => {
                let max_address = if self.nec_extended { 0xFFFF } else { 0xFF };
                let address = parse_hex(&self.nec_address, max_address).map_err(|e| format!("address: {e}"))?;
                let command = parse_hex(&self.nec_command, 0xFF).map_err(|e| format!("command: {e}"))?;
                Payload::Nec { address: address as u16, command: command as u8, extended: self.nec_extended }
            }
        })
    }

    /// Carrier override; `None` when the field is empty.
    fn carrier(&self) -> Result<Option<u32>, String> {
        let text = self.carrier_text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let hz: u32 = text.parse().map_err(|_| format!("carrier: {text:?} is not a number of Hz"))?;
        if !(1..=MAX_CARRIER_HZ).contains(&hz) {
            return Err(format!("carrier: must be between 1 and {MAX_CARRIER_HZ} Hz"));
        }
        Ok(Some(hz))
    }

    /// The command to send, with `defaults` for unset carrier and repeat.
    pub fn command(&self, defaults: &Defaults) -> Result<Command, String> {
        let payload = self.payload()?;
        if matches!(&payload, Payload::Learned { data } if data.is_empty()) {
            return Err("nothing to send".into());
        }
        let code = Code {
            id: 0,
            name: String::new(),
            tags: Vec::new(),
            carrier_hz: self.carrier()?,
            repeat: self.repeat,
            payload,
            notes: String::new(),
        };
        match send::prepare(&code, defaults) {
            Ok(Prepared::Pulses { pulses, carrier_hz }) => Ok(Command::Transmit { pulses, frequency_hz: carrier_hz }),
            Ok(Prepared::Raw(bytes)) => Ok(Command::TransmitRaw { bytes }),
            Err(PrepareError::Empty) => Err("nothing to send".into()),
            Err(PrepareError::UndecodableLearnedData) => Err("the learned bytes do not decode into pulses".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elk_proto::nec;
    use elk_proto::pulse::Pulse;

    const DEFAULTS: Defaults = Defaults { carrier_hz: 38_000, repeat: RepeatMode::AsLearned };

    fn form(kind: PayloadKind) -> SendForm {
        SendForm { kind, ..SendForm::default() }
    }

    #[test]
    fn kind_labels() {
        let labels: Vec<_> = PayloadKind::ALL.into_iter().map(PayloadKind::label).collect();
        assert_eq!(labels, ["Learned code", "Pulse timings", "NEC", "Raw USB bytes"]);
    }

    #[test]
    fn pulses_parse_with_any_separator() {
        assert_eq!(parse_pulses("9000/4500 560/560,560/0\n560/1690"), Ok(vec![[9000, 4500], [560, 560], [560, 0], [560, 1690]]));
        assert_eq!(parse_pulses("  "), Ok(vec![]));
    }

    #[test]
    fn bad_pulses_name_the_pair() {
        assert_eq!(parse_pulses("560/560 560"), Err("pulse 2 (\"560\") is not on/off".into()));
        assert_eq!(parse_pulses("560/x"), Err("pulse 1 (\"560/x\") is not on/off".into()));
    }

    #[test]
    fn hex_numbers() {
        assert_eq!(parse_hex("59", 0xFF), Ok(0x59));
        assert_eq!(parse_hex("0xEF00", 0xFFFF), Ok(0xEF00));
        assert_eq!(parse_hex(" 0X1f ", 0xFF), Ok(0x1F));
        assert_eq!(parse_hex("100", 0xFF), Err("0x100 is larger than 0xFF".into()));
        assert_eq!(parse_hex("", 0xFF), Err("a hex number is missing".into()));
        assert_eq!(parse_hex("zz", 0xFF), Err("\"zz\" is not a hex number".into()));
    }

    #[test]
    fn learned_and_raw_bytes_use_the_encoding() {
        let mut f = SendForm { bytes_text: "35 55".into(), encoding: Encoding::Decimal, ..form(PayloadKind::Learned) };
        assert_eq!(f.payload(), Ok(Payload::Learned { data: vec![35, 55] }));
        f.kind = PayloadKind::Raw;
        assert_eq!(f.payload(), Ok(Payload::Raw { data: vec![35, 55] }));
        f.bytes_text = "3x".into();
        assert!(f.payload().unwrap_err().starts_with("unexpected character"), "{:?}", f.payload());
    }

    #[test]
    fn nec_fields() {
        let f = SendForm { nec_address: "0x00".into(), nec_command: "59".into(), ..form(PayloadKind::Nec) };
        assert_eq!(f.payload(), Ok(Payload::Nec { address: 0, command: 0x59, extended: false }));
        let ext = SendForm { nec_address: "EF00".into(), nec_command: "3".into(), nec_extended: true, ..form(PayloadKind::Nec) };
        assert_eq!(ext.payload(), Ok(Payload::Nec { address: 0xEF00, command: 3, extended: true }));
        let too_big = SendForm { nec_address: "100".into(), nec_command: "1".into(), ..form(PayloadKind::Nec) };
        assert_eq!(too_big.payload(), Err("address: 0x100 is larger than 0xFF".into()));
        let no_cmd = SendForm { nec_address: "1".into(), ..form(PayloadKind::Nec) };
        assert_eq!(no_cmd.payload(), Err("command: a hex number is missing".into()));
    }

    #[test]
    fn command_uses_defaults_and_overrides() {
        let f = SendForm { nec_address: "0".into(), nec_command: "59".into(), ..form(PayloadKind::Nec) };
        let expected = nec::encode_command(0, 0x59, false);
        assert_eq!(f.command(&DEFAULTS), Ok(Command::Transmit { pulses: expected.clone(), frequency_hz: 38_000 }));
        let custom = SendForm { carrier_text: " 36000 ".into(), ..f };
        assert_eq!(custom.command(&DEFAULTS), Ok(Command::Transmit { pulses: expected, frequency_hz: 36_000 }));
    }

    #[test]
    fn repeat_override_applies() {
        let f = SendForm {
            pulses_text: "560/20000 560/20000".into(),
            repeat: Some(RepeatMode::FirstFrame { count: 3 }),
            ..form(PayloadKind::Pulses)
        };
        let one = Pulse { on_us: 560, off_us: 20_000 };
        assert_eq!(f.command(&DEFAULTS), Ok(Command::Transmit { pulses: vec![one; 3], frequency_hz: 38_000 }));
    }

    #[test]
    fn raw_bytes_become_transmit_raw() {
        let f = SendForm { bytes_text: "FC FC FC FC".into(), ..form(PayloadKind::Raw) };
        assert_eq!(f.command(&DEFAULTS), Ok(Command::TransmitRaw { bytes: vec![0xFC; 4] }));
    }

    #[test]
    fn carrier_validation() {
        let base = SendForm { bytes_text: "23 37".into(), ..form(PayloadKind::Learned) };
        for (text, message) in [
            ("fast", "carrier: \"fast\" is not a number of Hz"),
            ("0", "carrier: must be between 1 and 16252928 Hz"),
            ("16252929", "carrier: must be between 1 and 16252928 Hz"),
        ] {
            let f = SendForm { carrier_text: text.into(), ..base.clone() };
            assert_eq!(f.command(&DEFAULTS), Err(message.into()), "{text}");
        }
        let max = SendForm { carrier_text: MAX_CARRIER_HZ.to_string(), ..base };
        assert!(max.command(&DEFAULTS).is_ok());
    }

    #[test]
    fn empty_and_undecodable_inputs() {
        assert_eq!(form(PayloadKind::Raw).command(&DEFAULTS), Err("nothing to send".into()));
        assert_eq!(form(PayloadKind::Pulses).command(&DEFAULTS), Err("nothing to send".into()));
        assert_eq!(form(PayloadKind::Learned).command(&DEFAULTS), Err("nothing to send".into()));
        let bad = SendForm { bytes_text: "23 FF".into(), ..form(PayloadKind::Learned) };
        assert_eq!(bad.command(&DEFAULTS), Err("the learned bytes do not decode into pulses".into()));
    }
}
