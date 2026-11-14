//! `SendController`: the Send page form.
//!
//! Exists to hold what the user typed on the Send page and hand it to
//! `ir_view::send::SendForm`, which builds the session command with the
//! global defaults from the settings. The encoding of the bytes field is the
//! Send view's encoding switch and is stored in the settings. Parse errors
//! are shown in `error`; transmit results arrive through `DeviceController`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, kind)]
        #[qproperty(QString, bytes_text)]
        #[qproperty(i32, encoding_index, READ = encoding_index, WRITE = set_encoding_index, NOTIFY = encoding_changed)]
        #[qproperty(QString, pulses_text)]
        #[qproperty(QString, nec_address)]
        #[qproperty(QString, nec_command)]
        #[qproperty(bool, nec_extended)]
        #[qproperty(QString, carrier_text)]
        #[qproperty(i32, repeat_index)]
        #[qproperty(i32, repeat_count)]
        #[qproperty(QString, error)]
        #[qproperty(QStringList, kind_options, READ = kind_options, CONSTANT)]
        #[qproperty(QStringList, encoding_options, READ = encoding_options, CONSTANT)]
        #[qproperty(QStringList, repeat_options, READ = repeat_options, CONSTANT)]
        #[qproperty(QString, default_carrier, READ = default_carrier, CONSTANT)]
        type SendController = super::SendControllerRust;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        fn encoding_index(self: &SendController) -> i32;
        fn set_encoding_index(self: Pin<&mut SendController>, index: i32);
        fn kind_options(self: &SendController) -> QStringList;
        fn encoding_options(self: &SendController) -> QStringList;
        fn repeat_options(self: &SendController) -> QStringList;
        fn default_carrier(self: &SendController) -> QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qsignal]
        fn encoding_changed(self: Pin<&mut SendController>);

        /// Sends the code described by the form; returns false and sets
        /// `error` when the form is invalid or no dongle is connected.
        #[qinvokable]
        fn send(self: Pin<&mut SendController>) -> bool;
    }
}

use std::pin::Pin;

use cxx_qt_lib::{QString, QStringList};
use ir_codec::Encoding;
use ir_library::model::RepeatMode;
use ir_view::capture::Shown;
use ir_view::display;
use ir_view::send::{PayloadKind, SendForm};

use ir_view::text::capitalise;

use crate::state;

/// Repeat choices, in the order of `repeat_options`.
const REPEAT_DEFAULT: i32 = 0;
const REPEAT_AS_LEARNED: i32 = 1;

/// Rust side of `SendController`.
pub struct SendControllerRust {
    kind: i32,
    bytes_text: QString,
    pulses_text: QString,
    nec_address: QString,
    nec_command: QString,
    nec_extended: bool,
    carrier_text: QString,
    repeat_index: i32,
    repeat_count: i32,
    error: QString,
}

impl Default for SendControllerRust {
    fn default() -> Self {
        Self {
            kind: 0,
            bytes_text: QString::default(),
            pulses_text: QString::default(),
            nec_address: QString::default(),
            nec_command: QString::default(),
            nec_extended: false,
            carrier_text: QString::default(),
            repeat_index: REPEAT_DEFAULT,
            repeat_count: 1,
            error: QString::default(),
        }
    }
}

fn list(items: impl IntoIterator<Item = &'static str>) -> QStringList {
    let strings: Vec<QString> = items.into_iter().map(QString::from).collect();
    strings.iter().collect()
}

impl qobject::SendController {
    fn kind_options(&self) -> QStringList {
        list(PayloadKind::ALL.into_iter().map(PayloadKind::label))
    }

    fn encoding_options(&self) -> QStringList {
        list(Encoding::ALL.into_iter().map(|e| display::label(Shown::Encoded(e))))
    }

    fn repeat_options(&self) -> QStringList {
        list(["Default", "As learned", "First frame only"])
    }

    fn default_carrier(&self) -> QString {
        state::with(|s| QString::from(&s.core.settings.send.carrier_hz.to_string()))
    }

    fn encoding_index(&self) -> i32 {
        state::with(|s| {
            let encoding = s.core.settings.views.send.encoding;
            Encoding::ALL.iter().position(|e| *e == encoding).map_or(0, |i| i as i32)
        })
    }

    fn set_encoding_index(self: Pin<&mut Self>, index: i32) {
        let Some(&encoding) = usize::try_from(index).ok().and_then(|i| Encoding::ALL.get(i)) else { return };
        let changed = state::with(|s| {
            let view = &mut s.core.settings.views.send;
            if view.encoding == encoding {
                return false;
            }
            view.encoding = encoding;
            s.save_settings();
            true
        });
        if changed {
            self.encoding_changed();
        }
    }

    fn form(&self) -> SendForm {
        let kind = usize::try_from(self.kind).ok().and_then(|i| PayloadKind::ALL.get(i)).copied();
        let encoding = Encoding::ALL[usize::try_from(self.encoding_index()).unwrap_or(0)];
        let repeat = match self.repeat_index {
            REPEAT_DEFAULT => None,
            REPEAT_AS_LEARNED => Some(RepeatMode::AsLearned),
            _ => Some(RepeatMode::FirstFrame { count: u32::try_from(self.repeat_count).unwrap_or(1).max(1) }),
        };
        SendForm {
            kind: kind.unwrap_or(PayloadKind::Learned),
            bytes_text: self.bytes_text.to_string(),
            encoding,
            pulses_text: self.pulses_text.to_string(),
            nec_address: self.nec_address.to_string(),
            nec_command: self.nec_command.to_string(),
            nec_extended: self.nec_extended,
            carrier_text: self.carrier_text.to_string(),
            repeat,
        }
    }

    fn send(mut self: Pin<&mut Self>) -> bool {
        let form = self.form();
        let result = state::with(|s| {
            let command = form.command(&s.core.settings.send_defaults())?;
            match &s.session {
                Some(session) if session.send(command) => Ok(()),
                _ => Err("The dongle is not connected.".to_string()),
            }
        });
        let message = result.as_ref().err().map_or_else(String::new, |e| capitalise(e));
        self.as_mut().set_error(QString::from(&message));
        result.is_ok()
    }
}
