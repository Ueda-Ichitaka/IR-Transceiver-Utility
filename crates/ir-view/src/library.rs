//! Library page logic.
//!
//! Exists to show saved codes as one line each (what the payload is,
//! carrier, size) with an icon per payload kind and "learned" or "added"
//! before their date, to turn a captured signal into a new code, and to edit a
//! code's name, tags (design decision 23), carrier and repeat overrides
//! (decisions 12, 13) and notes through a form that validates its fields.

use elk_proto::nec::NecFrame;
use elk_proto::protocols::Decoded;
use elk_proto::protocols::describe::describe_pulses;
use elk_proto::pulse::{Pulse, decode_learned};
use ir_library::model::{Code, Payload, RepeatMode};

use crate::capture::{SignalRow, UNDECODABLE};
use crate::send::parse_carrier;

/// Name given to a saved signal whose protocol is not recognised.
const UNNAMED: &str = "Learned code";

/// One line describing a code, e.g.
/// `NEC addr 0x00 cmd 0x59, NEC repeat x5 · 38 kHz · 68 B`.
pub fn summary(code: &Code) -> String {
    let carrier = carrier_text(code.carrier_hz);
    match &code.payload {
        Payload::Learned { data } => {
            let what = decode_learned(data).map_or_else(|_| UNDECODABLE.to_string(), |p| describe_pulses(&p));
            format!("{what} · {carrier} · {} B", data.len())
        }
        Payload::Pulses { pulses } => {
            let pulses: Vec<Pulse> = pulses.iter().map(|&[on_us, off_us]| Pulse { on_us, off_us }).collect();
            format!("{} · {carrier} · {} pulses", describe_pulses(&pulses), pulses.len())
        }
        Payload::Nec { address, command, extended } => {
            let frame = Decoded::Nec(NecFrame::Command { address: *address, command: *command, extended: *extended });
            format!("{frame} · {carrier}")
        }
        Payload::Raw { data } => format!("Raw USB bytes · {} B", data.len()),
    }
}

/// Carrier as shown in lists: `38 kHz`, `36.7 kHz`, or `default carrier`.
pub fn carrier_text(carrier_hz: Option<u32>) -> String {
    match carrier_hz {
        None => "default carrier".into(),
        Some(hz) if hz % 1000 == 0 => format!("{} kHz", hz / 1000),
        Some(hz) => format!("{:.1} kHz", f64::from(hz) / 1000.0),
    }
}

/// Splits comma-separated tags, trimming them and dropping empty and
/// repeated ones.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_string());
        }
    }
    tags
}

/// Name and payload for saving a captured signal.
pub fn from_signal(row: &SignalRow) -> (String, Payload) {
    let first = row.summary.split(", ").next().unwrap_or_default();
    let first = match first.rsplit_once(" x") {
        Some((name, count)) if !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => first,
    };
    let name = if first.is_empty() || first == "unknown" || first == UNDECODABLE { UNNAMED } else { first };
    (name.to_string(), Payload::Learned { data: row.data.clone() })
}

/// Theme icon for a code's payload kind.
pub fn payload_icon(payload: &Payload) -> &'static str {
    match payload {
        Payload::Learned { .. } => "media-record",
        Payload::Pulses { .. } => "view-statistics",
        Payload::Nec { .. } => "input-tablet",
        Payload::Raw { .. } => "drive-removable-media-usb",
    }
}

/// `learned` for learned codes, `added` for the others (shown before the
/// date the code was added).
pub fn added_verb(payload: &Payload) -> &'static str {
    if matches!(payload, Payload::Learned { .. }) { "learned" } else { "added" }
}

/// The fields of the edit dialog as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditForm {
    pub name: String,
    pub tags_text: String,
    /// Empty uses the default.
    pub carrier_text: String,
    /// `None` uses the default.
    pub repeat: Option<RepeatMode>,
    pub notes: String,
}

impl EditForm {
    pub fn from_code(code: &Code) -> Self {
        Self {
            name: code.name.clone(),
            tags_text: code.tags.join(", "),
            carrier_text: code.carrier_hz.map_or_else(String::new, |hz| hz.to_string()),
            repeat: code.repeat,
            notes: code.notes.clone(),
        }
    }

    /// Writes the form into `code`; leaves it unchanged on an error.
    pub fn apply(&self, code: &mut Code) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("the name is empty".into());
        }
        let carrier_hz = parse_carrier(&self.carrier_text)?;
        code.name = name.to_string();
        code.tags = parse_tags(&self.tags_text);
        code.carrier_hz = carrier_hz;
        code.repeat = self.repeat;
        code.notes = self.notes.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elk_proto::pulse::Pulse;
    use std::time::Duration;

    const LEARN_LOG: &str = include_str!("../../../fixtures/1794596116-learn-tv-power.log");

    fn fixture_data() -> Vec<u8> {
        let line = LEARN_LOG.lines().find_map(|l| l.strip_prefix("# data:")).expect("data line");
        line.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    fn code(payload: Payload) -> Code {
        Code { id: 1, name: "x".into(), tags: vec![], carrier_hz: None, repeat: None, payload, notes: String::new(), added: None }
    }

    #[test]
    fn carrier_texts() {
        assert_eq!(carrier_text(Some(38_000)), "38 kHz");
        assert_eq!(carrier_text(Some(36_700)), "36.7 kHz");
        assert_eq!(carrier_text(Some(455_000)), "455 kHz");
        assert_eq!(carrier_text(None), "default carrier");
    }

    #[test]
    fn summaries_per_payload_kind() {
        let data = fixture_data();
        let len = data.len();
        let mut learned = code(Payload::Learned { data });
        learned.carrier_hz = Some(38_000);
        assert_eq!(summary(&learned), format!("NEC addr 0x00 cmd 0x59, NEC repeat x5 · 38 kHz · {len} B"));
        assert_eq!(
            summary(&code(Payload::Learned { data: vec![0x23, 0xFF] })),
            "cannot be decoded · default carrier · 2 B"
        );
        assert_eq!(
            summary(&code(Payload::Pulses { pulses: vec![[300, 300], [300, 20_000]] })),
            "unknown · default carrier · 2 pulses"
        );
        assert_eq!(
            summary(&code(Payload::Nec { address: 0xEF00, command: 3, extended: true })),
            "NEC-ext addr 0xEF00 cmd 0x03 · default carrier"
        );
        assert_eq!(summary(&code(Payload::Raw { data: vec![0xFC; 4] })), "Raw USB bytes · 4 B");
    }

    #[test]
    fn icons_and_verbs_per_payload_kind() {
        let learned = Payload::Learned { data: vec![] };
        let pulses = Payload::Pulses { pulses: vec![] };
        let nec = Payload::Nec { address: 0, command: 0, extended: false };
        let raw = Payload::Raw { data: vec![] };
        assert_eq!(
            [&learned, &pulses, &nec, &raw].map(payload_icon),
            ["media-record", "view-statistics", "input-tablet", "drive-removable-media-usb"]
        );
        assert_eq!([&learned, &pulses, &nec, &raw].map(added_verb), ["learned", "added", "added", "added"]);
    }

    #[test]
    fn tags_are_trimmed_and_unique() {
        assert_eq!(parse_tags(" tv, living room ,,tv, "), vec!["tv", "living room"]);
        assert_eq!(parse_tags(""), Vec::<String>::new());
    }

    #[test]
    fn captured_signal_becomes_a_learned_code() {
        let row = SignalRow {
            at: Duration::from_millis(1500),
            wall_ms: None,
            complete: true,
            announced_len: 216,
            data: fixture_data(),
            pulses: Some(vec![Pulse { on_us: 1, off_us: 1 }]),
            summary: "NEC addr 0x00 cmd 0x59, NEC repeat x5".into(),
        };
        assert_eq!(from_signal(&row), ("NEC addr 0x00 cmd 0x59".to_string(), Payload::Learned { data: fixture_data() }));
        let unknown = SignalRow { summary: "unknown x2".into(), ..row.clone() };
        assert_eq!(from_signal(&unknown).0, "Learned code");
        let undecodable = SignalRow { summary: crate::capture::UNDECODABLE.into(), ..row };
        assert_eq!(from_signal(&undecodable).0, "Learned code");
    }

    #[test]
    fn edit_form_round_trips_a_code() {
        let mut c = code(Payload::Raw { data: vec![1] });
        c.tags = vec!["tv".into(), "living room".into()];
        c.carrier_hz = Some(36_000);
        c.repeat = Some(RepeatMode::FirstFrame { count: 2 });
        c.notes = "note".into();
        let form = EditForm::from_code(&c);
        assert_eq!(
            form,
            EditForm {
                name: "x".into(),
                tags_text: "tv, living room".into(),
                carrier_text: "36000".into(),
                repeat: Some(RepeatMode::FirstFrame { count: 2 }),
                notes: "note".into(),
            }
        );
        let mut copy = code(Payload::Raw { data: vec![1] });
        form.apply(&mut copy).unwrap();
        assert_eq!(copy, c);
    }

    #[test]
    fn edit_form_clears_overrides_and_trims_the_name() {
        let mut c = code(Payload::Raw { data: vec![1] });
        c.carrier_hz = Some(36_000);
        let form = EditForm { name: "  Power ".into(), carrier_text: "".into(), ..EditForm::from_code(&c) };
        form.apply(&mut c).unwrap();
        assert_eq!((c.name.as_str(), c.carrier_hz), ("Power", None));
    }

    #[test]
    fn invalid_edits_leave_the_code_unchanged() {
        let c = code(Payload::Raw { data: vec![1] });
        let mut target = c.clone();
        let empty_name = EditForm { name: "  ".into(), ..EditForm::from_code(&c) };
        assert_eq!(empty_name.apply(&mut target), Err("the name is empty".into()));
        let bad_carrier = EditForm { carrier_text: "x".into(), ..EditForm::from_code(&c) };
        assert!(bad_carrier.apply(&mut target).is_err());
        assert_eq!(target, c);
    }
}
