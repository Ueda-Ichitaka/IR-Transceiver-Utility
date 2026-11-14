//! Runs a [`Session`] on its own thread.
//!
//! Exists so the GUI thread never blocks on USB: commands go in through a
//! channel, batches of events come out through a callback (which the Qt
//! bridge forwards to the GUI thread). While the session is idle the thread
//! sleeps on the channel; while it is busy it alternates between pending
//! commands and ticks. Dropping the handle stops and joins the thread.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;

use crate::session::{Command, Connector, Event, Session};

/// Handle to a running session thread.
pub struct SessionHandle {
    tx: Option<Sender<Command>>,
    thread: Option<JoinHandle<()>>,
}

impl SessionHandle {
    /// Queues a command. Returns false if the thread has stopped.
    pub fn send(&self, command: Command) -> bool {
        self.tx.as_ref().is_some_and(|tx| tx.send(command).is_ok())
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        // Closing the channel ends the loop; the thread finishes its current
        // read (at most 100 ms) first.
        self.tx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts the session thread. `sink` receives each non-empty batch of events.
pub fn spawn<C>(session: Session<C>, sink: impl FnMut(Vec<Event>) + Send + 'static) -> SessionHandle
where
    C: Connector + Send + 'static,
    C::Transport: Send,
{
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("ir-session".into())
        .spawn(move || run(session, rx, sink))
        .expect("spawning the session thread");
    SessionHandle { tx: Some(tx), thread: Some(thread) }
}

fn run<C: Connector>(mut session: Session<C>, rx: Receiver<Command>, mut sink: impl FnMut(Vec<Event>)) {
    loop {
        let command = if session.is_busy() {
            match rx.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            }
        };
        match command {
            Some(command) => session.handle(command),
            None => session.tick(),
        }
        let events = session.take_events();
        if !events.is_empty() {
            sink(events);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{DeviceInfo, State};
    use elk_proto::command;
    use ir_transport::mock::{MockTransport, Step};
    use std::time::Duration;

    struct OneShot(Option<Vec<Step>>);

    impl Connector for OneShot {
        type Transport = MockTransport;
        fn connect(&mut self) -> Result<(MockTransport, DeviceInfo), String> {
            Ok((MockTransport::new(self.0.take().ok_or("no script")?), DeviceInfo::named("test")))
        }
    }

    fn states(rx: &mpsc::Receiver<Vec<Event>>, until: State) -> Vec<State> {
        let mut seen = Vec::new();
        while let Ok(batch) = rx.recv_timeout(Duration::from_secs(5)) {
            for e in batch {
                if let Event::State(s) = e {
                    seen.push(s);
                    if s == until {
                        return seen;
                    }
                }
            }
        }
        panic!("did not reach {until:?}, saw {seen:?}");
    }

    #[test]
    fn connects_on_the_worker_thread() {
        let script = vec![
            Step::Timeout,
            Step::ExpectWrite(command::IDENTIFY.to_vec()),
            Step::Reply(vec![0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01]),
        ];
        let session = Session::new(OneShot(Some(script)), Box::new(|_| {}));
        let (tx, rx) = mpsc::channel();
        let handle = spawn(session, move |batch| {
            let _ = tx.send(batch);
        });
        assert!(handle.send(Command::Connect));
        assert_eq!(states(&rx, State::Ready), vec![State::Ready]);
    }

    #[test]
    fn ticks_while_busy_and_accepts_commands() {
        let mut script = vec![
            Step::Timeout,
            Step::ExpectWrite(command::IDENTIFY.to_vec()),
            Step::Reply(vec![0xFC, 0xFC, 0xFC, 0xFC, 0x70, 0x01]),
            Step::ExpectWrite(command::LEARN.to_vec()),
        ];
        // The mock times out immediately once the script is used up.
        script.push(Step::Timeout);
        let session = Session::new(OneShot(Some(script)), Box::new(|_| {}));
        let (tx, rx) = mpsc::channel();
        let handle = spawn(session, move |batch| {
            let _ = tx.send(batch);
        });
        handle.send(Command::Connect);
        handle.send(Command::StartCapture);
        states(&rx, State::Capturing);
        handle.send(Command::StopCapture);
        assert_eq!(states(&rx, State::Recovering), vec![State::Recovering]);
    }

    #[test]
    fn dropping_the_handle_stops_the_thread() {
        let session = Session::new(OneShot(None), Box::new(|_| {}));
        let handle = spawn(session, |_| {});
        drop(handle);
    }
}
