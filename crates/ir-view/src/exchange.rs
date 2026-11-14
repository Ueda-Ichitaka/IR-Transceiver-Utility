//! Converting library codes and captured signals to and from other tools'
//! formats (design decisions 17 and 18): LIRC lircd.conf, Flipper `.ir` and
//! Pronto hex.
//!
//! Exists to put the format rules in one tested place: codes become pulse
//! timings plus a carrier (the global default where a code has none); raw USB
//! bytes have no timings and are skipped and reported. LIRC gets one remote
//! per carrier and one-word, unique button names. Flipper gets NEC codes as
//! parsed signals and everything else as raw signals with Flipper's common
//! duty cycle. Pronto holds one code.

use elk_proto::nec::{self, NecFrame};
use elk_proto::protocols::Decoded;
use elk_proto::pulse::{Pulse, decode_learned};
use ir_formats::{flipper, lirc, pronto};
use ir_library::model::{Code, Library, Payload, iso_utc};

use crate::capture::SignalRow;
use crate::library::from_signal;

/// Flipper's `INFRARED_COMMON_DUTY_CYCLE` (lib/infrared/encoder_decoder/infrared.h).
pub const FLIPPER_DUTY_CYCLE: f32 = 0.33;
/// Name of exported LIRC remotes.
pub const LIRC_REMOTE: &str = "ir-transceiver";

/// One code ready for export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub pulses: Vec<Pulse>,
    pub carrier_hz: u32,
    /// Set for NEC payloads, which Flipper stores as parsed signals.
    pub nec: Option<NecFrame>,
}

/// Exported text plus the names of codes that could not be exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub text: String,
    pub skipped: Vec<String>,
}

/// A code read from a file, not yet in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct NewCode {
    pub name: String,
    pub payload: Payload,
    pub carrier_hz: Option<u32>,
    pub tags: Vec<String>,
}

/// The export form of a library code; `None` for raw USB bytes and learned
/// data that does not decode.
pub fn code_item(code: &Code, default_carrier_hz: u32) -> Option<Item> {
    let (pulses, nec) = match &code.payload {
        Payload::Learned { data } => (decode_learned(data).ok()?, None),
        Payload::Pulses { pulses } => (pulses.iter().map(|&[on_us, off_us]| Pulse { on_us, off_us }).collect(), None),
        Payload::Nec { address, command, extended } => (
            nec::encode_command(*address, *command, *extended),
            Some(NecFrame::Command { address: *address, command: *command, extended: *extended }),
        ),
        Payload::Raw { .. } => return None,
    };
    Some(Item { name: code.name.clone(), pulses, carrier_hz: code.carrier_hz.unwrap_or(default_carrier_hz), nec })
}

/// The export form of a captured signal, named like a saved signal.
pub fn signal_item(row: &SignalRow, default_carrier_hz: u32) -> Option<Item> {
    let pulses = row.pulses.clone()?;
    Some(Item { name: from_signal(row).0, pulses, carrier_hz: default_carrier_hz, nec: None })
}

/// One word for LIRC: whitespace becomes `_`.
fn lirc_word(name: &str) -> String {
    let word: String = name.chars().map(|c| if c.is_whitespace() { '_' } else { c }).collect();
    if word.is_empty() { "code".into() } else { word }
}

pub fn to_lirc(items: &[Item]) -> Result<String, String> {
    let mut carriers: Vec<u32> = Vec::new();
    for item in items {
        if !carriers.contains(&item.carrier_hz) {
            carriers.push(item.carrier_hz);
        }
    }
    let remotes: Vec<lirc::Remote> = carriers
        .iter()
        .map(|&carrier| {
            let mut names: Vec<String> = Vec::new();
            let codes = items
                .iter()
                .filter(|i| i.carrier_hz == carrier)
                .map(|i| {
                    let base = lirc_word(&i.name);
                    let mut name = base.clone();
                    let mut n = 2;
                    while names.contains(&name) {
                        name = format!("{base}_{n}");
                        n += 1;
                    }
                    names.push(name.clone());
                    lirc::RawCode { name, pulses: i.pulses.clone() }
                })
                .collect();
            let name = if carriers.len() == 1 { LIRC_REMOTE.to_string() } else { format!("{LIRC_REMOTE}-{carrier}") };
            lirc::Remote { name, frequency_hz: carrier, gap_us: None, codes }
        })
        .collect();
    lirc::format(&remotes).map_err(|e| e.to_string())
}

pub fn to_flipper(items: &[Item]) -> Result<String, String> {
    let signals: Vec<flipper::Signal> = items
        .iter()
        .map(|i| {
            let parsed = i.nec.and_then(|frame| flipper::decoded_to_parsed(&Decoded::Nec(frame)));
            let kind = match parsed {
                Some((protocol, address, command)) => {
                    flipper::SignalKind::Parsed { protocol: protocol.into(), address, command }
                }
                None => flipper::SignalKind::Raw {
                    frequency_hz: i.carrier_hz,
                    duty_cycle: FLIPPER_DUTY_CYCLE,
                    pulses: i.pulses.clone(),
                },
            };
            flipper::Signal { name: i.name.clone(), kind }
        })
        .collect();
    flipper::format(&signals).map_err(|e| e.to_string())
}

pub fn to_pronto(item: &Item) -> Result<String, String> {
    let code = pronto::Pronto { carrier_hz: item.carrier_hz, once: item.pulses.clone(), repeat: Vec::new() };
    pronto::format(&code).map_err(|e| e.to_string())
}

fn pairs(pulses: &[Pulse]) -> Vec<[u32; 2]> {
    pulses.iter().map(|p| [p.on_us, p.off_us]).collect()
}

pub fn from_lirc(text: &str) -> Result<Vec<NewCode>, String> {
    let remotes = lirc::parse(text).map_err(|e| e.to_string())?;
    Ok(remotes
        .iter()
        .flat_map(|remote| {
            remote.codes.iter().map(|code| NewCode {
                name: code.name.clone(),
                payload: Payload::Pulses { pulses: pairs(&code.pulses) },
                carrier_hz: Some(remote.frequency_hz),
                tags: vec![remote.name.clone()],
            })
        })
        .collect())
}

pub fn from_flipper(text: &str) -> Result<Vec<NewCode>, String> {
    flipper::parse(text)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|signal| {
            let (pulses, carrier) = match signal.kind {
                flipper::SignalKind::Raw { frequency_hz, pulses, .. } => (pulses, frequency_hz),
                flipper::SignalKind::Parsed { protocol, address, command } => {
                    flipper::parsed_to_pulses(&protocol, address, command)
                        .map_err(|e| format!("{}: {e}", signal.name))?
                }
            };
            Ok(NewCode {
                name: signal.name,
                payload: Payload::Pulses { pulses: pairs(&pulses) },
                carrier_hz: Some(carrier),
                tags: Vec::new(),
            })
        })
        .collect()
}

/// Reads one Pronto code, named `name`.
pub fn from_pronto(text: &str, name: &str) -> Result<Vec<NewCode>, String> {
    let code = pronto::parse(text).map_err(|e| e.to_string())?;
    let pulses: Vec<Pulse> = code.once.iter().chain(&code.repeat).copied().collect();
    Ok(vec![NewCode {
        name: name.to_string(),
        payload: Payload::Pulses { pulses: pairs(&pulses) },
        carrier_hz: Some(code.carrier_hz),
        tags: Vec::new(),
    }])
}

/// Adds imported codes to the library, stamped with `now_secs` (Unix
/// seconds); returns how many were added.
pub fn add_all(library: &mut Library, codes: Vec<NewCode>, now_secs: u64) -> usize {
    let count = codes.len();
    for new in codes {
        let id = library.add(new.name, new.payload);
        let code = library.get_mut(id).expect("just added");
        code.carrier_hz = new.carrier_hz;
        code.tags = new.tags;
        code.added = Some(iso_utc(now_secs));
    }
    count
}

/// Exports library codes with `to_text`, collecting skipped names.
pub fn export_codes<'a>(
    codes: impl IntoIterator<Item = &'a Code>,
    default_carrier_hz: u32,
    to_text: impl Fn(&[Item]) -> Result<String, String>,
) -> Result<Exported, String> {
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    for code in codes {
        match code_item(code, default_carrier_hz) {
            Some(item) => items.push(item),
            None => skipped.push(code.name.clone()),
        }
    }
    Ok(Exported { text: to_text(&items)?, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const LEARN_LOG: &str = include_str!("../../../fixtures/1794596116-learn-tv-power.log");

    fn fixture_data() -> Vec<u8> {
        let line = LEARN_LOG.lines().find_map(|l| l.strip_prefix("# data:")).expect("data line");
        line.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    fn code(name: &str, payload: Payload) -> Code {
        Code { id: 1, name: name.into(), tags: vec![], carrier_hz: None, repeat: None, payload, notes: String::new(), added: None }
    }

    fn item(name: &str, carrier_hz: u32) -> Item {
        Item { name: name.into(), pulses: vec![Pulse { on_us: 560, off_us: 560 }, Pulse { on_us: 560, off_us: 0 }], carrier_hz, nec: None }
    }

    #[test]
    fn learned_codes_become_pulses_with_the_default_carrier() {
        let it = code_item(&code("TV", Payload::Learned { data: fixture_data() }), 38_000).unwrap();
        assert_eq!(it.pulses, decode_learned(&fixture_data()).unwrap());
        assert_eq!((it.name.as_str(), it.carrier_hz, it.nec), ("TV", 38_000, None));
    }

    #[test]
    fn nec_and_pulse_codes() {
        let mut c = code("Mute", Payload::Nec { address: 0, command: 0x59, extended: false });
        c.carrier_hz = Some(40_000);
        let it = code_item(&c, 38_000).unwrap();
        assert_eq!(it.pulses, nec::encode_command(0, 0x59, false));
        assert_eq!(it.carrier_hz, 40_000);
        assert_eq!(it.nec, Some(NecFrame::Command { address: 0, command: 0x59, extended: false }));
        let p = code_item(&code("P", Payload::Pulses { pulses: vec![[100, 200]] }), 38_000).unwrap();
        assert_eq!(p.pulses, vec![Pulse { on_us: 100, off_us: 200 }]);
    }

    #[test]
    fn raw_and_undecodable_codes_are_not_exportable() {
        assert_eq!(code_item(&code("R", Payload::Raw { data: vec![1] }), 38_000), None);
        assert_eq!(code_item(&code("L", Payload::Learned { data: vec![0x23, 0xFF] }), 38_000), None);
    }

    #[test]
    fn signals_are_named_like_saved_signals() {
        let row = SignalRow {
            at: Duration::ZERO,
            wall_ms: None,
            complete: true,
            announced_len: 216,
            data: fixture_data(),
            pulses: decode_learned(&fixture_data()).ok(),
            summary: "NEC addr 0x00 cmd 0x59, NEC repeat x5".into(),
        };
        let it = signal_item(&row, 36_000).unwrap();
        assert_eq!((it.name.as_str(), it.carrier_hz), ("NEC addr 0x00 cmd 0x59", 36_000));
        assert_eq!(it.pulses, row.pulses.clone().unwrap());
        assert_eq!(signal_item(&SignalRow { pulses: None, ..row }, 36_000), None);
    }

    #[test]
    fn lirc_groups_by_carrier_with_unique_one_word_names() {
        let text = to_lirc(&[item("TV power", 38_000), item("TV power", 38_000), item("Fan", 36_000)]).unwrap();
        let remotes = lirc::parse(&text).unwrap();
        let summary: Vec<(String, u32, Vec<String>)> = remotes
            .iter()
            .map(|r| (r.name.clone(), r.frequency_hz, r.codes.iter().map(|c| c.name.clone()).collect()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("ir-transceiver-38000".to_string(), 38_000, vec!["TV_power".to_string(), "TV_power_2".to_string()]),
                ("ir-transceiver-36000".to_string(), 36_000, vec!["Fan".to_string()]),
            ]
        );
        let single = lirc::parse(&to_lirc(&[item("A", 38_000)]).unwrap()).unwrap();
        assert_eq!(single[0].name, LIRC_REMOTE);
    }

    #[test]
    fn flipper_uses_parsed_nec_and_raw_otherwise() {
        let nec_item = Item { nec: Some(NecFrame::Command { address: 4, command: 8, extended: false }), ..item("Power", 38_000) };
        let text = to_flipper(&[nec_item, item("Fan", 36_000)]).unwrap();
        let signals = flipper::parse(&text).unwrap();
        assert_eq!(signals[0].kind, flipper::SignalKind::Parsed { protocol: "NEC".into(), address: 4, command: 8 });
        assert_eq!(
            signals[1].kind,
            flipper::SignalKind::Raw { frequency_hz: 36_000, duty_cycle: 0.33, pulses: item("Fan", 36_000).pulses }
        );
    }

    #[test]
    fn pronto_round_trip() {
        let text = to_pronto(&item("A", 38_000)).unwrap();
        let back = pronto::parse(&text).unwrap();
        assert_eq!(back.once.len() + back.repeat.len(), 2);
        assert!((back.carrier_hz as i64 - 38_000).abs() < 500, "{}", back.carrier_hz);
    }

    #[test]
    fn lirc_import_tags_codes_with_the_remote() {
        let text = to_lirc(&[item("power", 36_000)]).unwrap().replace(LIRC_REMOTE, "tv");
        assert_eq!(
            from_lirc(&text).unwrap(),
            vec![NewCode {
                name: "power".into(),
                payload: Payload::Pulses { pulses: vec![[560, 560], [560, 0]] },
                carrier_hz: Some(36_000),
                tags: vec!["tv".into()],
            }]
        );
        assert!(from_lirc("begin remote\n").unwrap_err().starts_with("line"));
    }

    #[test]
    fn flipper_import_turns_parsed_signals_into_pulses() {
        let nec_item = Item { nec: Some(NecFrame::Command { address: 0, command: 0x59, extended: false }), ..item("Power", 38_000) };
        let codes = from_flipper(&to_flipper(&[nec_item, item("Fan", 36_000)]).unwrap()).unwrap();
        assert_eq!(codes[0].name, "Power");
        let expected: Vec<[u32; 2]> = nec::encode_command(0, 0x59, false).iter().map(|p| [p.on_us, p.off_us]).collect();
        assert_eq!(codes[0].payload, Payload::Pulses { pulses: expected });
        assert_eq!(codes[0].carrier_hz, Some(38_000));
        assert_eq!(codes[1].payload, Payload::Pulses { pulses: vec![[560, 560], [560, 0]] });
        assert_eq!(codes[1].carrier_hz, Some(36_000));
        assert!(from_flipper("hello").is_err());
    }

    #[test]
    fn pronto_import_names_the_code() {
        let text = to_pronto(&item("A", 38_000)).unwrap();
        let codes = from_pronto(&text, "Remote button").unwrap();
        assert_eq!(codes.len(), 1);
        assert_eq!(codes[0].name, "Remote button");
        assert!(matches!(&codes[0].payload, Payload::Pulses { pulses } if pulses.len() == 2));
        assert!(from_pronto("zz", "x").is_err());
    }

    #[test]
    fn add_all_assigns_ids_and_keeps_fields() {
        let mut lib = Library::default();
        let added = add_all(
            &mut lib,
            vec![NewCode { name: "a".into(), payload: Payload::Raw { data: vec![1] }, carrier_hz: Some(36_000), tags: vec!["tv".into()] }],
            0,
        );
        assert_eq!(added, 1);
        let c = lib.get(1).unwrap();
        assert_eq!((c.name.as_str(), c.carrier_hz, c.tags.clone()), ("a", Some(36_000), vec!["tv".to_string()]));
        assert_eq!(c.added.as_deref(), Some("1970-01-01T00:00:00Z"));
    }

    #[test]
    fn export_reports_skipped_codes() {
        let codes = [code("Raw", Payload::Raw { data: vec![1] }), code("P", Payload::Pulses { pulses: vec![[560, 0]] })];
        let exported = export_codes(&codes, 38_000, to_flipper).unwrap();
        assert_eq!(exported.skipped, vec!["Raw".to_string()]);
        assert_eq!(flipper::parse(&exported.text).unwrap().len(), 1);
    }
}
