//! Testable parts of the elk-probe tool: argument parsing (including the
//! transmit payload) and the text that describes a learn outcome in the
//! fixture log.
//!
//! Exists so the binary itself is only wiring, while its user-facing
//! behaviour is covered by unit tests.

use elk_proto::nec;
use elk_proto::pulse::{self, DecodeError, Pulse};
use ir_codec::Encoding;

use crate::ops::LearnOutcome;
use crate::recorder::hex;

pub const USAGE: &str = "usage: elk-probe identify\n       elk-probe learn [--wait SECONDS] [--count N] [--name NAME]\n       elk-probe transmit (--nec HEX | --learned HEX) [--freq HZ]";

/// What the probe should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeCmd {
    Identify,
    Learn { wait_s: u32, count: u32, name: Option<String> },
    Transmit { payload: Payload, freq_hz: u32 },
}

/// What to transmit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    /// NEC (address, command) byte pairs.
    Nec(Vec<u8>),
    /// Learned bytes in the dongle's plain format.
    Learned(Vec<u8>),
}

pub const DEFAULT_FREQ_HZ: u32 = 38_000;

pub const DEFAULT_WAIT_S: u32 = 30;

/// Parses the arguments after the program name.
pub fn parse_args(args: &[String]) -> Result<ProbeCmd, String> {
    let (cmd, rest) = args.split_first().ok_or("missing command")?;
    match cmd.as_str() {
        "identify" if rest.is_empty() => Ok(ProbeCmd::Identify),
        "identify" => Err("identify takes no options".into()),
        "learn" => parse_learn(rest),
        "transmit" => parse_transmit(rest),
        other => Err(format!("unknown command: {other}")),
    }
}

fn parse_learn(mut rest: &[String]) -> Result<ProbeCmd, String> {
    let (mut wait_s, mut count, mut name) = (DEFAULT_WAIT_S, 1, None);
    while let [flag, tail @ ..] = rest {
        let [value, tail @ ..] = tail else {
            return Err(format!("{flag} needs a value"));
        };
        let number = || value.parse::<u32>().map_err(|_| format!("{flag}: not a number: {value}"));
        match flag.as_str() {
            "--wait" => wait_s = number()?,
            "--count" => count = number()?,
            "--name" => name = Some(value.clone()),
            other => return Err(format!("unknown option: {other}")),
        }
        rest = tail;
    }
    if count == 0 {
        return Err("--count must be at least 1".into());
    }
    if let Some(n) = &name {
        let safe = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        if n.is_empty() || !n.chars().all(safe) {
            return Err("--name may only contain letters, digits, '-' and '_'".into());
        }
    }
    Ok(ProbeCmd::Learn { wait_s, count, name })
}

fn parse_transmit(mut rest: &[String]) -> Result<ProbeCmd, String> {
    let (mut payload, mut freq_hz) = (None, DEFAULT_FREQ_HZ);
    while let [flag, tail @ ..] = rest {
        let [value, tail @ ..] = tail else {
            return Err(format!("{flag} needs a value"));
        };
        let bytes = || ir_codec::parse(value, Encoding::Hex).map_err(|e| format!("{flag}: {e}"));
        let new_payload = match flag.as_str() {
            "--nec" => Some(Payload::Nec(bytes()?)),
            "--learned" => Some(Payload::Learned(bytes()?)),
            "--freq" => {
                freq_hz = value.parse().map_err(|_| format!("--freq: not a number: {value}"))?;
                None
            }
            other => return Err(format!("unknown option: {other}")),
        };
        if new_payload.is_some() {
            if payload.is_some() {
                return Err("give either --nec or --learned, not both".into());
            }
            payload = new_payload;
        }
        rest = tail;
    }
    if freq_hz == 0 {
        return Err("--freq must be above 0".into());
    }
    let payload = payload.ok_or("transmit needs --nec or --learned")?;
    payload.pulses()?;
    Ok(ProbeCmd::Transmit { payload, freq_hz })
}

impl Payload {
    /// Converts the payload to pulses, with a readable error.
    pub fn pulses(&self) -> Result<Vec<Pulse>, String> {
        match self {
            Payload::Nec(b) if b.is_empty() => Err("--nec: no bytes".into()),
            Payload::Nec(b) => nec::encode(b, nec::BitOrder::LsbFirst).map_err(|_| "--nec needs address/command pairs".into()),
            Payload::Learned(b) => {
                pulse::decode_learned(b).map_err(|_| "--learned: code ends unexpectedly".into())
            }
        }
    }
}

/// Log lines describing one learn outcome, including a pulse decode.
pub fn describe_outcome(outcome: &LearnOutcome) -> Vec<String> {
    let (kind, code) = match outcome {
        LearnOutcome::Complete(code) => ("complete", code),
        LearnOutcome::Short(code) => ("short", code),
        LearnOutcome::NothingReceived => return vec!["outcome: nothing received".into()],
    };
    let pulses = match pulse::decode_learned(&code.data) {
        Ok(pulses) => {
            let list: Vec<String> = pulses.iter().map(|p| format!("{}/{}", p.on_us, p.off_us)).collect();
            format!("pulses (on/off us): {}", list.join(" "))
        }
        Err(DecodeError::EndsUnexpectedly) => "pulses: decode failed (code ends unexpectedly)".into(),
    };
    vec![
        format!("outcome: {kind}, announced {}, received {}", code.announced_len, code.data.len()),
        format!("data: {}", hex(&code.data)),
        pulses,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use elk_proto::learn::LearnedCode;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn identify_has_no_options() {
        assert_eq!(parse_args(&args(&["identify"])), Ok(ProbeCmd::Identify));
        assert!(parse_args(&args(&["identify", "--wait", "1"])).is_err());
    }

    #[test]
    fn learn_defaults() {
        assert_eq!(
            parse_args(&args(&["learn"])),
            Ok(ProbeCmd::Learn { wait_s: DEFAULT_WAIT_S, count: 1, name: None })
        );
    }

    #[test]
    fn learn_with_all_options() {
        assert_eq!(
            parse_args(&args(&["learn", "--wait", "10", "--count", "3", "--name", "tv-power"])),
            Ok(ProbeCmd::Learn { wait_s: 10, count: 3, name: Some("tv-power".into()) })
        );
    }

    #[test]
    fn invalid_input_is_rejected() {
        assert!(parse_args(&args(&[])).is_err());
        assert!(parse_args(&args(&["transmit"])).is_err());
        assert!(parse_args(&args(&["learn", "--wait"])).is_err());
        assert!(parse_args(&args(&["learn", "--wait", "x"])).is_err());
        assert!(parse_args(&args(&["learn", "--count", "0"])).is_err());
        assert!(parse_args(&args(&["learn", "--bogus"])).is_err());
    }

    #[test]
    fn name_must_be_filename_safe() {
        assert!(parse_args(&args(&["learn", "--name", "../x"])).is_err());
        assert!(parse_args(&args(&["learn", "--name", "a b"])).is_err());
        assert!(parse_args(&args(&["learn", "--name", ""])).is_err());
    }

    #[test]
    fn transmit_nec_with_default_frequency() {
        assert_eq!(
            parse_args(&args(&["transmit", "--nec", "00 59"])),
            Ok(ProbeCmd::Transmit { payload: Payload::Nec(vec![0x00, 0x59]), freq_hz: DEFAULT_FREQ_HZ })
        );
    }

    #[test]
    fn transmit_learned_with_frequency() {
        assert_eq!(
            parse_args(&args(&["transmit", "--learned", "FF2337", "--freq", "36000"])),
            Ok(ProbeCmd::Transmit { payload: Payload::Learned(vec![0xFF, 0x23, 0x37]), freq_hz: 36000 })
        );
    }

    #[test]
    fn transmit_rejects_bad_input() {
        assert!(parse_args(&args(&["transmit"])).is_err());
        assert!(parse_args(&args(&["transmit", "--nec", "00"])).is_err(), "NEC needs pairs");
        assert!(parse_args(&args(&["transmit", "--nec", "zz"])).is_err());
        assert!(parse_args(&args(&["transmit", "--nec", ""])).is_err(), "empty");
        assert!(parse_args(&args(&["transmit", "--nec", "0059", "--learned", "23"])).is_err());
        assert!(parse_args(&args(&["transmit", "--learned", "23FF"])).is_err(), "must decode");
        assert!(parse_args(&args(&["transmit", "--nec", "0059", "--freq", "0"])).is_err());
    }

    #[test]
    fn describes_complete_code_with_pulses() {
        let o = LearnOutcome::Complete(LearnedCode { announced_len: 3, data: vec![0x23, 0x69, 0x23] });
        assert_eq!(
            describe_outcome(&o),
            vec![
                "outcome: complete, announced 3, received 3".to_string(),
                "data: 23 69 23".to_string(),
                "pulses (on/off us): 560/1680 560/0".to_string(),
            ]
        );
    }

    #[test]
    fn describes_short_code_and_decode_failure() {
        let o = LearnOutcome::Short(LearnedCode { announced_len: 10, data: vec![0x23, 0xFF] });
        assert_eq!(
            describe_outcome(&o),
            vec![
                "outcome: short, announced 10, received 2".to_string(),
                "data: 23 FF".to_string(),
                "pulses: decode failed (code ends unexpectedly)".to_string(),
            ]
        );
    }

    #[test]
    fn describes_nothing_received() {
        assert_eq!(
            describe_outcome(&LearnOutcome::NothingReceived),
            vec!["outcome: nothing received".to_string()]
        );
    }
}
