//! The app state shared by the cxx-qt objects on the GUI thread.
//!
//! Exists because several QML objects (the device controller singleton and
//! the capture list models) show the same data: it lives once, in a
//! thread-local `AppState` on the GUI thread, holding the Qt-free `AppCore`,
//! the code library, the settings and library file paths and the session
//! worker. `with` must not be nested
//! and Qt signals are emitted only after it returns, so a QML handler can
//! call back into Rust safely.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ir_library::model::Library;
use ir_library::store;
use ir_session::session::Event;
use ir_session::worker::SessionHandle;
use ir_settings::Settings;
use ir_view::app::AppCore;
use ir_view::text::capitalise;

/// State of the running app.
pub struct AppState {
    pub core: AppCore,
    pub library: Library,
    pub session: Option<SessionHandle>,
    settings_path: Option<PathBuf>,
    library_path: Option<PathBuf>,
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
                    capitalise(&format!("{reason}; the file was moved to {} and the default settings are used.", backup.display()))
                });
                (loaded.settings, message)
            }
            Some(Err(e)) => (Settings::default(), Some(format!("Reading the settings failed: {e}"))),
            None => (Settings::default(), None),
        };
        let library_path = store::default_path(|key| std::env::var(key).ok());
        let (library, library_message) = match library_path.as_deref().map(|p| store::load(p, now)) {
            Some(Ok(loaded)) => {
                let message = loaded.recovered.map(|(backup, reason)| {
                    capitalise(&format!("{reason}; the file was moved to {} and the library starts empty.", backup.display()))
                });
                (loaded.library, message)
            }
            Some(Err(e)) => (Library::default(), Some(format!("Reading the library failed: {e}"))),
            None => (Library::default(), None),
        };
        let mut core = AppCore::new(settings);
        for message in [message, library_message].into_iter().flatten() {
            core.apply(&[Event::Error(message)]);
        }
        Self { core, library, session: None, settings_path, library_path }
    }

    /// Writes the library; a failure is returned as a message.
    pub fn save_library(&mut self) -> Result<(), String> {
        let Some(path) = &self.library_path else { return Err("No place to store the library (HOME is not set).".into()) };
        store::save(path, &self.library).map_err(|e| format!("Saving the library failed: {e}"))
    }

    /// Writes the settings; a failure is shown as the last error.
    pub fn save_settings(&mut self) {
        let Some(path) = &self.settings_path else { return };
        if let Err(e) = ir_settings::save(path, &self.core.settings) {
            self.core.apply(&[Event::Error(format!("Saving the settings failed: {e}"))]);
        }
    }
}
