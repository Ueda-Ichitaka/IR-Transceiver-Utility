//! `CaptureModel`: a QML list model for one capture tab, plus the detail
//! panel of its selected row.
//!
//! Exists to show the capture log (`ir_view::capture::CaptureLog`) as in the
//! mockup: compact rows (number, time, kind chip, one-line byte preview) and,
//! for the selected row, the detail panel from `ir_view::capture_view`. `tab`
//! selects the Signals tab (0) or the Transfers tab (1); each tab has its own
//! encoding switch, stored in the settings (`views.capture`,
//! `views.transfers`). Rows are only appended, so `sync` (called on
//! `DeviceController.captureChanged`) inserts the new rows, or resets the
//! model after the capture was cleared; `reformat` re-reads everything.

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
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
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
        #[qproperty(bool, time_of_day, READ = time_of_day, NOTIFY = display_changed)]
        #[qproperty(i32, selected, READ, NOTIFY = selection_changed)]
        #[qproperty(QString, detail_title, READ = detail_title, NOTIFY = selection_changed)]
        #[qproperty(QString, detail_bytes, READ = detail_bytes, NOTIFY = selection_changed)]
        #[qproperty(QStringList, detail_fact_labels, READ = detail_fact_labels, NOTIFY = selection_changed)]
        #[qproperty(QStringList, detail_fact_values, READ = detail_fact_values, NOTIFY = selection_changed)]
        #[qproperty(QList_i32, detail_waveform, READ = detail_waveform, NOTIFY = selection_changed)]
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
        fn time_of_day(self: &CaptureModel) -> bool;
        fn set_tab(self: Pin<&mut CaptureModel>, tab: i32);
        fn detail_title(self: &CaptureModel) -> QString;
        fn detail_bytes(self: &CaptureModel) -> QString;
        fn detail_fact_labels(self: &CaptureModel) -> QStringList;
        fn detail_fact_values(self: &CaptureModel) -> QStringList;
        fn detail_waveform(self: &CaptureModel) -> QList_i32;

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
        #[qsignal]
        fn selection_changed(self: Pin<&mut CaptureModel>);

        /// Selects `row` for the detail panel (-1 for none).
        #[qinvokable]
        fn select(self: Pin<&mut CaptureModel>, row: i32);
        /// Re-reads and re-renders every row (after the display settings
        /// changed or the capture was replaced).
        #[qinvokable]
        fn reformat(self: Pin<&mut CaptureModel>);
        /// Brings the rows up to date with the capture log.
        #[qinvokable]
        fn sync(self: Pin<&mut CaptureModel>);
    }
}

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QStringList, QVariant};
use ir_settings::ViewSettings;
use ir_view::capture::{Shown, format_time, signal_data_text, transfer_data_text};
use ir_view::capture_view::{self, Detail, Tone};
use ir_view::display;

use crate::state;

pub const TAB_SIGNALS: i32 = 0;
pub const TAB_TRANSFERS: i32 = 1;

/// Characters of the one-line byte preview.
const PREVIEW_CHARS: usize = 48;

/// Roles, starting at Qt::UserRole (0x100).
const ROLE_NUMBER: i32 = 0x100;
const ROLE_TIME: i32 = 0x101;
const ROLE_WALL_MS: i32 = 0x102;
const ROLE_KIND: i32 = 0x103;
const ROLE_TONE: i32 = 0x104;
const ROLE_PREVIEW: i32 = 0x105;
const ROLE_SUMMARY: i32 = 0x106;

/// Rust side of `CaptureModel`.
pub struct CaptureModelRust {
    tab: i32,
    /// Rows the view currently knows about.
    rows: usize,
    selected: i32,
}

impl Default for CaptureModelRust {
    fn default() -> Self {
        Self { tab: TAB_SIGNALS, rows: 0, selected: -1 }
    }
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

/// What a transfer shows when the switch is on pulses (not offered there).
fn transfer_encoding(view: &ViewSettings) -> ir_codec::Encoding {
    match display::current(view) {
        Shown::Encoded(e) => e,
        Shown::Pulses => view.encoding,
    }
}

fn tone_number(tone: Tone) -> i32 {
    match tone {
        Tone::Ir => 0,
        Tone::Control => 1,
        Tone::Problem => 2,
    }
}

fn strings(items: impl IntoIterator<Item = String>) -> QStringList {
    let list: Vec<QString> = items.into_iter().map(|s| QString::from(&s)).collect();
    list.iter().collect()
}

impl qobject::CaptureModel {
    fn set_tab(mut self: Pin<&mut Self>, tab: i32) {
        if self.tab == tab {
            return;
        }
        self.as_mut().rust_mut().tab = tab;
        self.as_mut().reformat();
    }

    fn log_rows(tab: i32) -> usize {
        state::with(|s| {
            if tab == TAB_TRANSFERS { s.core.capture.transfer_count() } else { s.core.capture.signal_count() }
        })
    }

    fn display_options(&self) -> QStringList {
        let tab = self.tab;
        let labels: Vec<String> = state::with(|s| {
            let view = *view_of(&mut s.core.settings, tab);
            options_for(&view, tab).into_iter().map(|o| display::label(o).to_string()).collect()
        });
        strings(labels)
    }

    fn display_index(&self) -> i32 {
        let tab = self.tab;
        state::with(|s| {
            let view = *view_of(&mut s.core.settings, tab);
            let current = display::current(&view);
            options_for(&view, tab).iter().position(|o| *o == current).map_or(0, |i| i as i32)
        })
    }

    fn set_display_index(self: Pin<&mut Self>, index: i32) {
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
            self.reformat();
        }
    }

    fn time_of_day(&self) -> bool {
        state::with(|s| s.core.settings.capture.time_of_day)
    }

    fn select(mut self: Pin<&mut Self>, row: i32) {
        let row = if row >= 0 && (row as usize) < self.rows { row } else { -1 };
        if self.selected != row {
            self.as_mut().rust_mut().selected = row;
        }
        self.selection_changed();
    }

    fn reformat(mut self: Pin<&mut Self>) {
        self.as_mut().begin_reset_model();
        let rows = Self::log_rows(self.tab);
        self.as_mut().rust_mut().rows = rows;
        if self.selected >= 0 && self.selected as usize >= rows {
            self.as_mut().rust_mut().selected = -1;
        }
        self.as_mut().end_reset_model();
        self.as_mut().display_changed();
        self.selection_changed();
    }

    fn sync(mut self: Pin<&mut Self>) {
        let now = Self::log_rows(self.tab);
        let known = self.rows;
        if now < known {
            self.reformat();
        } else if now > known {
            let first = i32::try_from(known).unwrap_or(i32::MAX);
            let last = i32::try_from(now - 1).unwrap_or(i32::MAX);
            self.as_mut().begin_insert_rows(&QModelIndex::default(), first, last);
            self.as_mut().rust_mut().rows = now;
            self.as_mut().end_insert_rows();
        }
    }

    /// The detail of the selected row, if any.
    fn detail(&self) -> Option<Detail> {
        let row = usize::try_from(self.selected).ok()?;
        let tab = self.tab;
        state::with(|s| {
            let view = *view_of(&mut s.core.settings, tab);
            let options = view.format_options();
            if tab == TAB_TRANSFERS {
                let r = s.core.capture.transfer(row)?;
                Some(capture_view::transfer_detail(row + 1, r, transfer_encoding(&view), &options))
            } else {
                let r = s.core.capture.signal(row)?;
                Some(capture_view::signal_detail(row + 1, r, display::current(&view), &options))
            }
        })
    }

    fn detail_title(&self) -> QString {
        QString::from(&self.detail().map(|d| d.title).unwrap_or_default())
    }

    fn detail_bytes(&self) -> QString {
        QString::from(&self.detail().map(|d| d.bytes).unwrap_or_default())
    }

    fn detail_fact_labels(&self) -> QStringList {
        strings(self.detail().map(|d| d.facts.into_iter().map(|(l, _)| l).collect::<Vec<_>>()).unwrap_or_default())
    }

    fn detail_fact_values(&self) -> QStringList {
        strings(self.detail().map(|d| d.facts.into_iter().map(|(_, v)| v).collect::<Vec<_>>()).unwrap_or_default())
    }

    fn detail_waveform(&self) -> QList<i32> {
        let waveform = self.detail().map(|d| d.waveform).unwrap_or_default();
        waveform.into_iter().map(|us| i32::try_from(us).unwrap_or(i32::MAX)).collect()
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        i32::try_from(self.rows).unwrap_or(i32::MAX)
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROLE_NUMBER, "number"),
            (ROLE_TIME, "time"),
            (ROLE_WALL_MS, "wallMs"),
            (ROLE_KIND, "kind"),
            (ROLE_TONE, "tone"),
            (ROLE_PREVIEW, "preview"),
            (ROLE_SUMMARY, "summary"),
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
        if role == ROLE_NUMBER {
            return QVariant::from(&i32::try_from(row + 1).unwrap_or(i32::MAX));
        }
        let tab = self.tab;
        enum Value {
            Text(String),
            Number(f64),
            Tone(i32),
        }
        let value = state::with(|s| {
            let view = *view_of(&mut s.core.settings, tab);
            let options = view.format_options();
            if tab == TAB_TRANSFERS {
                let r = s.core.capture.transfer(row)?;
                let (kind, tone) = capture_view::transfer_kind(r);
                Some(match role {
                    ROLE_TIME => Value::Text(format_time(r.at)),
                    ROLE_WALL_MS => Value::Number(r.wall_ms.map_or(-1.0, |ms| ms as f64)),
                    ROLE_KIND => Value::Text(kind),
                    ROLE_TONE => Value::Tone(tone_number(tone)),
                    ROLE_PREVIEW => Value::Text(capture_view::preview(
                        &transfer_data_text(r, transfer_encoding(&view), &options),
                        PREVIEW_CHARS,
                    )),
                    ROLE_SUMMARY => Value::Text(String::new()),
                    _ => return None,
                })
            } else {
                let r = s.core.capture.signal(row)?;
                let (kind, tone) = capture_view::signal_kind(r);
                Some(match role {
                    ROLE_TIME => Value::Text(format_time(r.at)),
                    ROLE_WALL_MS => Value::Number(r.wall_ms.map_or(-1.0, |ms| ms as f64)),
                    ROLE_KIND => Value::Text(kind),
                    ROLE_TONE => Value::Tone(tone_number(tone)),
                    ROLE_PREVIEW => Value::Text(capture_view::preview(
                        &signal_data_text(r, display::current(&view), &options),
                        PREVIEW_CHARS,
                    )),
                    ROLE_SUMMARY => Value::Text(r.summary.clone()),
                    _ => return None,
                })
            }
        });
        match value {
            Some(Value::Text(text)) => QVariant::from(&QString::from(&text)),
            Some(Value::Number(n)) => QVariant::from(&n),
            Some(Value::Tone(t)) => QVariant::from(&t),
            None => QVariant::default(),
        }
    }
}
