//! `LibraryModel`: the QML singleton for the code library.
//!
//! Exists to list the saved codes filtered by a search text and a tag
//! (design decision 23), and to add, edit, delete and send them. Every
//! change is saved to the library file at once. The edit dialog works on the
//! `edit*` properties: `beginEdit` fills them from a code, `commitEdit`
//! validates them through `ir_view::library::EditForm` and saves.

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
        #[qml_singleton]
        #[base = QAbstractListModel]
        #[qproperty(QString, query, READ, WRITE = set_query, NOTIFY = filter_changed)]
        #[qproperty(QString, tag, READ, WRITE = set_tag, NOTIFY = filter_changed)]
        #[qproperty(QStringList, tags, READ = tags, NOTIFY = library_changed)]
        #[qproperty(i32, total, READ = total, NOTIFY = library_changed)]
        #[qproperty(QString, file_path, READ = file_path, CONSTANT)]
        #[qproperty(QString, error)]
        #[qproperty(QString, edit_name)]
        #[qproperty(QString, edit_tags)]
        #[qproperty(QString, edit_carrier)]
        #[qproperty(i32, edit_repeat_index)]
        #[qproperty(i32, edit_repeat_count)]
        #[qproperty(QString, edit_notes)]
        type LibraryModel = super::LibraryModelRust;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        fn begin_reset_model(self: Pin<&mut LibraryModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        fn end_reset_model(self: Pin<&mut LibraryModel>);
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        fn set_query(self: Pin<&mut LibraryModel>, query: QString);
        fn set_tag(self: Pin<&mut LibraryModel>, tag: QString);
        fn tags(self: &LibraryModel) -> QStringList;
        fn total(self: &LibraryModel) -> i32;
        fn file_path(self: &LibraryModel) -> QString;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &LibraryModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        fn row_count(self: &LibraryModel, parent: &QModelIndex) -> i32;
        #[qinvokable]
        #[cxx_override]
        fn role_names(self: &LibraryModel) -> QHash_i32_QByteArray;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qsignal]
        fn filter_changed(self: Pin<&mut LibraryModel>);
        #[qsignal]
        fn library_changed(self: Pin<&mut LibraryModel>);

        /// Re-reads the shown codes after the library changed elsewhere.
        #[qinvokable]
        fn reload(self: Pin<&mut LibraryModel>);
        /// Saves captured signal `row` as a new code; returns its id or -1.
        #[qinvokable]
        fn save_signal(self: Pin<&mut LibraryModel>, row: i32) -> i32;
        /// Sends the code with id `id`; returns false and sets `error` on failure.
        #[qinvokable]
        fn send(self: Pin<&mut LibraryModel>, id: i32) -> bool;
        #[qinvokable]
        fn remove(self: Pin<&mut LibraryModel>, id: i32) -> bool;
        /// Fills the `edit*` properties from the code with id `id`.
        #[qinvokable]
        fn begin_edit(self: Pin<&mut LibraryModel>, id: i32) -> bool;
        /// Writes the `edit*` properties into the code; false and `error` when invalid.
        #[qinvokable]
        fn commit_edit(self: Pin<&mut LibraryModel>) -> bool;
    }
}

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QStringList, QVariant};
use ir_library::model::RepeatMode;
use ir_library::model::iso_utc;
use ir_transport::recorder::now_unix_ms;
use ir_view::library::{EditForm, added_verb, from_signal, payload_icon, summary};
use ir_view::send::command_for;

use ir_view::text::capitalise;

use crate::state;

const ROLE_ID: i32 = 0x100;
const ROLE_NAME: i32 = 0x101;
const ROLE_SUMMARY: i32 = 0x102;
const ROLE_TAGS: i32 = 0x103;
const ROLE_NOTES: i32 = 0x104;
const ROLE_ICON: i32 = 0x105;
const ROLE_ADDED: i32 = 0x106;
const ROLE_ADDED_VERB: i32 = 0x107;

/// Repeat choices of the edit dialog: default, as learned, first frame.
const REPEAT_DEFAULT: i32 = 0;
const REPEAT_AS_LEARNED: i32 = 1;
const REPEAT_FIRST_FRAME: i32 = 2;

/// Rust side of `LibraryModel`.
pub struct LibraryModelRust {
    query: QString,
    tag: QString,
    error: QString,
    /// Ids of the shown codes, in library order.
    shown: Vec<u64>,
    /// Code being edited.
    editing: Option<u64>,
    edit_name: QString,
    edit_tags: QString,
    edit_carrier: QString,
    edit_repeat_index: i32,
    edit_repeat_count: i32,
    edit_notes: QString,
}

impl Default for LibraryModelRust {
    fn default() -> Self {
        let mut model = Self {
            query: QString::default(),
            tag: QString::default(),
            error: QString::default(),
            shown: Vec::new(),
            editing: None,
            edit_name: QString::default(),
            edit_tags: QString::default(),
            edit_carrier: QString::default(),
            edit_repeat_index: REPEAT_DEFAULT,
            edit_repeat_count: 1,
            edit_notes: QString::default(),
        };
        model.shown = model.filtered();
        model
    }
}

impl LibraryModelRust {
    fn filtered(&self) -> Vec<u64> {
        let query = self.query.to_string();
        let tag = self.tag.to_string();
        state::with(|s| {
            let tag = (!tag.is_empty()).then_some(tag.as_str());
            s.library.search(&query, tag).into_iter().map(|c| c.id).collect()
        })
    }
}

fn id_of(id: i32) -> Option<u64> {
    u64::try_from(id).ok()
}

impl qobject::LibraryModel {
    /// Re-runs the filter and resets the view.
    fn refresh(mut self: Pin<&mut Self>) {
        self.as_mut().begin_reset_model();
        let shown = self.filtered();
        self.as_mut().rust_mut().shown = shown;
        self.as_mut().end_reset_model();
    }

    /// Saves the library and refreshes; shows a save error.
    fn changed(mut self: Pin<&mut Self>) -> bool {
        let saved = state::with(|s| s.save_library());
        self.as_mut().refresh();
        self.as_mut().library_changed();
        let message = saved.as_ref().err().cloned().unwrap_or_default();
        self.set_error(QString::from(&message));
        saved.is_ok()
    }

    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().refresh();
        self.library_changed();
    }

    fn set_query(mut self: Pin<&mut Self>, query: QString) {
        if self.query == query {
            return;
        }
        self.as_mut().rust_mut().query = query;
        self.as_mut().refresh();
        self.filter_changed();
    }

    fn set_tag(mut self: Pin<&mut Self>, tag: QString) {
        if self.tag == tag {
            return;
        }
        self.as_mut().rust_mut().tag = tag;
        self.as_mut().refresh();
        self.filter_changed();
    }

    fn tags(&self) -> QStringList {
        let tags: Vec<QString> = state::with(|s| s.library.all_tags().iter().map(QString::from).collect());
        tags.iter().collect()
    }

    fn file_path(&self) -> QString {
        state::with(|s| QString::from(&s.library_path().map(|p| p.display().to_string()).unwrap_or_default()))
    }

    fn total(&self) -> i32 {
        state::with(|s| i32::try_from(s.library.codes.len()).unwrap_or(i32::MAX))
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        i32::try_from(self.shown.len()).unwrap_or(i32::MAX)
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (role, name) in [
            (ROLE_ID, "codeId"),
            (ROLE_NAME, "name"),
            (ROLE_SUMMARY, "summary"),
            (ROLE_TAGS, "tags"),
            (ROLE_NOTES, "notes"),
            (ROLE_ICON, "iconName"),
            (ROLE_ADDED, "added"),
            (ROLE_ADDED_VERB, "addedVerb"),
        ] {
            roles.insert(role, QByteArray::from(name));
        }
        roles
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(&id) = usize::try_from(index.row()).ok().and_then(|r| self.shown.get(r)) else {
            return QVariant::default();
        };
        if role == ROLE_ID {
            return QVariant::from(&i32::try_from(id).unwrap_or(-1));
        }
        let text = state::with(|s| {
            let code = s.library.get(id)?;
            Some(match role {
                ROLE_NAME => code.name.clone(),
                ROLE_SUMMARY => summary(code),
                ROLE_TAGS => code.tags.join(", "),
                ROLE_NOTES => code.notes.clone(),
                ROLE_ICON => payload_icon(&code.payload).to_string(),
                ROLE_ADDED => code.added.clone().unwrap_or_default(),
                ROLE_ADDED_VERB => added_verb(&code.payload).to_string(),
                _ => return None,
            })
        });
        text.map_or_else(QVariant::default, |t| QVariant::from(&QString::from(&t)))
    }

    fn save_signal(mut self: Pin<&mut Self>, row: i32) -> i32 {
        let id = state::with(|s| {
            let signal = usize::try_from(row).ok().and_then(|r| s.core.capture.signal(r))?;
            let (name, payload) = from_signal(signal);
            let id = s.library.add(name, payload);
            if let Some(code) = s.library.get_mut(id) {
                code.added = Some(iso_utc(now_unix_ms() / 1000));
            }
            Some(id)
        });
        match id {
            Some(id) => {
                self.as_mut().changed();
                i32::try_from(id).unwrap_or(-1)
            }
            None => -1,
        }
    }

    fn send(mut self: Pin<&mut Self>, id: i32) -> bool {
        let result = state::with(|s| {
            let code = id_of(id).and_then(|id| s.library.get(id)).ok_or("the code no longer exists")?;
            let command = command_for(code, &s.core.settings.send_defaults())?;
            s.send_command(command)
        });
        let message = result.as_ref().err().map_or_else(String::new, |e| capitalise(e));
        self.as_mut().set_error(QString::from(&message));
        result.is_ok()
    }

    fn remove(mut self: Pin<&mut Self>, id: i32) -> bool {
        let removed = state::with(|s| id_of(id).and_then(|id| s.library.remove(id)).is_some());
        removed && self.as_mut().changed()
    }

    fn begin_edit(mut self: Pin<&mut Self>, id: i32) -> bool {
        let Some(form) = state::with(|s| id_of(id).and_then(|id| s.library.get(id)).map(EditForm::from_code)) else {
            return false;
        };
        let (repeat_index, repeat_count) = match form.repeat {
            None => (REPEAT_DEFAULT, 1),
            Some(RepeatMode::AsLearned) => (REPEAT_AS_LEARNED, 1),
            Some(RepeatMode::FirstFrame { count }) => (REPEAT_FIRST_FRAME, i32::try_from(count).unwrap_or(1)),
        };
        self.as_mut().rust_mut().editing = id_of(id);
        self.as_mut().set_edit_name(QString::from(&form.name));
        self.as_mut().set_edit_tags(QString::from(&form.tags_text));
        self.as_mut().set_edit_carrier(QString::from(&form.carrier_text));
        self.as_mut().set_edit_repeat_index(repeat_index);
        self.as_mut().set_edit_repeat_count(repeat_count);
        self.as_mut().set_edit_notes(QString::from(&form.notes));
        self.set_error(QString::default());
        true
    }

    fn commit_edit(mut self: Pin<&mut Self>) -> bool {
        let Some(id) = self.editing else { return false };
        let form = EditForm {
            name: self.edit_name.to_string(),
            tags_text: self.edit_tags.to_string(),
            carrier_text: self.edit_carrier.to_string(),
            repeat: match self.edit_repeat_index {
                REPEAT_DEFAULT => None,
                REPEAT_AS_LEARNED => Some(RepeatMode::AsLearned),
                _ => Some(RepeatMode::FirstFrame { count: u32::try_from(self.edit_repeat_count).unwrap_or(1).max(1) }),
            },
            notes: self.edit_notes.to_string(),
        };
        let result = state::with(|s| {
            let code = s.library.get_mut(id).ok_or("the code no longer exists")?;
            form.apply(code)
        });
        match result {
            Ok(()) => {
                self.as_mut().rust_mut().editing = None;
                self.as_mut().changed()
            }
            Err(message) => {
                self.set_error(QString::from(&capitalise(&message)));
                false
            }
        }
    }
}
