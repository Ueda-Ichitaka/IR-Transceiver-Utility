//! LIRC remote definitions (`lircd.conf`) with raw codes.
//!
//! Exists to exchange codes with LIRC. Format (lirc.org, lircd.conf):
//! `begin remote` ... `end remote` blocks with parameters such as `name`,
//! `flags`, `frequency` and `gap`; raw buttons sit between `begin raw_codes`
//! and `end raw_codes`, each a `name <button>` line followed by microsecond
//! durations alternating pulse/space, starting and ending with a pulse.
//! Lines starting with `#` are comments. Remotes with decoded (non-raw)
//! codes are read without codes.

use std::fmt;

use elk_proto::pulse::Pulse;

/// One remote with its raw buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub frequency_hz: u32,
    pub gap_us: Option<u32>,
    pub codes: Vec<RawCode>,
}

/// One button. A final space (the last pulse's off time) is 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCode {
    pub name: String,
    pub pulses: Vec<Pulse>,
}

/// LIRC's default carrier frequency.
pub const DEFAULT_FREQUENCY_HZ: u32 = 38_000;

/// Why a file could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LircError {
    /// 1-based line number for read errors, 0 for write errors.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for LircError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 { write!(f, "{}", self.message) } else { write!(f, "line {}: {}", self.line, self.message) }
    }
}

impl std::error::Error for LircError {}

/// Parses all remotes in a lircd.conf file.
pub fn parse(text: &str) -> Result<Vec<Remote>, LircError> {
    #[derive(PartialEq)]
    enum Section {
        Outside,
        Remote,
        RawCodes,
        Codes,
    }
    let mut section = Section::Outside;
    let mut remotes = Vec::new();
    let mut remote: Option<(usize, Remote)> = None;
    let mut code: Option<(String, Vec<u32>)> = None;
    let mut last_line = 0;

    fn finish_code(remote: &mut Option<(usize, Remote)>, code: &mut Option<(String, Vec<u32>)>) {
        if let (Some((_, r)), Some((name, timings))) = (remote.as_mut(), code.take()) {
            let pulses = timings
                .chunks(2)
                .map(|pair| Pulse { on_us: pair[0], off_us: pair.get(1).copied().unwrap_or(0) })
                .collect();
            r.codes.push(RawCode { name, pulses });
        }
    }

    for (index, raw_line) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        last_line = number;
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or_default();
        let second = words.next();
        match section {
            Section::Outside => {
                if (first, second) != ("begin", Some("remote")) {
                    return Err(error(number, "expected 'begin remote'"));
                }
                let empty = Remote { name: String::new(), frequency_hz: DEFAULT_FREQUENCY_HZ, gap_us: None, codes: vec![] };
                remote = Some((number, empty));
                section = Section::Remote;
            }
            Section::Remote => {
                let (start, r) = remote.as_mut().expect("inside a remote");
                match (first, second) {
                    ("begin", Some("raw_codes")) => section = Section::RawCodes,
                    ("begin", Some("codes")) => section = Section::Codes,
                    ("end", Some("remote")) => {
                        if r.name.is_empty() {
                            return Err(error(*start, "remote has no name"));
                        }
                        remotes.push(remote.take().expect("inside a remote").1);
                        section = Section::Outside;
                    }
                    ("name", Some(name)) => r.name = name.to_string(),
                    ("frequency", Some(v)) => r.frequency_hz = number_at(v, number)?,
                    ("gap", Some(v)) => r.gap_us = Some(number_at(v, number)?),
                    _ => {}
                }
            }
            Section::RawCodes => match (first, second) {
                ("end", Some("raw_codes")) => {
                    finish_code(&mut remote, &mut code);
                    section = Section::Remote;
                }
                ("name", Some(name)) => {
                    finish_code(&mut remote, &mut code);
                    code = Some((name.to_string(), Vec::new()));
                }
                _ => {
                    let (_, timings) = code.as_mut().ok_or_else(|| error(number, "durations before a button name"))?;
                    for word in line.split_whitespace() {
                        timings.push(number_at(word, number)?);
                    }
                }
            },
            Section::Codes => {
                if (first, second) == ("end", Some("codes")) {
                    section = Section::Remote;
                }
            }
        }
    }
    if section != Section::Outside {
        return Err(error(last_line, "file ends inside a remote"));
    }
    Ok(remotes)
}

fn error(line: usize, message: impl Into<String>) -> LircError {
    LircError { line, message: message.into() }
}

fn number_at(word: &str, line: usize) -> Result<u32, LircError> {
    word.parse().map_err(|_| error(line, format!("'{word}' is not a number")))
}

/// Writes remotes as raw lircd.conf blocks.
pub fn format(remotes: &[Remote]) -> Result<String, LircError> {
    let valid = |name: &str| !name.is_empty() && !name.chars().any(char::is_whitespace);
    let mut out = String::new();
    for remote in remotes {
        if !valid(&remote.name) {
            return Err(error(0, format!("remote name '{}' must be one word", remote.name)));
        }
        out.push_str(&format!(
            "begin remote\n  name  {}\n  flags RAW_CODES\n  eps   30\n  aeps  100\n  frequency {}\n",
            remote.name, remote.frequency_hz
        ));
        if let Some(gap) = remote.gap_us {
            out.push_str(&format!("  gap   {gap}\n"));
        }
        out.push_str("\n  begin raw_codes\n");
        for code in &remote.codes {
            if !valid(&code.name) {
                return Err(error(0, format!("button name '{}' must be one word", code.name)));
            }
            out.push_str(&format!("    name {}\n", code.name));
            let mut timings: Vec<u32> = code.pulses.iter().flat_map(|p| [p.on_us, p.off_us]).collect();
            if timings.last() == Some(&0) {
                timings.pop();
            }
            for line in timings.chunks(6) {
                let words: Vec<String> = line.iter().map(u32::to_string).collect();
                out.push_str(&format!("      {}\n", words.join(" ")));
            }
        }
        out.push_str("  end raw_codes\nend remote\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(on_us: u32, off_us: u32) -> Pulse {
        Pulse { on_us, off_us }
    }

    const SAMPLE: &str = "\
# Recorded with irrecord
begin remote
  name  tv
  flags RAW_CODES
  eps   30
  aeps  100
  frequency 36000
  gap   40000

  begin raw_codes
    name KEY_POWER
      9000 4500 560 560
      560
    name KEY_MUTE
      9000 2250 560
  end raw_codes
end remote

begin remote
  name  decoded
  bits  16
  begin codes
    KEY_1 0x00FF
  end codes
end remote
";

    #[test]
    fn parses_raw_remote_and_skips_decoded_codes() {
        let remotes = parse(SAMPLE).unwrap();
        assert_eq!(remotes.len(), 2);
        assert_eq!(
            remotes[0],
            Remote {
                name: "tv".into(),
                frequency_hz: 36000,
                gap_us: Some(40000),
                codes: vec![
                    RawCode { name: "KEY_POWER".into(), pulses: vec![p(9000, 4500), p(560, 560), p(560, 0)] },
                    RawCode { name: "KEY_MUTE".into(), pulses: vec![p(9000, 2250), p(560, 0)] },
                ],
            }
        );
        assert_eq!(remotes[1].name, "decoded");
        assert!(remotes[1].codes.is_empty());
    }

    #[test]
    fn frequency_defaults_to_38khz() {
        let text = "begin remote\nname r\nflags RAW_CODES\nbegin raw_codes\nname A\n100 200 300\nend raw_codes\nend remote\n";
        let remote = &parse(text).unwrap()[0];
        assert_eq!((remote.frequency_hz, remote.gap_us), (DEFAULT_FREQUENCY_HZ, None));
    }

    #[test]
    fn code_ending_with_space_is_accepted() {
        let text = "begin remote\nname r\nbegin raw_codes\nname A\n100 200\nend raw_codes\nend remote\n";
        assert_eq!(parse(text).unwrap()[0].codes[0].pulses, vec![p(100, 200)]);
    }

    #[test]
    fn format_then_parse_round_trips() {
        let remotes = parse(SAMPLE).unwrap();
        let raw_only = vec![remotes[0].clone()];
        assert_eq!(parse(&format(&raw_only).unwrap()).unwrap(), raw_only);
    }

    #[test]
    fn format_layout() {
        let remote = Remote {
            name: "tv".into(),
            frequency_hz: 38000,
            gap_us: None,
            codes: vec![RawCode { name: "KEY_A".into(), pulses: vec![p(9000, 4500), p(560, 0)] }],
        };
        assert_eq!(
            format(&[remote]).unwrap(),
            "begin remote\n  name  tv\n  flags RAW_CODES\n  eps   30\n  aeps  100\n  frequency 38000\n\n  begin raw_codes\n    name KEY_A\n      9000 4500 560\n  end raw_codes\nend remote\n"
        );
    }

    #[test]
    fn long_codes_wrap_after_six_values() {
        let remote = Remote {
            name: "r".into(),
            frequency_hz: 38000,
            gap_us: None,
            codes: vec![RawCode { name: "A".into(), pulses: vec![p(1, 2); 4] }],
        };
        assert!(format(&[remote]).unwrap().contains("      1 2 1 2 1 2\n      1 2\n"));
    }

    #[test]
    fn read_errors_name_the_line() {
        let line = |t: &str| parse(t).unwrap_err().line;
        assert_eq!(line("begin remote\nname r\nbegin raw_codes\n100 200\n"), 4, "durations before a name");
        assert_eq!(line("begin remote\nname r\nbegin raw_codes\nname A\n100 x\n"), 5, "bad number");
        assert_eq!(line("begin remote\nname r\n"), 2, "missing end remote");
        assert_eq!(line("name r\n"), 1, "outside a remote");
        assert_eq!(line("begin remote\nfrequency 38000\nend remote\n"), 1, "remote without name");
    }

    #[test]
    fn names_with_whitespace_cannot_be_written() {
        let remote = |name: &str, code: &str| Remote {
            name: name.into(),
            frequency_hz: 38000,
            gap_us: None,
            codes: vec![RawCode { name: code.into(), pulses: vec![p(1, 0)] }],
        };
        assert!(format(&[remote("my tv", "A")]).is_err());
        assert!(format(&[remote("tv", "KEY A")]).is_err());
        assert!(format(&[remote("tv", "")]).is_err());
    }
}
