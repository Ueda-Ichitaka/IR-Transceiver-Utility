//! The device session: a state machine over one dongle connection.
//!
//! Exists so the GUI only sends [`Command`]s and renders [`Event`]s, while
//! the order of USB operations, the dongle's quirks (it stays armed after an
//! unanswered learn) and continuous capture live in one tested place. The
//! session is driven by `handle` (commands) and `tick` (one blocking read of
//! at most 100 ms while learning, capturing or recovering); `worker` runs it
//! on a thread. Every USB transfer is reported as an event.

use std::time::Duration;

use elk_proto::identify::Protocol;
use elk_proto::pulse::Pulse;
use ir_transport::ops::{self, LearnConfig, LearnOutcome, Learner, OpError};
use ir_transport::recorder::{Entry, Recorder};
use ir_transport::transport::{Transport, TransportError};

/// Opens the dongle. Implemented for USB and for tests.
pub trait Connector {
    type Transport: Transport;
    /// Returns the transport and a one-line description of the device.
    fn connect(&mut self) -> Result<(Self::Transport, String), String>;
}

/// Requests from the GUI.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Connect,
    Disconnect,
    /// Learn one code, waiting up to `wait_s` seconds for a button press.
    LearnOnce { wait_s: u32 },
    /// Learn codes continuously until `StopCapture`.
    StartCapture,
    StopCapture,
    Transmit { pulses: Vec<Pulse>, frequency_hz: u32 },
}

/// Session states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Disconnected,
    Ready,
    Learning,
    Capturing,
    /// The dongle is armed but nobody is waiting for its answer (a learn was
    /// cancelled or timed out). It ignores commands until it sees an IR
    /// signal; the session then identifies it again.
    Recovering,
}

/// Notifications to the GUI.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    State(State),
    Connected { device: String, protocol: Protocol },
    /// One USB transfer (or transfer error) with its time offset.
    Transfer(Entry),
    /// A learn finished (also every code during capture).
    Learned(LearnOutcome),
    Transmitted { pulses: usize },
    Error(String),
}

/// Empty reads after a started reply before it counts as short.
pub const GAP_READS: u32 = 10;
/// Reads (of 100 ms) per second, for converting wait times.
pub const READS_PER_SECOND: u32 = 10;

/// What a learner is learning for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LearnMode {
    Once,
    Capture,
}

/// One dongle session.
pub struct Session<C: Connector> {
    connector: C,
    transport: Option<Recorder<C::Transport>>,
    device: String,
    state: State,
    learner: Option<(Learner, LearnMode)>,
    /// In `Recovering`: data seen since entering (the dongle answered).
    recovery_saw_data: bool,
    pause: Box<dyn FnMut(Duration) + Send>,
    events: Vec<Event>,
}

impl<C: Connector> Session<C> {
    /// `pause` is used between transmit chunks (`std::thread::sleep` in
    /// production).
    pub fn new(connector: C, pause: Box<dyn FnMut(Duration) + Send>) -> Self {
        Self {
            connector,
            transport: None,
            device: String::new(),
            state: State::Disconnected,
            learner: None,
            recovery_saw_data: false,
            pause,
            events: Vec::new(),
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// True while the session needs `tick` calls (learning, capturing,
    /// recovering).
    pub fn is_busy(&self) -> bool {
        matches!(self.state, State::Learning | State::Capturing | State::Recovering)
    }

    pub fn handle(&mut self, command: Command) {
        match (command, self.state) {
            (Command::Connect, State::Disconnected) => self.connect(),
            (Command::Disconnect, state) if state != State::Disconnected => {
                self.drop_device();
                self.set_state(State::Disconnected);
            }
            (Command::LearnOnce { wait_s }, State::Ready) => {
                let wait_reads = wait_s.saturating_mul(READS_PER_SECOND).max(1);
                self.start_learner(LearnConfig { wait_reads, gap_reads: GAP_READS }, LearnMode::Once);
            }
            (Command::StartCapture, State::Ready) => {
                self.start_learner(LearnConfig { wait_reads: u32::MAX, gap_reads: GAP_READS }, LearnMode::Capture);
            }
            (Command::StopCapture, State::Capturing) => {
                self.learner = None;
                self.enter_recovering();
            }
            (Command::Transmit { pulses, frequency_hz }, State::Ready) => self.transmit(&pulses, frequency_hz),
            (command, state) => {
                self.events.push(Event::Error(format!("{} is not possible while {state:?}", name(&command))));
            }
        }
    }

    /// Advances learning, capture or recovery by one read.
    pub fn tick(&mut self) {
        match self.state {
            State::Learning | State::Capturing => self.step_learner(),
            State::Recovering => self.step_recovery(),
            State::Disconnected | State::Ready => {}
        }
    }

    /// Events produced since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn connect(&mut self) {
        match self.connector.connect() {
            Ok((transport, device)) => {
                self.transport = Some(Recorder::new(transport));
                self.device = device;
                self.identify();
            }
            Err(e) => self.events.push(Event::Error(e)),
        }
    }

    fn identify(&mut self) {
        let Some(t) = self.transport.as_mut() else { return };
        let result = ops::identify(t);
        self.flush_io();
        match result {
            Ok(protocol) => {
                self.events.push(Event::Connected { device: self.device.clone(), protocol });
                self.set_state(State::Ready);
            }
            Err(e) if is_disconnect(&e) => self.lost(e),
            Err(e) => {
                self.events.push(Event::Error(format!(
                    "the dongle did not answer ({e}); point any remote at it and press a button, or replug it"
                )));
                self.enter_recovering();
            }
        }
    }

    fn start_learner(&mut self, cfg: LearnConfig, mode: LearnMode) {
        let Some(t) = self.transport.as_mut() else { return };
        let result = Learner::start(t, cfg);
        self.flush_io();
        match result {
            Ok(learner) => {
                self.learner = Some((learner, mode));
                self.set_state(if mode == LearnMode::Once { State::Learning } else { State::Capturing });
            }
            Err(e) => self.learn_failed(e),
        }
    }

    fn step_learner(&mut self) {
        let (Some(t), Some((learner, mode))) = (self.transport.as_mut(), self.learner.as_mut()) else {
            return;
        };
        let mode = *mode;
        let result = learner.step(t);
        self.flush_io();
        match result {
            Ok(None) => {}
            Ok(Some(outcome)) => {
                let nothing = outcome == LearnOutcome::NothingReceived;
                self.events.push(Event::Learned(outcome));
                match mode {
                    LearnMode::Once => {
                        self.learner = None;
                        if nothing { self.enter_recovering() } else { self.set_state(State::Ready) }
                    }
                    LearnMode::Capture => {
                        let cfg = LearnConfig { wait_reads: u32::MAX, gap_reads: GAP_READS };
                        self.learner = None;
                        self.start_learner(cfg, LearnMode::Capture);
                    }
                }
            }
            Err(e) => self.learn_failed(e),
        }
    }

    fn step_recovery(&mut self) {
        let Some(t) = self.transport.as_mut() else { return };
        let result = t.read(ops::LEARN_READ_TIMEOUT);
        self.flush_io();
        match result {
            Ok(_) => self.recovery_saw_data = true,
            Err(TransportError::Timeout) if self.recovery_saw_data => self.identify(),
            Err(TransportError::Timeout) => {}
            Err(e) => self.learn_failed(OpError::Transport(e)),
        }
    }

    fn transmit(&mut self, pulses: &[Pulse], frequency_hz: u32) {
        let Some(t) = self.transport.as_mut() else { return };
        let result = ops::transmit(t, frequency_hz, pulses, &mut *self.pause);
        self.flush_io();
        match result {
            Ok(()) => self.events.push(Event::Transmitted { pulses: pulses.len() }),
            Err(e) if is_disconnect(&e) => self.lost(e),
            Err(e) => self.events.push(Event::Error(format!("transmit failed: {e}"))),
        }
    }

    /// A learn-related failure: the dongle's state is unknown afterwards.
    fn learn_failed(&mut self, e: OpError) {
        self.learner = None;
        if is_disconnect(&e) {
            self.lost(e);
        } else {
            self.events.push(Event::Error(format!("learning failed: {e}")));
            self.enter_recovering();
        }
    }

    fn lost(&mut self, e: OpError) {
        self.events.push(Event::Error(e.to_string()));
        self.drop_device();
        self.set_state(State::Disconnected);
    }

    fn drop_device(&mut self) {
        self.learner = None;
        self.transport = None;
    }

    fn enter_recovering(&mut self) {
        self.recovery_saw_data = false;
        self.set_state(State::Recovering);
    }

    fn set_state(&mut self, state: State) {
        self.state = state;
        self.events.push(Event::State(state));
    }

    fn flush_io(&mut self) {
        if let Some(t) = self.transport.as_mut() {
            self.events.extend(t.take_entries().into_iter().map(Event::Transfer));
        }
    }
}

fn is_disconnect(e: &OpError) -> bool {
    *e == OpError::Transport(TransportError::Disconnected)
}

fn name(command: &Command) -> &'static str {
    match command {
        Command::Connect => "Connect",
        Command::Disconnect => "Disconnect",
        Command::LearnOnce { .. } => "Learn",
        Command::StartCapture => "Start capture",
        Command::StopCapture => "Stop capture",
        Command::Transmit { .. } => "Transmit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use elk_proto::command;
    use elk_proto::learn::LearnedCode;
    use ir_transport::mock::{MockTransport, Step};
    use ir_transport::recorder::Event as Io;
    use std::collections::VecDeque;

    const PLAIN: [u8; 6] = [0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01];

    struct ScriptConnector(VecDeque<Result<Vec<Step>, String>>);

    impl Connector for ScriptConnector {
        type Transport = MockTransport;
        fn connect(&mut self) -> Result<(MockTransport, String), String> {
            let script = self.0.pop_front().expect("unexpected connect")?;
            Ok((MockTransport::new(script), "test dongle".into()))
        }
    }

    fn session(scripts: Vec<Result<Vec<Step>, String>>) -> Session<ScriptConnector> {
        Session::new(ScriptConnector(scripts.into()), Box::new(|_| {}))
    }

    fn w(b: [u8; 4]) -> Step {
        Step::ExpectWrite(b.to_vec())
    }

    fn identify_steps() -> Vec<Step> {
        vec![Step::Timeout, w(command::IDENTIFY), Step::Reply(PLAIN.to_vec())]
    }

    fn code(len: u16, data: &[u8]) -> Vec<u8> {
        let mut h = vec![0xFE; 4];
        h.extend_from_slice(&len.to_be_bytes());
        h.extend_from_slice(data);
        h
    }

    /// Events without transfer entries.
    fn non_io(events: Vec<Event>) -> Vec<Event> {
        events.into_iter().filter(|e| !matches!(e, Event::Transfer(_))).collect()
    }

    fn connected(steps: Vec<Step>) -> Session<ScriptConnector> {
        let mut script = identify_steps();
        script.extend(steps);
        let mut s = session(vec![Ok(script)]);
        s.handle(Command::Connect);
        s.take_events();
        s
    }

    fn tick_until_idle(s: &mut Session<ScriptConnector>, max: usize) {
        for _ in 0..max {
            if !s.is_busy() {
                return;
            }
            s.tick();
        }
    }

    #[test]
    fn starts_disconnected() {
        let s = session(vec![]);
        assert_eq!(s.state(), State::Disconnected);
        assert!(!s.is_busy());
    }

    #[test]
    fn connect_identifies_and_enters_ready() {
        let mut s = session(vec![Ok(identify_steps())]);
        s.handle(Command::Connect);
        let events = s.take_events();
        assert_eq!(
            non_io(events.clone()),
            vec![
                Event::Connected { device: "test dongle".into(), protocol: Protocol::Plain },
                Event::State(State::Ready),
            ]
        );
        let io: Vec<Io> = events
            .into_iter()
            .filter_map(|e| if let Event::Transfer(t) = e { Some(t.event) } else { None })
            .collect();
        assert_eq!(io, vec![Io::Out(command::IDENTIFY.to_vec()), Io::In(PLAIN.to_vec())]);
    }

    #[test]
    fn connect_failure_stays_disconnected() {
        let mut s = session(vec![Err("no device".into())]);
        s.handle(Command::Connect);
        assert_eq!(s.take_events(), vec![Event::Error("no device".into())]);
        assert_eq!(s.state(), State::Disconnected);
    }

    #[test]
    fn unanswered_identify_enters_recovering() {
        let mut script = vec![Step::Timeout];
        for _ in 0..ops::IDENTIFY_ATTEMPTS {
            script.extend([w(command::IDENTIFY), Step::Timeout]);
        }
        let mut s = session(vec![Ok(script)]);
        s.handle(Command::Connect);
        let events = non_io(s.take_events());
        assert_eq!(events.last(), Some(&Event::State(State::Recovering)));
        assert!(matches!(events[0], Event::Error(_)));
        assert!(s.is_busy());
    }

    #[test]
    fn learn_once_completes_and_returns_to_ready() {
        let mut s = connected(vec![
            w(command::LEARN),
            Step::Timeout,
            Step::Reply(code(2, &[0x23, 0x23])),
            w(command::STOP),
        ]);
        s.handle(Command::LearnOnce { wait_s: 5 });
        assert_eq!(non_io(s.take_events()), vec![Event::State(State::Learning)]);
        tick_until_idle(&mut s, 10);
        assert_eq!(
            non_io(s.take_events()),
            vec![
                Event::Learned(LearnOutcome::Complete(LearnedCode { announced_len: 2, data: vec![0x23, 0x23] })),
                Event::State(State::Ready),
            ]
        );
    }

    #[test]
    fn learn_once_without_signal_enters_recovering() {
        let mut steps = vec![w(command::LEARN)];
        steps.extend((0..READS_PER_SECOND).map(|_| Step::Timeout));
        let mut s = connected(steps);
        s.handle(Command::LearnOnce { wait_s: 1 });
        for _ in 0..READS_PER_SECOND {
            s.tick();
        }
        assert_eq!(
            non_io(s.take_events()),
            vec![
                Event::State(State::Learning),
                Event::Learned(LearnOutcome::NothingReceived),
                Event::State(State::Recovering),
            ]
        );
    }

    #[test]
    fn capture_rearms_after_each_code() {
        let mut s = connected(vec![
            w(command::LEARN),
            Step::Reply(code(1, &[0x11])),
            w(command::STOP),
            w(command::LEARN),
            Step::Timeout,
            Step::Reply(code(1, &[0x22])),
            w(command::STOP),
            w(command::LEARN),
        ]);
        s.handle(Command::StartCapture);
        for _ in 0..3 {
            s.tick();
        }
        let learned: Vec<Event> =
            non_io(s.take_events()).into_iter().filter(|e| matches!(e, Event::Learned(_))).collect();
        assert_eq!(learned.len(), 2);
        assert_eq!(s.state(), State::Capturing);
    }

    #[test]
    fn capture_reports_every_transfer() {
        let mut s = connected(vec![w(command::LEARN), Step::Reply(code(1, &[0x11])), w(command::STOP), w(command::LEARN)]);
        s.handle(Command::StartCapture);
        s.tick();
        let io = s.take_events().into_iter().filter(|e| matches!(e, Event::Transfer(_))).count();
        assert_eq!(io, 4, "LEARN out, code in, STOP out, LEARN out");
    }

    #[test]
    fn stop_capture_while_armed_enters_recovering() {
        let mut s = connected(vec![w(command::LEARN), Step::Timeout]);
        s.handle(Command::StartCapture);
        s.tick();
        s.handle(Command::StopCapture);
        assert_eq!(s.state(), State::Recovering);
        assert_eq!(non_io(s.take_events()).last(), Some(&Event::State(State::Recovering)));
    }

    #[test]
    fn recovering_identifies_again_after_dongle_answers() {
        let mut steps = vec![w(command::LEARN), Step::Timeout];
        // Stopped while armed; then someone presses a remote button.
        steps.extend([Step::Timeout, Step::Reply(code(1, &[0x33])), Step::Timeout]);
        steps.extend(identify_steps());
        let mut s = connected(steps);
        s.handle(Command::StartCapture);
        s.tick();
        s.handle(Command::StopCapture);
        s.take_events();
        tick_until_idle(&mut s, 10);
        assert_eq!(s.state(), State::Ready);
        assert_eq!(
            non_io(s.take_events()),
            vec![
                Event::Connected { device: "test dongle".into(), protocol: Protocol::Plain },
                Event::State(State::Ready),
            ]
        );
    }

    #[test]
    fn transmit_sends_chunks_and_reports() {
        let pulses = vec![Pulse { on_us: 560, off_us: 0 }];
        let chunks = elk_proto::transmit::transmit_chunks(38000, &pulses).unwrap();
        let mut s = connected(chunks.into_iter().map(Step::ExpectWrite).collect());
        s.handle(Command::Transmit { pulses, frequency_hz: 38000 });
        assert_eq!(non_io(s.take_events()), vec![Event::Transmitted { pulses: 1 }]);
        assert_eq!(s.state(), State::Ready);
    }

    #[test]
    fn commands_in_wrong_state_report_error_without_change() {
        let mut s = session(vec![]);
        s.handle(Command::LearnOnce { wait_s: 1 });
        s.handle(Command::StartCapture);
        s.handle(Command::Transmit { pulses: vec![], frequency_hz: 38000 });
        let events = s.take_events();
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|e| matches!(e, Event::Error(_))));
        assert_eq!(s.state(), State::Disconnected);
    }

    #[test]
    fn unplug_during_capture_disconnects() {
        let mut s = connected(vec![w(command::LEARN), Step::Fail(TransportError::Disconnected)]);
        s.handle(Command::StartCapture);
        s.tick();
        assert_eq!(s.state(), State::Disconnected);
        let events = non_io(s.take_events());
        assert_eq!(events.last(), Some(&Event::State(State::Disconnected)));
        assert!(events.iter().any(|e| matches!(e, Event::Error(_))));
        assert!(!s.is_busy());
    }

    #[test]
    fn disconnect_command_drops_device() {
        let mut s = connected(vec![]);
        s.handle(Command::Disconnect);
        assert_eq!(non_io(s.take_events()), vec![Event::State(State::Disconnected)]);
        assert_eq!(s.state(), State::Disconnected);
    }
}
