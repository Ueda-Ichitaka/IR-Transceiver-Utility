//! Testable parts of the elk-probe tool: argument parsing and the text that
//! describes a learn outcome in the fixture log.
//!
//! Exists so the binary itself is only wiring, while its user-facing
//! behaviour is covered by unit tests.

use elk_proto::pulse::{self, DecodeError};

use crate::ops::LearnOutcome;
use crate::recorder::hex;

pub const USAGE: &str = "usage: elk-probe identify\n       elk-probe learn [--wait SECONDS] [--count N] [--name NAME]";

/// What the probe should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeCmd {
    Identify,
    Learn { wait_s: u32, count: u32, name: Option<String> },
}

pub const DEFAULT_WAIT_S: u32 = 30;

/// Parses the arguments after the program name.
pub fn parse_args(args: &[String]) -> Result<ProbeCmd, String> {
    let (cmd, rest) = args.split_first().ok_or("missing command")?;
    match cmd.as_str() {
        "identify" if rest.is_empty() => Ok(ProbeCmd::Identify),
        "identify" => Err("identify takes no options".into()),
        "learn" => parse_learn(rest),
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
