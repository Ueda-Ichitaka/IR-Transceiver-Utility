//! The app state shared by the cxx-qt objects on the GUI thread.
//!
//! Exists because several QML objects (the device controller singleton and
//! the capture list models) show the same data: it lives once, in a
//! thread-local `AppState` on the GUI thread, holding the Qt-free `AppCore`,
//! the settings file path and the session worker. `with` must not be nested
//! and Qt signals are emitted only after it returns, so a QML handler can
//! call back into Rust safely.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ir_session::session::Event;
use ir_session::worker::SessionHandle;
use ir_settings::Settings;
use ir_view::app::AppCore;

/// State of the running app.
pub struct AppState {
    pub core: AppCore,
    pub session: Option<SessionHandle>,
    settings_path: Option<PathBuf>,
}

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

/// Runs `f` on the state, loading the settings on first use.
pub fn with<R>(f: impl FnOnce(&mut AppState) -> R) -> R {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        f(state.get_or_insert_with(AppState::load))
    })
}

/// Drops the state, which stops the session worker and releases the dongle.
pub fn shutdown() {
    let state = STATE.with(|cell| cell.borrow_mut().take());
    drop(state);
}

impl AppState {
    fn load() -> Self {
        let settings_path = ir_settings::default_path(|key| std::env::var(key).ok());
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let (settings, message) = match settings_path.as_deref().map(|p| ir_settings::load(p, now)) {
            Some(Ok(loaded)) => {
                let message = loaded.recovered.map(|(backup, reason)| {
                    format!("The settings file could not be read ({reason:?}); it was moved to {}.", backup.display())
                });
                (loaded.settings, message)
            }
            Some(Err(e)) => (Settings::default(), Some(format!("Reading the settings failed: {e}"))),
            None => (Settings::default(), None),
        };
        let mut core = AppCore::new(settings);
        if let Some(message) = message {
            core.apply(&[Event::Error(message)]);
        }
        Self { core, session: None, settings_path }
    }

    /// Writes the settings; a failure is shown as the last error.
    pub fn save_settings(&mut self) {
        let Some(path) = &self.settings_path else { return };
        if let Err(e) = ir_settings::save(path, &self.core.settings) {
            self.core.apply(&[Event::Error(format!("Saving the settings failed: {e}"))]);
        }
    }
}
