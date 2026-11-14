//! What to do when the set of plugged-in dongles changes.
//!
//! Exists to keep the hotplug reaction testable: connect when a dongle
//! appears while disconnected, disconnect when none is left while connected
//! (an idle connection would not notice otherwise).

use ir_session::session::{Command, State};

/// The command for `present` dongles in session `state`, if any.
pub fn command(present: usize, state: State) -> Option<Command> {
    match (present, state) {
        (1.., State::Disconnected) => Some(Command::Connect),
        (0, State::Disconnected) => None,
        (0, _) => Some(Command::Disconnect),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connects_when_a_dongle_appears() {
        assert_eq!(command(1, State::Disconnected), Some(Command::Connect));
        assert_eq!(command(2, State::Disconnected), Some(Command::Connect));
    }

    #[test]
    fn disconnects_when_none_is_left() {
        for state in [State::Ready, State::Learning, State::Capturing, State::Recovering] {
            assert_eq!(command(0, state), Some(Command::Disconnect), "{state:?}");
        }
    }

    #[test]
    fn otherwise_nothing() {
        assert_eq!(command(0, State::Disconnected), None);
        assert_eq!(command(1, State::Ready), None);
        assert_eq!(command(1, State::Capturing), None);
    }
}
