//! Replays recorded dongle traffic (fixtures/*.log) through the real
//! operations via the scripted mock.
//!
//! Exists to pin behaviour observed on real hardware: if protocol code changes
//! in a way that would mis-handle what the 0132 actually sends, these fail.

use std::path::PathBuf;

use elk_proto::command;
use elk_proto::pulse::decode_learned;
use ir_transport::mock::{MockTransport, Step};
use ir_transport::ops::{self, LearnConfig, LearnOutcome};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn parse_hex(s: &str) -> Vec<u8> {
    s.split_whitespace().map(|b| u8::from_str_radix(b, 16).expect("hex byte")).collect()
}

/// Turns logged OUT/IN lines into mock steps. Skips the identify exchange
/// (the first OUT and IN line) when `skip_identify` is set.
fn script_from_log(log: &str, skip_identify: bool) -> Vec<Step> {
    let mut steps = Vec::new();
    for line in log.lines().filter(|l| !l.starts_with('#')) {
        let mut parts = line.split_whitespace();
        let _time = parts.next();
        let dir = parts.next().expect("direction");
        let bytes = parse_hex(&parts.collect::<Vec<_>>().join(" "));
        steps.push(match dir {
            "OUT" => Step::ExpectWrite(bytes),
            "IN" => Step::Reply(bytes),
            other => panic!("unexpected log direction {other}"),
        });
    }
    if skip_identify { steps.split_off(2) } else { steps }
}

const LEARN_LOG: &str = "1794596116-learn-tv-power.log";

#[test]
fn identify_fixture_replays_as_plain() {
    let mut script = vec![Step::Timeout];
    script.extend(script_from_log(&fixture("1794595998-identify.log"), false));
    let mut m = MockTransport::new(script);
    assert_eq!(ops::identify(&mut m), Ok(elk_proto::identify::Protocol::Plain));
    assert!(m.is_done());
}

#[test]
fn learn_fixture_replays_three_complete_codes() {
    let mut m = MockTransport::new(script_from_log(&fixture(LEARN_LOG), true));
    let cfg = LearnConfig { wait_reads: 1, gap_reads: 1 };
    let mut lengths = Vec::new();
    for _ in 0..3 {
        match ops::learn(&mut m, cfg) {
            Ok(LearnOutcome::Complete(code)) => {
                assert_eq!(code.data.len(), code.announced_len);
                lengths.push(code.data.len());
            }
            other => panic!("expected complete code, got {other:?}"),
        }
    }
    assert_eq!(lengths, vec![216, 216, 158]);
    assert!(m.is_done());
}

#[test]
fn learn_fixture_has_63_byte_chunks() {
    let script = script_from_log(&fixture(LEARN_LOG), true);
    assert!(script.iter().all(|s| match s {
        Step::Reply(b) => b.len() <= 63,
        _ => true,
    }));
}

/// This unit sends no `FF FF FF FF` acknowledgement: the first transfer after
/// each learn command is the reply header. (Continuation transfers may still
/// start with FF bytes, which are long-duration data.)
#[test]
fn learn_fixture_first_reply_is_header_without_ack() {
    let script = script_from_log(&fixture(LEARN_LOG), true);
    let first_replies: Vec<&Vec<u8>> = script
        .windows(2)
        .filter_map(|w| match w {
            [Step::ExpectWrite(cmd), Step::Reply(b)] if cmd[..] == command::LEARN => Some(b),
            _ => None,
        })
        .collect();
    assert_eq!(first_replies.len(), 3);
    assert!(first_replies.iter().all(|b| b.starts_with(&command::LEARN)));
}

#[test]
fn learned_fixture_decodes_to_nec_frame_with_repeats() {
    let mut m = MockTransport::new(script_from_log(&fixture(LEARN_LOG), true));
    let Ok(LearnOutcome::Complete(code)) = ops::learn(&mut m, LearnConfig { wait_reads: 1, gap_reads: 1 })
    else {
        panic!("expected complete code");
    };
    let pulses = decode_learned(&code.data).expect("decodes");
    // Leader ~9 ms / 4.5 ms, 32 data bits, stop bit, then NEC repeat frames.
    assert!((8800..9300).contains(&pulses[0].on_us));
    assert!((4300..4700).contains(&pulses[0].off_us));
    let bits: Vec<u8> = pulses[1..33].iter().map(|p| u8::from(p.off_us > 1000)).collect();
    let byte = |k: usize| (0..8).fold(0u8, |acc, j| acc | (bits[k * 8 + j] << j));
    assert_eq!([byte(0), byte(1), byte(2), byte(3)], [0x00, 0xFF, 0x59, 0xA6]);
    let repeats = pulses[34..].iter().filter(|p| (8800..9300).contains(&p.on_us) && p.off_us < 2500).count();
    assert_eq!(repeats, 5);
}
