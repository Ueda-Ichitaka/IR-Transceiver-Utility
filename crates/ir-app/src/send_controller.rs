//! `SendController`: the Send page form.
//!
//! Exists to hold what the user typed on the Send page and hand it to
//! `ir_view::send::SendForm`, which builds the session command with the
//! global defaults from the settings. The encoding of the bytes field is the
//! Send view's encoding switch and is stored in the settings; typed bytes are
//! converted when it changes. `valid`, `hint` and `wirePreview` follow every
//! change (all fields notify `formChanged`); codes can be loaded from and saved to the library. Parse errors
//! are shown in `error`; transmit results arrive through `DeviceController`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, kind, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(QString, bytes_text, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(i32, encoding_index, READ = encoding_index, WRITE = set_encoding_index, NOTIFY = form_changed)]
        #[qproperty(QString, pulses_text, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(QString, nec_address, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(QString, nec_command, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(bool, nec_extended, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(QString, carrier_text, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(i32, repeat_index, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(i32, repeat_count, READ, WRITE, NOTIFY = form_changed)]
        #[qproperty(QString, error)]
        #[qproperty(QStringList, kind_options, READ = kind_options, CONSTANT)]
        #[qproperty(QStringList, encoding_options, READ = encoding_options, CONSTANT)]
        #[qproperty(QStringList, repeat_options, READ = repeat_options, CONSTANT)]
        #[qproperty(QString, default_carrier, READ = default_carrier, NOTIFY = form_changed)]
        #[qproperty(bool, valid, READ = valid, NOTIFY = form_changed)]
        #[qproperty(QString, hint, READ = hint, NOTIFY = form_changed)]
        #[qproperty(QString, wire_preview, READ = wire_preview, NOTIFY = form_changed)]
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
        fn valid(self: &SendController) -> bool;
        fn hint(self: &SendController) -> QString;
        fn wire_preview(self: &SendController) -> QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// Any field, the encoding or the defaults changed.
        #[qsignal]
        fn form_changed(self: Pin<&mut SendController>);

        /// Re-reads the defaults shown as placeholders (after the settings changed).
        #[qinvokable]
        fn refresh_defaults(self: Pin<&mut SendController>);

        /// Sends the code described by the form; returns false and sets
        /// `error` when the form is invalid or no dongle is connected.
        #[qinvokable]
        fn send(self: Pin<&mut SendController>) -> bool;
        /// Names and ids of the library codes for the Load dialog.
        #[qinvokable]
        fn library_names(self: &SendController) -> QStringList;
        #[qinvokable]
        fn library_ids(self: &SendController) -> QList_i32;
        /// Fills the form from library code `id`.
        #[qinvokable]
        fn load_code(self: Pin<&mut SendController>, id: i32) -> bool;
        /// Saves the form as a library code named `name`; returns its id or
        /// -1 and sets `error`.
        #[qinvokable]
        fn save_to_library(self: Pin<&mut SendController>, name: &QString) -> i32;
    }
}

use std::pin::Pin;

use cxx_qt_lib::{QList, QString, QStringList};
use ir_codec::{Encoding, FormatOptions};
use ir_library::model::RepeatMode;
use ir_view::capture::Shown;
use ir_view::display;
use ir_view::send::{PayloadKind, SendForm, reencode, wire};

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

    fn refresh_defaults(self: Pin<&mut Self>) {
        self.form_changed();
    }

    fn encoding_index(&self) -> i32 {
        state::with(|s| {
            let encoding = s.core.settings.views.send.encoding;
            Encoding::ALL.iter().position(|e| *e == encoding).map_or(0, |i| i as i32)
        })
    }

    fn set_encoding_index(mut self: Pin<&mut Self>, index: i32) {
        let Some(&encoding) = usize::try_from(index).ok().and_then(|i| Encoding::ALL.get(i)) else { return };
        let previous = state::with(|s| {
            let view = &mut s.core.settings.views.send;
            if view.encoding == encoding {
                return None;
            }
            let previous = view.encoding;
            view.encoding = encoding;
            s.save_settings();
            Some(previous)
        });
        let Some(previous) = previous else { return };
        // The typed bytes follow the switch when they parse.
        if let Some(text) = reencode(&self.bytes_text.to_string(), previous, encoding) {
            self.as_mut().set_bytes_text(QString::from(&text));
        }
        self.form_changed();
    }

    /// Whether the form is valid, its hint or error, and the wire preview.
    fn checked(&self) -> (bool, String, String) {
        let form = self.form();
        state::with(|s| {
            let defaults = s.core.settings.send_defaults();
            match form.hint(&defaults) {
                Ok(hint) => {
                    let options = FormatOptions { uppercase: true, bytes_per_line: 0 };
                    let chunks = form.command(&defaults).and_then(|c| wire(&c)).unwrap_or_default();
                    let lines: Vec<String> =
                        chunks.iter().map(|c| ir_codec::format(c, Encoding::Hex, &options)).collect();
                    (true, capitalise(&hint), lines.join("\n"))
                }
                Err(e) => (false, capitalise(&e), String::new()),
            }
        })
    }

    fn valid(&self) -> bool {
        self.checked().0
    }

    fn hint(&self) -> QString {
        QString::from(&self.checked().1)
    }

    fn wire_preview(&self) -> QString {
        QString::from(&self.checked().2)
    }

    fn library_names(&self) -> QStringList {
        let names: Vec<QString> = state::with(|s| s.library.codes.iter().map(|c| QString::from(&c.name)).collect());
        names.iter().collect()
    }

    fn library_ids(&self) -> QList<i32> {
        state::with(|s| s.library.codes.iter().map(|c| i32::try_from(c.id).unwrap_or(-1)).collect())
    }

    fn load_code(mut self: Pin<&mut Self>, id: i32) -> bool {
        let encoding = Encoding::ALL[usize::try_from(self.encoding_index()).unwrap_or(0)];
        let Some(form) = state::with(|s| {
            u64::try_from(id).ok().and_then(|id| s.library.get(id)).map(|code| SendForm::from_code(code, encoding))
        }) else {
            return false;
        };
        let kind = PayloadKind::ALL.iter().position(|k| *k == form.kind).unwrap_or(0) as i32;
        let (repeat_index, repeat_count) = match form.repeat {
            None => (REPEAT_DEFAULT, 1),
            Some(RepeatMode::AsLearned) => (REPEAT_AS_LEARNED, 1),
            Some(RepeatMode::FirstFrame { count }) => (2, i32::try_from(count).unwrap_or(1)),
        };
        self.as_mut().set_kind(kind);
        self.as_mut().set_bytes_text(QString::from(&form.bytes_text));
        self.as_mut().set_pulses_text(QString::from(&form.pulses_text));
        self.as_mut().set_nec_address(QString::from(&form.nec_address));
        self.as_mut().set_nec_command(QString::from(&form.nec_command));
        self.as_mut().set_nec_extended(form.nec_extended);
        self.as_mut().set_carrier_text(QString::from(&form.carrier_text));
        self.as_mut().set_repeat_index(repeat_index);
        self.as_mut().set_repeat_count(repeat_count);
        self.set_error(QString::default());
        true
    }

    fn save_to_library(self: Pin<&mut Self>, name: &QString) -> i32 {
        let form = self.form();
        let name = name.to_string().trim().to_string();
        let result = (|| {
            if name.is_empty() {
                return Err("the name is empty".to_string());
            }
            let (payload, carrier_hz, repeat) = form.code_parts()?;
            state::with(|s| {
                let id = s.library.add(name, payload);
                if let Some(code) = s.library.get_mut(id) {
                    code.carrier_hz = carrier_hz;
                    code.repeat = repeat;
                }
                s.save_library().map(|()| id)
            })
        })();
        match result {
            Ok(id) => {
                self.set_error(QString::default());
                i32::try_from(id).unwrap_or(-1)
            }
            Err(e) => {
                self.set_error(QString::from(&capitalise(&e)));
                -1
            }
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
            s.send_command(command)
        });
        let message = result.as_ref().err().map_or_else(String::new, |e| capitalise(e));
        self.as_mut().set_error(QString::from(&message));
        result.is_ok()
    }
}
