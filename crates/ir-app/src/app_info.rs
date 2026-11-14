//! `AppInfo`: exposes application facts (version) to QML.
//!
//! Exists as the first cxx-qt bridge object of the app; QML shows the
//! version in the sidebar footer.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, version)]
        type AppInfo = super::AppInfoRust;
    }
}

use cxx_qt_lib::QString;

/// Rust side of `AppInfo`.
pub struct AppInfoRust {
    version: QString,
}

impl Default for AppInfoRust {
    fn default() -> Self {
        Self { version: QString::from(env!("CARGO_PKG_VERSION")) }
    }
}
