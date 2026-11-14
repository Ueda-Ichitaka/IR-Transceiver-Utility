//! Connection status shown in the sidebar and on the device page.
//!
//! Exists to derive, from session events alone, what the user sees (state,
//! device line, dongle protocol, last error, the last transmit, the "waiting
//! for an IR signal" banner of design decision 14) and which actions are
//! enabled.

use elk_proto::identify::Protocol;
use ir_session::session::{Event, State};

/// Banner text while the dongle waits for an IR signal.
pub const RECOVERING_BANNER: &str =
    "The dongle is waiting for an IR signal. Point any remote at it and press a button, or replug it.";

/// What the pages show about the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceView {
    state: State,
    device: Option<String>,
    protocol: Option<Protocol>,
    last_error: Option<String>,
    last_sent: Option<String>,
}

impl Default for DeviceView {
    fn default() -> Self {
        Self { state: State::Disconnected, device: None, protocol: None, last_error: None, last_sent: None }
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
                    self.last_sent = None;
                }
            }
            Event::Connected { device, protocol } => {
                self.device = Some(device.clone());
                self.protocol = Some(*protocol);
            }
            Event::Error(message) => self.last_error = Some(message.clone()),
            Event::Transmitted { pulses } => self.last_sent = Some(format!("Sent {pulses} pulses.")),
            Event::RawSent { bytes } => self.last_sent = Some(format!("Sent {bytes} raw bytes.")),
            _ => {}
        }
        *self != before
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

    pub fn device(&self) -> Option<&str> {
        self.device.as_deref()
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
    use ir_transport::ops::LearnOutcome;

    fn connected() -> DeviceView {
        let mut v = DeviceView::default();
        v.apply(&Event::Connected { device: "Smart IR Blaster (045c:0132)".into(), protocol: Protocol::Plain });
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
        v.apply(&Event::Connected { device: "x".into(), protocol: Protocol::Huffman });
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
