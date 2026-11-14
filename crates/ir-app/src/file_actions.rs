//! `FileActions`: the QML singleton for import, export and Pronto copies.
//!
//! Exists to do the file work of design decisions 17 and 18: write the
//! capture or the library in the chosen format, import codes into the
//! library, and give Pronto hex for one code or signal. The contents come
//! from `ir_view::files` and `ir_view::exchange`; the outcome is shown
//! through `message` and `ok`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, message)]
        #[qproperty(bool, ok)]
        type FileActions = super::FileActionsRust;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Writes the capture as `format` (json, signals-csv, transfers-csv,
        /// bin, lirc, flipper).
        #[qinvokable]
        fn export_capture(self: Pin<&mut FileActions>, url: &QUrl, format: &QString) -> bool;
        /// Writes the codes matching `query` and `tag` as `format` (lirc, flipper).
        #[qinvokable]
        fn export_library(self: Pin<&mut FileActions>, url: &QUrl, format: &QString, query: &QString, tag: &QString)
        -> bool;
        /// Adds the codes of a Flipper, LIRC or Pronto file to the library.
        #[qinvokable]
        fn import_library(self: Pin<&mut FileActions>, url: &QUrl) -> bool;
        /// Pronto hex of a library code; empty with a message on failure.
        #[qinvokable]
        fn pronto_for_code(self: Pin<&mut FileActions>, id: i32) -> QString;
        /// Pronto hex of captured signal `row`; empty with a message on failure.
        #[qinvokable]
        fn pronto_for_signal(self: Pin<&mut FileActions>, row: i32) -> QString;
    }
}

use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt_lib::{QString, QUrl};
use ir_view::exchange::{self, add_all, code_item, signal_item};
use ir_view::files::{self, CaptureFormat, LibraryFormat};
use ir_view::text::capitalise;

use crate::state;

/// Rust side of `FileActions`.
#[derive(Default)]
pub struct FileActionsRust {
    message: QString,
    ok: bool,
}

fn local_path(url: &QUrl) -> Result<PathBuf, String> {
    url.to_local_file().map(|p| PathBuf::from(p.to_string())).ok_or_else(|| "only local files are supported".into())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("writing {} failed: {e}", path.display()))
}

impl qobject::FileActions {
    fn report(mut self: Pin<&mut Self>, result: Result<String, String>) -> bool {
        let ok = result.is_ok();
        let message = result.unwrap_or_else(|e| capitalise(&e));
        self.as_mut().set_ok(ok);
        self.set_message(QString::from(&message));
        ok
    }

    fn export_capture(self: Pin<&mut Self>, url: &QUrl, format: &QString) -> bool {
        let result = (|| {
            let path = local_path(url)?;
            let format = CaptureFormat::from_name(&format.to_string()).ok_or("unknown export format")?;
            let output = state::with(|s| files::export_capture(&s.core.capture, format, s.core.settings.send.carrier_hz))?;
            write(&path, &output.bytes)?;
            Ok(files::export_message(&path, &output.skipped))
        })();
        self.report(result)
    }

    fn export_library(self: Pin<&mut Self>, url: &QUrl, format: &QString, query: &QString, tag: &QString) -> bool {
        let result = (|| {
            let path = local_path(url)?;
            let format = LibraryFormat::from_name(&format.to_string()).ok_or("unknown export format")?;
            let (query, tag) = (query.to_string(), tag.to_string());
            let output = state::with(|s| {
                let codes = s.library.search(&query, (!tag.is_empty()).then_some(tag.as_str()));
                files::export_library(codes, format, s.core.settings.send.carrier_hz)
            })?;
            write(&path, &output.bytes)?;
            Ok(files::export_message(&path, &output.skipped))
        })();
        self.report(result)
    }

    fn import_library(self: Pin<&mut Self>, url: &QUrl) -> bool {
        let result = (|| {
            let path = local_path(url)?;
            let text = std::fs::read_to_string(&path).map_err(|e| format!("reading {} failed: {e}", path.display()))?;
            let codes = files::import(&text, &path)?;
            let count = state::with(|s| {
                let count = add_all(&mut s.library, codes);
                s.save_library().map(|()| count)
            })?;
            Ok(files::import_message(count, &path))
        })();
        self.report(result)
    }

    fn pronto_for_code(self: Pin<&mut Self>, id: i32) -> QString {
        let result = state::with(|s| {
            let code = u64::try_from(id).ok().and_then(|id| s.library.get(id)).ok_or("the code no longer exists")?;
            let item = code_item(code, s.core.settings.send.carrier_hz).ok_or("this code has no pulse timings")?;
            exchange::to_pronto(&item)
        });
        self.pronto(result)
    }

    fn pronto_for_signal(self: Pin<&mut Self>, row: i32) -> QString {
        let result = state::with(|s| {
            let signal = usize::try_from(row).ok().and_then(|r| s.core.capture.signal(r)).ok_or("no such signal")?;
            let item = signal_item(signal, s.core.settings.send.carrier_hz).ok_or("this signal cannot be decoded")?;
            exchange::to_pronto(&item)
        });
        self.pronto(result)
    }

    fn pronto(self: Pin<&mut Self>, result: Result<String, String>) -> QString {
        let text = result.as_ref().ok().cloned().unwrap_or_default();
        self.report(result.map(|_| "Copied as Pronto hex.".to_string()));
        QString::from(&text)
    }
}
