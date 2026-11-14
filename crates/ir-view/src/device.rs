//! Connection status shown in the sidebar and on the device page.
//!
//! Exists to derive, from session events alone, what the user sees (state,
//! device line, location and facts, dongle protocol and identify reply, when
//! it was connected and how many errors occurred since, last error, the last
//! transmit, the "waiting for an IR signal" banner of design decision 14)
//! and which actions are enabled.

use elk_proto::identify::Protocol;
use ir_session::session::{DeviceInfo, Event, State};
use ir_transport::recorder;

/// Banner text while the dongle waits for an IR signal.
pub const RECOVERING_BANNER: &str =
    "The dongle is waiting for an IR signal. Point any remote at it and press a button, or replug it.";

/// What the pages show about the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceView {
    state: State,
    device: Option<DeviceInfo>,
    protocol: Option<Protocol>,
    identify_reply: Option<Vec<u8>>,
    connected_since_ms: Option<u64>,
    errors: usize,
    last_error: Option<String>,
    last_sent: Option<String>,
    /// Wall time of the latest transfer (not shown itself).
    last_transfer_ms: Option<u64>,
}

impl Default for DeviceView {
    fn default() -> Self {
        Self {
            state: State::Disconnected,
            device: None,
            protocol: None,
            identify_reply: None,
            connected_since_ms: None,
            errors: 0,
            last_error: None,
            last_sent: None,
            last_transfer_ms: None,
        }
    }
}

impl DeviceView {
    /// Updates from one session event; returns true when anything shown changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        let before = self.clone();
        match event {
            Event::State(state) => {
                self.state = *state;
                if *state == State::Disconnected {
                    self.device = None;
                    self.protocol = None;
                    self.identify_reply = None;
                    self.connected_since_ms = None;
                    self.errors = 0;
                    self.last_sent = None;
                }
            }
            Event::Connected { device, protocol } => {
                if self.device.is_none() {
                    self.connected_since_ms = self.last_transfer_ms;
                    self.errors = 0;
                }
                self.device = Some(device.clone());
                self.protocol = Some(*protocol);
            }
            Event::Transfer(entry) => {
                self.last_transfer_ms = Some(entry.wall_ms);
                match &entry.event {
                    recorder::Event::In(bytes) if bytes.starts_with(&[0xFC; 4]) => {
                        self.identify_reply = Some(bytes.clone());
                    }
                    recorder::Event::Error(_) => self.errors += 1,
                    _ => {}
                }
            }
            Event::Error(message) => {
                self.last_error = Some(message.clone());
                self.errors += 1;
            }
            Event::Transmitted { pulses } => self.last_sent = Some(format!("Sent {pulses} pulses.")),
            Event::RawSent { bytes } => self.last_sent = Some(format!("Sent {bytes} raw bytes.")),
            _ => {}
        }
        Self { last_transfer_ms: before.last_transfer_ms, ..self.clone() } != before
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn state_label(&self) -> &'static str {
        match self.state {
            State::Disconnected => "Disconnected",
            State::Ready => "Connected",
            State::Learning => "Waiting for a code",
            State::Capturing => "Capturing",
            State::Recovering => "Waiting for an IR signal",
        }
    }

    /// `Connected · plain`: the state and, when known, the protocol.
    pub fn sidebar_line(&self) -> String {
        match self.protocol_label() {
            Some(protocol) => format!("{} · {protocol}", self.state_label()),
            None => self.state_label().to_string(),
        }
    }

    /// Short location for the sidebar.
    pub fn location(&self) -> Option<&str> {
        self.device.as_ref().map(|d| d.location.as_str()).filter(|l| !l.is_empty())
    }

    /// Labelled details for the device page: the connector's facts, the
    /// protocol and the identify reply.
    pub fn facts(&self) -> Vec<(String, String)> {
        let Some(device) = &self.device else { return Vec::new() };
        let mut facts = device.facts.clone();
        if let Some(protocol) = self.protocol_label() {
            facts.push(("Protocol".to_string(), protocol.to_string()));
        }
        if let Some(reply) = &self.identify_reply {
            facts.push(("Identify".to_string(), recorder::hex(reply)));
        }
        facts
    }

    /// When the dongle was connected (wall-clock Unix milliseconds).
    pub fn connected_since_ms(&self) -> Option<u64> {
        self.connected_since_ms
    }

    /// Errors since connecting.
    pub fn error_count(&self) -> usize {
        self.errors
    }

    pub fn device(&self) -> Option<&str> {
        self.device.as_ref().map(|d| d.description.as_str())
    }

    pub fn protocol_label(&self) -> Option<&'static str> {
        self.protocol.map(|p| match p {
            Protocol::Plain => "plain",
            Protocol::Huffman => "Huffman",
        })
    }

    pub fn banner(&self) -> Option<&'static str> {
        (self.state == State::Recovering).then_some(RECOVERING_BANNER)
    }

    /// Confirmation of the last transmit, until disconnected.
    pub fn last_sent(&self) -> Option<&str> {
        self.last_sent.as_deref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    pub fn can_connect(&self) -> bool {
        self.state == State::Disconnected
    }

    pub fn can_disconnect(&self) -> bool {
        self.state != State::Disconnected
    }

    /// Learn once, start capture and send all need an idle, connected dongle.
    pub fn is_ready(&self) -> bool {
        self.state == State::Ready
    }

    pub fn can_stop_capture(&self) -> bool {
        self.state == State::Capturing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir_session::session::DeviceInfo;
    use ir_transport::ops::LearnOutcome;
    use ir_transport::recorder::{self, Entry};
    use ir_transport::transport::TransportError;
    use std::time::Duration;

    fn info() -> DeviceInfo {
        DeviceInfo {
            description: "Smart IR Blaster (045c:0132)".into(),
            location: "045c:0132 · bus 003 dev 014".into(),
            facts: vec![("Device".into(), "045c:0132 Smart IR Blaster".into())],
        }
    }

    fn transfer(wall_ms: u64, event: recorder::Event) -> Event {
        Event::Transfer(Entry { at: Duration::ZERO, wall_ms, event })
    }

    #[test]
    fn connection_facts_location_and_sidebar_line() {
        let mut v = DeviceView::default();
        assert_eq!(v.sidebar_line(), "Disconnected");
        v.apply(&transfer(1_000, recorder::Event::Out(vec![0xFC; 4])));
        v.apply(&transfer(1_005, recorder::Event::In(vec![0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01])));
        v.apply(&Event::Connected { device: info(), protocol: Protocol::Plain });
        v.apply(&Event::State(State::Ready));
        assert_eq!(v.sidebar_line(), "Connected · plain");
        assert_eq!(v.location(), Some("045c:0132 · bus 003 dev 014"));
        let all = v.facts();
        let facts: Vec<(&str, &str)> = all.iter().map(|(l, x)| (l.as_str(), x.as_str())).collect();
        assert_eq!(
            facts,
            vec![("Device", "045c:0132 Smart IR Blaster"), ("Protocol", "plain"), ("Identify", "FC FC FC FC 70 01")]
        );
        assert_eq!(v.connected_since_ms(), Some(1_005));
    }

    #[test]
    fn errors_are_counted_per_connection() {
        let mut v = connected();
        v.apply(&Event::Error("a".into()));
        v.apply(&transfer(5, recorder::Event::Error(TransportError::Timeout)));
        assert_eq!(v.error_count(), 2);
        v.apply(&transfer(9, recorder::Event::In(vec![0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01])));
        v.apply(&Event::Connected { device: info(), protocol: Protocol::Plain });
        assert_eq!(v.error_count(), 2, "identifying again keeps the connection");
        v.apply(&Event::State(State::Disconnected));
        assert_eq!((v.error_count(), v.connected_since_ms(), v.location()), (0, None, None));
        assert!(v.facts().is_empty());
    }

    fn connected() -> DeviceView {
        let mut v = DeviceView::default();
        v.apply(&Event::Connected { device: info(), protocol: Protocol::Plain });
        v.apply(&Event::State(State::Ready));
        v
    }

    #[test]
    fn starts_disconnected() {
        let v = DeviceView::default();
        assert_eq!(v.state(), State::Disconnected);
        assert_eq!(v.state_label(), "Disconnected");
        assert_eq!(v.device(), None);
        assert_eq!(v.protocol_label(), None);
        assert_eq!(v.banner(), None);
        assert!(v.can_connect());
        assert!(!v.can_disconnect());
        assert!(!v.is_ready());
        assert!(!v.can_stop_capture());
    }

    #[test]
    fn connected_and_ready() {
        let v = connected();
        assert_eq!(v.state_label(), "Connected");
        assert_eq!(v.device(), Some("Smart IR Blaster (045c:0132)"));
        assert_eq!(v.protocol_label(), Some("plain"));
        assert!(!v.can_connect());
        assert!(v.can_disconnect());
        assert!(v.is_ready());
    }

    #[test]
    fn huffman_protocol_label() {
        let mut v = DeviceView::default();
        v.apply(&Event::Connected { device: DeviceInfo::named("x"), protocol: Protocol::Huffman });
        assert_eq!(v.protocol_label(), Some("Huffman"));
    }

    #[test]
    fn capturing_allows_only_stop_and_disconnect() {
        let mut v = connected();
        assert!(v.apply(&Event::State(State::Capturing)));
        assert_eq!(v.state_label(), "Capturing");
        assert!(v.can_stop_capture());
        assert!(v.can_disconnect());
        assert!(!v.is_ready());
        assert!(!v.can_connect());
    }

    #[test]
    fn learning_label() {
        let mut v = connected();
        v.apply(&Event::State(State::Learning));
        assert_eq!(v.state_label(), "Waiting for a code");
        assert!(!v.can_stop_capture());
    }

    #[test]
    fn recovering_shows_the_banner() {
        let mut v = connected();
        v.apply(&Event::State(State::Recovering));
        assert_eq!(v.state_label(), "Waiting for an IR signal");
        assert_eq!(v.banner(), Some(RECOVERING_BANNER));
        assert!(!v.is_ready());
        assert!(v.can_disconnect());
        v.apply(&Event::State(State::Ready));
        assert_eq!(v.banner(), None);
    }

    #[test]
    fn disconnect_forgets_the_device() {
        let mut v = connected();
        v.apply(&Event::State(State::Disconnected));
        assert_eq!(v.device(), None);
        assert_eq!(v.protocol_label(), None);
        assert!(v.can_connect());
    }

    #[test]
    fn errors_are_kept_until_cleared() {
        let mut v = DeviceView::default();
        assert!(v.apply(&Event::Error("no supported ELK Smart dongle found".into())));
        assert_eq!(v.last_error(), Some("no supported ELK Smart dongle found"));
        v.apply(&Event::State(State::Ready));
        assert_eq!(v.last_error(), Some("no supported ELK Smart dongle found"));
        v.clear_error();
        assert_eq!(v.last_error(), None);
    }

    #[test]
    fn sent_codes_are_confirmed() {
        let mut v = connected();
        assert_eq!(v.last_sent(), None);
        assert!(v.apply(&Event::Transmitted { pulses: 68 }));
        assert_eq!(v.last_sent(), Some("Sent 68 pulses."));
        assert!(v.apply(&Event::RawSent { bytes: 4 }));
        assert_eq!(v.last_sent(), Some("Sent 4 raw bytes."));
        v.apply(&Event::State(State::Disconnected));
        assert_eq!(v.last_sent(), None);
    }

    #[test]
    fn unrelated_events_change_nothing() {
        let mut v = connected();
        assert!(!v.apply(&Event::Learned(LearnOutcome::NothingReceived)));
        assert!(!v.apply(&Event::State(State::Ready)), "same state again");
    }
}
