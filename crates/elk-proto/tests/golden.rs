//! Checks the Rust transmit path against golden vectors produced by the
//! reference elksmart-comm.c (tests/data/elksmart-comm-golden.txt, generated
//! by tools/golden/generate.sh).
//!
//! Exists to prove the port is byte-identical to the reference for mangle,
//! checksum, NEC encoding, pulse compression and chunked messages.

use std::collections::HashMap;

use elk_proto::nec;
use elk_proto::pulse::{Pulse, decode_learned};
use elk_proto::transmit::{checksum, compress_pulses, mangle, transmit_chunks};

const GOLDEN: &str = include_str!("data/elksmart-comm-golden.txt");
const LEARN_LOG: &str = include_str!("../../../fixtures/1794596116-learn-tv-power.log");

fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
}

/// Expected output of one message block.
struct Expected {
    compressed: Vec<u8>,
    chunks: Vec<Vec<u8>>,
}

struct Golden {
    values: HashMap<String, String>,
    messages: HashMap<String, Expected>,
}

fn golden() -> Golden {
    let mut values = HashMap::new();
    let mut messages = HashMap::new();
    let mut pending_compressed = None;
    let mut current: Option<String> = None;
    for line in GOLDEN.lines() {
        if let Some(label) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let compressed = pending_compressed.take().expect("compressed line before label");
            messages.insert(label.to_string(), Expected { compressed, chunks: Vec::new() });
            current = Some(label.to_string());
            continue;
        }
        let (key, value) = line.split_once(':').expect("key: value");
        if key == "compressed" {
            pending_compressed = Some(hex(value));
        } else if key.starts_with("chunk") {
            let label = current.as_ref().expect("chunk inside a message");
            messages.get_mut(label).unwrap().chunks.push(hex(value));
        } else {
            values.insert(key.to_string(), value.trim().to_string());
        }
    }
    Golden { values, messages }
}

fn p(on_us: u32, off_us: u32) -> Pulse {
    Pulse { on_us, off_us }
}

fn check_message(label: &str, frequency: u32, pulses: &[Pulse]) {
    let g = golden();
    let expected = &g.messages[label];
    assert_eq!(compress_pulses(pulses), expected.compressed, "{label}: compressed");
    assert_eq!(transmit_chunks(frequency, pulses).unwrap(), expected.chunks, "{label}: chunks");
}

#[test]
fn mangle_matches_reference_for_all_bytes() {
    let expected = hex(&golden().values["mangle"]);
    let actual: Vec<u8> = (0..=255u8).map(mangle).collect();
    assert_eq!(actual, expected);
}

#[test]
fn checksum_matches_reference() {
    let g = golden();
    assert_eq!(checksum(&[1, 2, 3, 250, 250, 250]), hex(&g.values["checksum_small"])[0]);
    let c62: Vec<u8> = (0..62).map(|k| 0xF0 + (k % 16) as u8).collect();
    assert_eq!(checksum(&c62), hex(&g.values["checksum_62"])[0]);
}

#[test]
fn nec_pulses_match_reference() {
    let expected: Vec<Pulse> = golden().values["nec_pulses"]
        .split_whitespace()
        .map(|pair| {
            let (on, off) = pair.split_once('/').unwrap();
            p(on.parse().unwrap(), off.parse().unwrap())
        })
        .collect();
    assert_eq!(nec::encode(&[0x00, 0x59]).unwrap(), expected);
}

#[test]
fn nec_message_matches_reference() {
    check_message("nec_00_59_38000", 38000, &nec::encode(&[0x00, 0x59]).unwrap());
}

#[test]
fn single_pulse_message_matches_reference() {
    check_message("single_560_0_38000", 38000, &[p(560, 0)]);
}

#[test]
fn multibyte_values_message_matches_reference() {
    let pulses = [p(9000, 4500), p(560, 560), p(560, 1690), p(560, 560), p(3000, 70000), p(560, 1690)];
    check_message("multibyte_36000", 36000, &pulses);
}

#[test]
fn learned_fixture_message_matches_reference() {
    let data = LEARN_LOG.lines().find_map(|l| l.strip_prefix("# data:")).expect("data line");
    let pulses = decode_learned(&hex(data)).unwrap();
    check_message("learned_fixture_38000", 38000, &pulses);
}
