//! `SettingsController`: the QML singleton behind the Settings page.
//!
//! Exists to show the user settings in editable fields, validate and save
//! them through `ir_view::settings_form::SettingsForm`, apply a changed
//! capture limit to the running capture, and announce `saved` so the
//! capture lists re-render with the new display options.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, carrier_text)]
        #[qproperty(bool, first_frame_only)]
        #[qproperty(i32, repeat_count)]
        #[qproperty(QString, limit_mib_text)]
        #[qproperty(bool, uppercase)]
        #[qproperty(QString, bytes_per_line_text)]
        #[qproperty(bool, offer_pulses)]
        #[qproperty(QString, error)]
        type SettingsController = super::SettingsControllerRust;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qsignal]
        fn saved(self: Pin<&mut SettingsController>);

        /// Fills the fields from the current settings.
        #[qinvokable]
        fn load(self: Pin<&mut SettingsController>);
        /// Validates and saves the fields; false and `error` when invalid.
        #[qinvokable]
        fn save(self: Pin<&mut SettingsController>) -> bool;
    }
}

use std::pin::Pin;

use cxx_qt_lib::QString;
use ir_library::model::RepeatMode;
use ir_view::settings_form::SettingsForm;
use ir_view::text::capitalise;

use crate::state;

/// Rust side of `SettingsController`.
#[derive(Default)]
pub struct SettingsControllerRust {
    carrier_text: QString,
    first_frame_only: bool,
    repeat_count: i32,
    limit_mib_text: QString,
    uppercase: bool,
    bytes_per_line_text: QString,
    offer_pulses: bool,
    error: QString,
}

impl qobject::SettingsController {
    fn load(mut self: Pin<&mut Self>) {
        let form = state::with(|s| SettingsForm::from_settings(&s.core.settings));
        let (first_frame_only, count) = match form.repeat {
            RepeatMode::AsLearned => (false, 1),
            RepeatMode::FirstFrame { count } => (true, count),
        };
        self.as_mut().set_carrier_text(QString::from(&form.carrier_text));
        self.as_mut().set_first_frame_only(first_frame_only);
        self.as_mut().set_repeat_count(i32::try_from(count).unwrap_or(i32::MAX));
        self.as_mut().set_limit_mib_text(QString::from(&form.limit_mib_text));
        self.as_mut().set_uppercase(form.uppercase);
        self.as_mut().set_bytes_per_line_text(QString::from(&form.bytes_per_line_text));
        self.as_mut().set_offer_pulses(form.offer_pulses);
        self.set_error(QString::default());
    }

    fn save(mut self: Pin<&mut Self>) -> bool {
        let form = SettingsForm {
            carrier_text: self.carrier_text.to_string(),
            repeat: if self.first_frame_only {
                RepeatMode::FirstFrame { count: u32::try_from(self.repeat_count).unwrap_or(0) }
            } else {
                RepeatMode::AsLearned
            },
            limit_mib_text: self.limit_mib_text.to_string(),
            uppercase: self.uppercase,
            bytes_per_line_text: self.bytes_per_line_text.to_string(),
            offer_pulses: self.offer_pulses,
        };
        let result = state::with(|s| {
            form.apply(&mut s.core.settings)?;
            let limit = s.core.settings.capture_limit_bytes();
            s.core.capture.set_limit(limit);
            s.save_settings();
            Ok::<(), String>(())
        });
        match result {
            Ok(()) => {
                self.as_mut().set_error(QString::default());
                self.saved();
                true
            }
            Err(message) => {
                self.set_error(QString::from(&capitalise(&message)));
                false
            }
        }
    }
}
