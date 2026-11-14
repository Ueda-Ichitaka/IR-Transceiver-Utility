//! `CaptureModel`: a QML list model for one capture tab.
//!
//! Exists to show the capture log (`ir_view::capture::CaptureLog`) in a
//! ListView. `tab` selects the Signals tab (0) or the Transfers tab (1); each
//! tab has its own encoding switch, stored in the settings
//! (`views.capture` and `views.transfers`). Rows are only appended, so
//! `sync` (called on `DeviceController.captureChanged`) inserts the new rows,
//! or resets the model after the capture was cleared.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
    }

    extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(i32, tab, READ, WRITE = set_tab, NOTIFY = display_changed)]
        #[qproperty(i32, display_index, READ = display_index, WRITE = set_display_index, NOTIFY = display_changed)]
        #[qproperty(QStringList, display_options, READ = display_options, NOTIFY = display_changed)]
        type CaptureModel = super::CaptureModelRust;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        fn begin_insert_rows(self: Pin<&mut CaptureModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        fn end_insert_rows(self: Pin<&mut CaptureModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut CaptureModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut CaptureModel>);
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        fn display_index(self: &CaptureModel) -> i32;
        fn set_display_index(self: Pin<&mut CaptureModel>, index: i32);
        fn display_options(self: &CaptureModel) -> QStringList;
        fn set_tab(self: Pin<&mut CaptureModel>, tab: i32);

        #[qinvokable]
        #[cxx_override]
        fn data(self: &CaptureModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        fn row_count(self: &CaptureModel, parent: &QModelIndex) -> i32;
        #[qinvokable]
        #[cxx_override]
        fn role_names(self: &CaptureModel) -> QHash_i32_QByteArray;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qsignal]
        fn display_changed(self: Pin<&mut CaptureModel>);

        /// Re-renders every row (after the display settings changed).
        #[qinvokable]
        fn reformat(self: Pin<&mut CaptureModel>);

        /// Brings the rows up to date with the capture log.
        #[qinvokable]
        fn sync(self: Pin<&mut CaptureModel>);
    }
}

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant};
use ir_settings::ViewSettings;
use ir_view::capture::{Direction, format_time, signal_data_text, transfer_data_text, Shown};
use ir_view::display;

use crate::state;

pub const TAB_SIGNALS: i32 = 0;
pub const TAB_TRANSFERS: i32 = 1;

/// Roles, starting at Qt::UserRole (0x100).
const ROLE_TIME: i32 = 0x100;
const ROLE_KIND: i32 = 0x101;
const ROLE_SUMMARY: i32 = 0x102;
const ROLE_DATA: i32 = 0x103;
const ROLE_LENGTH: i32 = 0x104;

/// Rust side of `CaptureModel`.
#[derive(Default)]
pub struct CaptureModelRust {
    tab: i32,
    /// Rows the view currently knows about.
    rows: usize,
}

fn view_of(settings: &mut ir_settings::Settings, tab: i32) -> &mut ViewSettings {
    if tab == TAB_TRANSFERS { &mut settings.views.transfers } else { &mut settings.views.capture }
}

fn options_for(view: &ViewSettings, tab: i32) -> Vec<Shown> {
    let mut options = display::options(view);
    if tab == TAB_TRANSFERS {
        options.retain(|o| *o != Shown::Pulses);
    }
    options
}

impl qobject::CaptureModel {
    fn set_tab(mut self: Pin<&mut Self>, tab: i32) {
        if self.tab == tab {
            return;
        }
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().tab = tab;
        let rows = Self::log_rows(tab);
        self.as_mut().rust_mut().rows = rows;
        self.as_mut().end_reset_model();
        self.display_changed();
    }

    fn log_rows(tab: i32) -> usize {
        state::with(|s| {
            if tab == TAB_TRANSFERS { s.core.capture.transfer_count() } else { s.core.capture.signal_count() }
        })
    }

    fn display_options(&self) -> QStringList {
        let tab = self.tab;
        let labels: Vec<QString> = state::with(|s| {
            let view = *view_of(&mut s.core.settings, tab);
            options_for(&view, tab).into_iter().map(|o| QString::from(display::label(o))).collect()
        });
        labels.iter().collect()
    }

    fn display_index(&self) -> i32 {
        let tab = self.tab;
        state::with(|s| {
            let view = *view_of(&mut s.core.settings, tab);
            let current = display::current(&view);
            options_for(&view, tab).iter().position(|o| *o == current).map_or(0, |i| i as i32)
        })
    }

    fn set_display_index(mut self: Pin<&mut Self>, index: i32) {
        let tab = self.tab;
        let changed = state::with(|s| {
            let view = view_of(&mut s.core.settings, tab);
            let Some(choice) = usize::try_from(index).ok().and_then(|i| options_for(view, tab).get(i).copied()) else {
                return false;
            };
            if display::current(view) == choice || !display::select(view, choice) {
                return false;
            }
            s.save_settings();
            true
        });
        if changed {
            self.as_mut().begin_reset_model();
            self.as_mut().end_reset_model();
            self.display_changed();
        }
    }

    fn reformat(mut self: Pin<&mut Self>) {
        self.as_mut().begin_reset_model();
        self.as_mut().end_reset_model();
        self.display_changed();
    }

    fn sync(mut self: Pin<&mut Self>) {
        let now = Self::log_rows(self.tab);
        let known = self.rows;
        if now < known {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows = now;
            self.as_mut().end_reset_model();
        } else if now > known {
            let first = i32::try_from(known).unwrap_or(i32::MAX);
            let last = i32::try_from(now - 1).unwrap_or(i32::MAX);
            self.as_mut().begin_insert_rows(&QModelIndex::default(), first, last);
            self.as_mut().rust_mut().rows = now;
            self.as_mut().end_insert_rows();
        }
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        i32::try_from(self.rows).unwrap_or(i32::MAX)
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROLE_TIME, "time"),
            (ROLE_KIND, "kind"),
            (ROLE_SUMMARY, "summary"),
            (ROLE_DATA, "bytes"),
            (ROLE_LENGTH, "length"),
        ] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Ok(row) = usize::try_from(index.row()) else { return QVariant::default() };
        if row >= self.rows {
            return QVariant::default();
        }
        let tab = self.tab;
        state::with(|s| {
            let view = *view_of(&mut s.core.settings, tab);
            let options = view.format_options();
            let text = if tab == TAB_TRANSFERS {
                let r = s.core.capture.transfer(row)?;
                let encoding = match display::current(&view) {
                    Shown::Encoded(e) => e,
                    Shown::Pulses => view.encoding,
                };
                match role {
                    ROLE_TIME => format_time(r.at),
                    ROLE_KIND => match r.direction {
                        Direction::Out => "OUT".into(),
                        Direction::In => "IN".into(),
                        Direction::Error => "ERR".into(),
                    },
                    ROLE_SUMMARY => String::new(),
                    ROLE_DATA => transfer_data_text(r, encoding, &options),
                    ROLE_LENGTH => r.bytes.len().to_string(),
                    _ => return None,
                }
            } else {
                let r = s.core.capture.signal(row)?;
                match role {
                    ROLE_TIME => format_time(r.at),
                    ROLE_KIND => if r.complete { "complete".into() } else { "short".into() },
                    ROLE_SUMMARY => r.summary.clone(),
                    ROLE_DATA => signal_data_text(r, display::current(&view), &options),
                    ROLE_LENGTH => r.data.len().to_string(),
                    _ => return None,
                }
            };
            Some(text)
        })
        .map_or_else(QVariant::default, |text| QVariant::from(&QString::from(&text)))
    }
}
