//! The Send page form.
//!
//! Exists to turn what the user typed (design decision 10: learned bytes,
//! pulse timings, NEC address and command, or raw USB bytes; decision 11: raw
//! bytes always offered) into a session command, applying the global carrier
//! and repeat defaults unless the form overrides them (decisions 12, 13).
//! Parsing errors are returned as messages for the page. It also gives the
//! live hint under the data field, the wire preview (the USB transfers a
//! command produces) and converts between the form and library codes.

use elk_proto::transmit::{self, TransmitError};
use ir_codec::{Encoding, FormatOptions};
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

/// Parses a carrier override in Hz; `None` when the text is empty.
pub fn parse_carrier(text: &str) -> Result<Option<u32>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let hz: u32 = text.parse().map_err(|_| format!("carrier: {text:?} is not a number of Hz"))?;
    if !(1..=MAX_CARRIER_HZ).contains(&hz) {
        return Err(format!("carrier: must be between 1 and {MAX_CARRIER_HZ} Hz"));
    }
    Ok(Some(hz))
}

/// The session command that sends `code`, with `defaults` for its unset
/// carrier and repeat mode.
pub fn command_for(code: &Code, defaults: &Defaults) -> Result<Command, String> {
    match send::prepare(code, defaults) {
        Ok(Prepared::Pulses { pulses, carrier_hz }) => Ok(Command::Transmit { pulses, frequency_hz: carrier_hz }),
        Ok(Prepared::Raw(bytes)) => Ok(Command::TransmitRaw { bytes }),
        Err(PrepareError::Empty) => Err("nothing to send".into()),
        Err(PrepareError::UndecodableLearnedData) => Err("the learned bytes do not decode into pulses".into()),
    }
}

/// The USB OUT transfers a command produces (header and chunks for a
/// transmit, the bytes themselves for raw).
pub fn wire(command: &Command) -> Result<Vec<Vec<u8>>, String> {
    match command {
        Command::Transmit { pulses, frequency_hz } => {
            transmit::transmit_chunks(*frequency_hz, pulses).map_err(|e| match e {
                TransmitError::Empty => "nothing to send".to_string(),
                TransmitError::TooLong(n) => {
                    format!("the code is too long for the dongle ({n} bytes compressed, at most {})", transmit::MAX_PAYLOAD)
                }
            })
        }
        Command::TransmitRaw { bytes } if !bytes.is_empty() => Ok(vec![bytes.clone()]),
        _ => Err("nothing to send".into()),
    }
}

/// `82 bytes in 2 chunks`.
pub fn wire_summary(chunks: &[Vec<u8>]) -> String {
    let bytes: usize = chunks.iter().map(Vec::len).sum();
    let unit = if chunks.len() == 1 { "chunk" } else { "chunks" };
    format!("{bytes} bytes in {} {unit}", chunks.len())
}

/// `text` converted from one encoding to another; `None` when it does not
/// parse (it is then kept as typed).
pub fn reencode(text: &str, from: Encoding, to: Encoding) -> Option<String> {
    let bytes = ir_codec::parse(text, from).ok()?;
    Some(ir_codec::format(&bytes, to, &FormatOptions { uppercase: true, bytes_per_line: 0 }))
}

impl SendForm {
    /// The form filled from a library code, bytes in `encoding`.
    pub fn from_code(code: &Code, encoding: Encoding) -> Self {
        let bytes = |data: &[u8]| ir_codec::format(data, encoding, &FormatOptions { uppercase: true, bytes_per_line: 0 });
        let mut form = SendForm {
            encoding,
            carrier_text: code.carrier_hz.map_or_else(String::new, |hz| hz.to_string()),
            repeat: code.repeat,
            ..SendForm::default()
        };
        match &code.payload {
            Payload::Learned { data } => {
                form.kind = PayloadKind::Learned;
                form.bytes_text = bytes(data);
            }
            Payload::Raw { data } => {
                form.kind = PayloadKind::Raw;
                form.bytes_text = bytes(data);
            }
            Payload::Pulses { pulses } => {
                form.kind = PayloadKind::Pulses;
                form.pulses_text = pulses.iter().map(|[on, off]| format!("{on}/{off}")).collect::<Vec<_>>().join(" ");
            }
            Payload::Nec { address, command, extended } => {
                form.kind = PayloadKind::Nec;
                form.nec_address = if *extended { format!("{address:04X}") } else { format!("{address:02X}") };
                form.nec_command = format!("{command:02X}");
                form.nec_extended = *extended;
            }
        }
        form
    }

    /// One line under the data field: what will be sent, or why not.
    pub fn hint(&self, defaults: &Defaults) -> Result<String, String> {
        let command = self.command(defaults)?;
        let chunks = wire(&command)?;
        let what = match &command {
            Command::Transmit { pulses, frequency_hz } => format!("{} pulses at {frequency_hz} Hz", pulses.len()),
            Command::TransmitRaw { bytes } => format!("{} raw bytes", bytes.len()),
            _ => String::new(),
        };
        Ok(format!("{what}, {}", wire_summary(&chunks)))
    }

    /// The payload and overrides for saving the form as a library code.
    pub fn code_parts(&self) -> Result<(Payload, Option<u32>, Option<RepeatMode>), String> {
        Ok((self.payload()?, parse_carrier(&self.carrier_text)?, self.repeat))
    }

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
            carrier_hz: parse_carrier(&self.carrier_text)?,
            repeat: self.repeat,
            payload,
            notes: String::new(),
            added: None,
        };
        command_for(&code, defaults)
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

    #[test]
    fn wire_of_transmit_and_raw_commands() {
        let pulses = nec::encode_command(0, 0x59, false);
        let chunks = wire(&Command::Transmit { pulses: pulses.clone(), frequency_hz: 38_000 }).unwrap();
        assert_eq!(chunks, elk_proto::transmit::transmit_chunks(38_000, &pulses).unwrap());
        assert_eq!(wire(&Command::TransmitRaw { bytes: vec![0xFC; 4] }), Ok(vec![vec![0xFC; 4]]));
        assert_eq!(wire(&Command::Connect), Err("nothing to send".into()));
        assert_eq!(wire(&Command::Transmit { pulses: vec![], frequency_hz: 38_000 }), Err("nothing to send".into()));
    }

    #[test]
    fn wire_summaries() {
        assert_eq!(wire_summary(&[vec![0; 63], vec![0; 19]]), "82 bytes in 2 chunks");
        assert_eq!(wire_summary(&[vec![0; 4]]), "4 bytes in 1 chunk");
    }

    #[test]
    fn hints_say_what_is_sent_or_why_not() {
        let nec_form = SendForm { nec_address: "0".into(), nec_command: "59".into(), ..form(PayloadKind::Nec) };
        let chunks = wire(&nec_form.command(&DEFAULTS).unwrap()).unwrap();
        assert_eq!(nec_form.hint(&DEFAULTS), Ok(format!("34 pulses at 38000 Hz, {}", wire_summary(&chunks))));
        let raw = SendForm { bytes_text: "FC FC FC FC".into(), ..form(PayloadKind::Raw) };
        assert_eq!(raw.hint(&DEFAULTS), Ok("4 raw bytes, 4 bytes in 1 chunk".into()));
        let bad = SendForm { nec_address: "zz".into(), ..form(PayloadKind::Nec) };
        assert_eq!(bad.hint(&DEFAULTS), Err("address: \"zz\" is not a hex number".into()));
    }

    fn library_code(payload: Payload) -> Code {
        Code { id: 7, name: "x".into(), tags: vec![], carrier_hz: Some(36_000), repeat: Some(RepeatMode::FirstFrame { count: 2 }), payload, notes: String::new(), added: None }
    }

    #[test]
    fn forms_load_library_codes() {
        let learned = SendForm::from_code(&library_code(Payload::Learned { data: vec![0x23, 0x37] }), Encoding::Decimal);
        assert_eq!((learned.kind, learned.bytes_text.as_str(), learned.encoding), (PayloadKind::Learned, "35 55", Encoding::Decimal));
        assert_eq!((learned.carrier_text.as_str(), learned.repeat), ("36000", Some(RepeatMode::FirstFrame { count: 2 })));
        let pulses = SendForm::from_code(&library_code(Payload::Pulses { pulses: vec![[9000, 4500], [560, 0]] }), Encoding::Hex);
        assert_eq!((pulses.kind, pulses.pulses_text.as_str()), (PayloadKind::Pulses, "9000/4500 560/0"));
        let nec_code = SendForm::from_code(&library_code(Payload::Nec { address: 0xEF00, command: 3, extended: true }), Encoding::Hex);
        assert_eq!(
            (nec_code.kind, nec_code.nec_address.as_str(), nec_code.nec_command.as_str(), nec_code.nec_extended),
            (PayloadKind::Nec, "EF00", "03", true)
        );
        let raw = SendForm::from_code(&library_code(Payload::Raw { data: vec![0xFC] }), Encoding::Hex);
        assert_eq!((raw.kind, raw.bytes_text.as_str()), (PayloadKind::Raw, "FC"));
    }

    #[test]
    fn loaded_codes_round_trip_through_code_parts() {
        for payload in [
            Payload::Learned { data: vec![0x23, 0x37] },
            Payload::Pulses { pulses: vec![[9000, 4500], [560, 0]] },
            Payload::Nec { address: 0x04, command: 0x08, extended: false },
            Payload::Raw { data: vec![0xFC; 4] },
        ] {
            let code = library_code(payload.clone());
            let parts = SendForm::from_code(&code, Encoding::Base64).code_parts().unwrap();
            assert_eq!(parts, (payload, Some(36_000), Some(RepeatMode::FirstFrame { count: 2 })));
        }
        assert!(SendForm { carrier_text: "x".into(), ..form(PayloadKind::Raw) }.code_parts().is_err());
    }

    #[test]
    fn bytes_follow_the_encoding_switch() {
        assert_eq!(reencode("23 37", Encoding::Hex, Encoding::Decimal), Some("35 55".into()));
        assert_eq!(reencode("35 55", Encoding::Decimal, Encoding::Hex), Some("23 37".into()));
        assert_eq!(reencode("", Encoding::Hex, Encoding::Binary), Some(String::new()));
        assert_eq!(reencode("2", Encoding::Hex, Encoding::Decimal), None);
    }
}
