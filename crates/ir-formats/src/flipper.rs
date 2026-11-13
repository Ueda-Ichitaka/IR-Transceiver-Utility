//! Flipper Zero infrared files (`.ir`).
//!
//! Exists to exchange codes with Flipper Zero. Format (Flipper developer
//! docs, "Infrared file format"): header `Filetype: IR signals file` (or
//! `IR library file`) and `Version: 1`, then blocks separated by `#` lines.
//! Raw blocks carry `frequency`, `duty_cycle` and `data` (microsecond
//! timings alternating mark/space, starting with a mark, at most 1024).
//! Parsed blocks carry `protocol`, `address` and `command` as four hex bytes
//! of a little-endian 32-bit value. Unknown keys are ignored.

use std::fmt;

use elk_proto::pulse::Pulse;

/// Most timings a raw signal may have.
pub const MAX_TIMINGS: usize = 1024;

/// One button.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub name: String,
    pub kind: SignalKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignalKind {
    Raw { frequency_hz: u32, duty_cycle: f32, pulses: Vec<Pulse> },
    Parsed { protocol: String, address: u32, command: u32 },
}

/// Why a file could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlipperError {
    /// 1-based line number for read errors, 0 for write errors.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for FlipperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 { write!(f, "{}", self.message) } else { write!(f, "line {}: {}", self.line, self.message) }
    }
}

impl std::error::Error for FlipperError {}

/// Parses an `.ir` file.
pub fn parse(text: &str) -> Result<Vec<Signal>, FlipperError> {
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim())).filter(|(_, l)| !l.is_empty());
    match lines.next() {
        Some((_, "Filetype: IR signals file" | "Filetype: IR library file")) => {}
        other => return Err(error(other.map_or(1, |(n, _)| n), "not a Flipper IR file")),
    }
    match lines.next() {
        Some((_, "Version: 1")) => {}
        other => return Err(error(other.map_or(2, |(n, _)| n), "unsupported version (only 1)")),
    }
    let mut signals = Vec::new();
    let mut block: Option<Block> = None;
    for (number, line) in lines {
        if line == "#" {
            if let Some(b) = block.take() {
                signals.push(b.finish()?);
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(':').ok_or_else(|| error(number, "expected 'key: value'"))?;
        let b = block.get_or_insert_with(|| Block { start: number, fields: Vec::new() });
        b.fields.push((key.trim().to_string(), value.trim().to_string(), number));
    }
    if let Some(b) = block {
        signals.push(b.finish()?);
    }
    Ok(signals)
}

fn error(line: usize, message: impl Into<String>) -> FlipperError {
    FlipperError { line, message: message.into() }
}

/// The fields of one block with their line numbers.
struct Block {
    start: usize,
    fields: Vec<(String, String, usize)>,
}

impl Block {
    fn get(&self, key: &str) -> Result<(&str, usize), FlipperError> {
        self.fields
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, v, n)| (v.as_str(), *n))
            .ok_or_else(|| error(self.start, format!("missing '{key}'")))
    }

    fn number<T: std::str::FromStr>(&self, key: &str) -> Result<T, FlipperError> {
        let (value, line) = self.get(key)?;
        value.parse().map_err(|_| error(line, format!("'{key}' is not a number")))
    }

    fn hex_u32(&self, key: &str) -> Result<u32, FlipperError> {
        let (value, line) = self.get(key)?;
        let bytes: Vec<u8> = value
            .split_whitespace()
            .map(|b| u8::from_str_radix(b, 16))
            .collect::<Result<_, _>>()
            .map_err(|_| error(line, format!("'{key}' is not hex bytes")))?;
        let bytes: [u8; 4] = bytes.try_into().map_err(|_| error(line, format!("'{key}' needs four bytes")))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn finish(self) -> Result<Signal, FlipperError> {
        let name = self.get("name")?.0.to_string();
        let (kind, line) = self.get("type")?;
        let kind = match kind {
            "raw" => {
                let frequency_hz = self.number("frequency")?;
                let duty_cycle = self.number("duty_cycle")?;
                let (data, data_line) = self.get("data")?;
                let timings: Vec<u32> = data
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| error(data_line, "'data' is not a list of numbers"))?;
                if timings.is_empty() {
                    return Err(error(data_line, "'data' is empty"));
                }
                let pulses = timings
                    .chunks(2)
                    .map(|pair| Pulse { on_us: pair[0], off_us: pair.get(1).copied().unwrap_or(0) })
                    .collect();
                SignalKind::Raw { frequency_hz, duty_cycle, pulses }
            }
            "parsed" => SignalKind::Parsed {
                protocol: self.get("protocol")?.0.to_string(),
                address: self.hex_u32("address")?,
                command: self.hex_u32("command")?,
            },
            other => return Err(error(line, format!("unknown signal type '{other}'"))),
        };
        Ok(Signal { name, kind })
    }
}

/// Writes an `.ir` file (`IR signals file`, version 1).
pub fn format(signals: &[Signal]) -> Result<String, FlipperError> {
    let mut out = String::from("Filetype: IR signals file\nVersion: 1\n");
    for signal in signals {
        let printable = |c: char| (' '..='~').contains(&c);
        if signal.name.is_empty() || !signal.name.chars().all(printable) {
            return Err(error(0, format!("name '{}' must be printable ASCII", signal.name)));
        }
        out.push_str(&format!("#\nname: {}\n", signal.name));
        match &signal.kind {
            SignalKind::Raw { frequency_hz, duty_cycle, pulses } => {
                let mut timings: Vec<u32> = pulses.iter().flat_map(|p| [p.on_us, p.off_us]).collect();
                if timings.last() == Some(&0) {
                    timings.pop();
                }
                if timings.len() > MAX_TIMINGS {
                    return Err(error(
                        0,
                        format!("'{}' has {} timings, Flipper allows {MAX_TIMINGS}", signal.name, timings.len()),
                    ));
                }
                let data: Vec<String> = timings.iter().map(u32::to_string).collect();
                out.push_str(&format!(
                    "type: raw\nfrequency: {frequency_hz}\nduty_cycle: {duty_cycle:.6}\ndata: {}\n",
                    data.join(" ")
                ));
            }
            SignalKind::Parsed { protocol, address, command } => {
                let hex = |v: &u32| v.to_le_bytes().map(|b| format!("{b:02X}")).join(" ");
                out.push_str(&format!(
                    "type: parsed\nprotocol: {protocol}\naddress: {}\ncommand: {}\n",
                    hex(address),
                    hex(command)
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example from the Flipper documentation.
    const DOC_EXAMPLE: &str = "Filetype: IR signals file
Version: 1
#
name: Button_1
type: parsed
protocol: NECext
address: EE 87 00 00
command: 5D A0 00 00
#
name: Button_2
type: raw
frequency: 38000
duty_cycle: 0.330000
data: 504 3432 502 483 500 484 510 502 502 482 501 485 509 1452 504 1458 509 1452 504 481 501 474 509 3420 503
#
name: Button_3
type: parsed
protocol: SIRC
address: 01 00 00 00
command: 15 00 00 00
";

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    #[test]
    fn parses_documentation_example() {
        let signals = parse(DOC_EXAMPLE).unwrap();
        assert_eq!(signals.len(), 3);
        assert_eq!(
            signals[0],
            Signal {
                name: "Button_1".into(),
                kind: SignalKind::Parsed { protocol: "NECext".into(), address: 0x87EE, command: 0xA05D },
            }
        );
        let SignalKind::Raw { frequency_hz, duty_cycle, pulses } = &signals[1].kind else { panic!("raw") };
        assert_eq!((*frequency_hz, *duty_cycle), (38000, 0.33));
        assert_eq!(pulses.len(), 13, "25 timings -> 12 pairs plus a final mark");
        assert_eq!(pulses[0], p(504, 3432));
        assert_eq!(pulses[12], p(503, 0));
        assert_eq!(signals[2].kind, SignalKind::Parsed { protocol: "SIRC".into(), address: 1, command: 0x15 });
    }

    #[test]
    fn documentation_example_round_trips() {
        assert_eq!(format(&parse(DOC_EXAMPLE).unwrap()).unwrap(), DOC_EXAMPLE);
    }

    #[test]
    fn library_files_and_comments_are_accepted() {
        let text = "Filetype: IR library file\nVersion: 1\n# a comment\n#\nname: Power\ntype: parsed\nprotocol: NEC\naddress: 04 00 00 00\ncommand: 08 00 00 00\n";
        assert_eq!(parse(text).unwrap().len(), 1);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let text = "Filetype: IR signals file\nVersion: 1\n#\nname: A\ntype: parsed\nprotocol: NEC\nfuture: x\naddress: 04 00 00 00\ncommand: 08 00 00 00\n";
        assert_eq!(parse(text).unwrap().len(), 1);
    }

    #[test]
    fn read_errors_name_the_line() {
        let err = |t: &str| parse(t).unwrap_err().line;
        assert_eq!(err("Version: 1\n"), 1, "missing file type");
        assert_eq!(err("Filetype: IR signals file\nVersion: 2\n"), 2, "unsupported version");
        assert_eq!(
            err("Filetype: IR signals file\nVersion: 1\n#\nname: A\ntype: raw\nfrequency: x\n"),
            6,
            "bad number"
        );
        assert_eq!(
            err("Filetype: IR signals file\nVersion: 1\n#\nname: A\ntype: parsed\nprotocol: NEC\naddress: 04 00\ncommand: 08 00 00 00\n"),
            7,
            "address needs four bytes"
        );
        assert_eq!(err("Filetype: IR signals file\nVersion: 1\n#\nname: A\ntype: raw\nfrequency: 38000\n"), 4, "missing data");
    }

    #[test]
    fn write_rejects_what_flipper_cannot_read() {
        let raw = |name: &str, n: usize| Signal {
            name: name.into(),
            kind: SignalKind::Raw { frequency_hz: 38000, duty_cycle: 0.33, pulses: vec![p(500, 500); n] },
        };
        assert!(format(&[raw("ok", 512)]).is_ok());
        assert!(format(&[raw("too long", 513)]).is_err(), "1026 timings");
        assert!(format(&[raw("Lautstärke", 1)]).is_err(), "non-ASCII name");
        assert!(format(&[raw("", 1)]).is_err(), "empty name");
    }

    #[test]
    fn trailing_zero_space_is_not_written() {
        let s = Signal {
            name: "x".into(),
            kind: SignalKind::Raw { frequency_hz: 38000, duty_cycle: 0.33, pulses: vec![p(9000, 4500), p(560, 0)] },
        };
        assert!(format(&[s]).unwrap().contains("data: 9000 4500 560\n"));
    }
}
