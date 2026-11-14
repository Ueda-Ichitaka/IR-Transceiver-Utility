// LibraryPage.qml - the saved codes.
//
// A searchable list of codes with a tag filter (design decision 23), each
// with an icon for its payload kind and the date it was learned or added. Each
// row can be sent, edited (name, tags, carrier, repeat, notes), copied as
// Pronto hex or deleted. All changes go through the LibraryModel singleton,
// which saves at once. Import adds codes from Flipper, LIRC or Pronto files;
// Export writes the shown codes as LIRC or Flipper files.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title", "Library")

    actions: [
        Kirigami.Action {
            text: i18nc("@action:button", "Import…")
            icon.name: "document-import"
            onTriggered: importDialog.open()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Export")
            icon.name: "document-export"
            enabled: list.count > 0

            Kirigami.Action {
                text: i18nc("@action:inmenu", "LIRC config…")
                onTriggered: exportDialog.ask("lirc", "conf", i18nc("@item:inlistbox", "LIRC configs (*.conf)"))
            }
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Flipper file…")
                onTriggered: exportDialog.ask("flipper", "ir", i18nc("@item:inlistbox", "Flipper IR files (*.ir)"))
            }
        }
    ]

    Dialogs.FileDialog {
        id: importDialog
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [i18nc("@item:inlistbox", "IR code files (*.ir *.conf *.txt)"), i18nc("@item:inlistbox", "All files (*)")]
        onAccepted: if (FileActions.importLibrary(selectedFile)) LibraryModel.reload()
    }

    Dialogs.FileDialog {
        id: exportDialog
        property string format: ""
        function ask(format, suffix, filter) {
            exportDialog.format = format;
            defaultSuffix = suffix;
            nameFilters = [filter];
            open();
        }
        fileMode: Dialogs.FileDialog.SaveFile
        onAccepted: FileActions.exportLibrary(selectedFile, format, LibraryModel.query, LibraryModel.tag)
    }

    // QML has no clipboard API; a hidden text field copies through Qt.
    Controls.TextField {
        id: clipboardHelper
        visible: false
    }

    header: ColumnLayout {
        spacing: 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: FileActions.message !== ""
            type: FileActions.ok ? Kirigami.MessageType.Positive : Kirigami.MessageType.Error
            text: FileActions.message
            actions: Kirigami.Action {
                text: i18nc("@action:button", "Dismiss")
                icon.name: "dialog-close"
                onTriggered: FileActions.message = ""
            }
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: LibraryModel.error !== "" && !editDialog.opened
            type: Kirigami.MessageType.Error
            text: LibraryModel.error
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: DeviceController.lastSent !== ""
            type: Kirigami.MessageType.Positive
            text: DeviceController.lastSent
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing

            Kirigami.SearchField {
                Layout.fillWidth: true
                onTextChanged: LibraryModel.query = text
            }

            Controls.ComboBox {
                id: tagFilter
                model: [i18nc("@item:inlistbox", "All tags")].concat(LibraryModel.tags)
                onActivated: index => LibraryModel.tag = index === 0 ? "" : LibraryModel.tags[index - 1]
            }
        }
    }

    footer: Controls.Label {
        padding: Kirigami.Units.smallSpacing
        text: LibraryModel.filePath !== ""
            ? i18ncp("@info", "%1 code · %2", "%1 codes · %2", LibraryModel.total, LibraryModel.filePath)
            : i18ncp("@info", "%1 code", "%1 codes", LibraryModel.total)
        color: Kirigami.Theme.disabledTextColor
        font: Kirigami.Theme.smallFont
        elide: Text.ElideMiddle
    }

    ListView {
        id: list

        model: LibraryModel

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: list.count === 0
            text: LibraryModel.total === 0
                ? i18nc("@info:placeholder", "No saved codes")
                : i18nc("@info:placeholder", "No code matches")
            explanation: LibraryModel.total === 0
                ? i18nc("@info:placeholder", "Save a captured signal from the Capture page.")
                : ""
        }

        delegate: Controls.ItemDelegate {
            id: row
            required property int codeId
            required property string name
            required property string summary
            required property string tags
            required property string notes
            required property string iconName
            required property string added
            required property string addedVerb

            width: ListView.view.width
            Component.onCompleted: {
                if (Qt.application.arguments.indexOf("--smoke-test") >= 0) {
                    console.info("ir-transceiver: code " + name + " | " + summary);
                }
            }
            contentItem: RowLayout {
                Kirigami.Icon {
                    source: row.iconName
                    implicitWidth: Kirigami.Units.iconSizes.medium
                    implicitHeight: Kirigami.Units.iconSizes.medium
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Controls.Label {
                        text: row.name
                        font.bold: true
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                    }
                    Controls.Label {
                        text: row.added !== ""
                            ? row.summary + " · " + row.addedVerb + " " + Qt.formatDate(new Date(row.added), "yyyy-MM-dd")
                            : row.summary
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Label {
                        visible: row.tags !== ""
                        text: row.tags
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
                    }
                }
                Controls.Button {
                    text: i18nc("@action:button", "Send")
                    icon.name: "document-send"
                    enabled: DeviceController.ready
                    onClicked: LibraryModel.send(row.codeId)
                }
                Controls.Button {
                    text: i18nc("@action:button", "Edit")
                    icon.name: "document-edit"
                    onClicked: if (LibraryModel.beginEdit(row.codeId)) editDialog.open()
                }
                Controls.Button {
                    icon.name: "edit-copy"
                    display: Controls.AbstractButton.IconOnly
                    text: i18nc("@action:button", "Copy as Pronto hex")
                    Controls.ToolTip.text: text
                    Controls.ToolTip.visible: hovered
                    onClicked: {
                        const hex = FileActions.prontoForCode(row.codeId);
                        if (hex !== "") {
                            clipboardHelper.text = hex;
                            clipboardHelper.selectAll();
                            clipboardHelper.copy();
                        }
                    }
                }
                Controls.Button {
                    icon.name: "edit-delete"
                    display: Controls.AbstractButton.IconOnly
                    text: i18nc("@action:button", "Delete")
                    Controls.ToolTip.text: text
                    Controls.ToolTip.visible: hovered
                    onClicked: {
                        deleteDialog.codeId = row.codeId;
                        deleteDialog.codeName = row.name;
                        deleteDialog.open();
                    }
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: deleteDialog
        property int codeId: -1
        property string codeName: ""
        title: i18nc("@title:dialog", "Delete Code")
        subtitle: i18nc("@info", "Delete \"%1\" from the library?", codeName)
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
        onAccepted: LibraryModel.remove(codeId)
    }

    Kirigami.Dialog {
        id: editDialog
        title: i18nc("@title:dialog", "Edit Code")
        padding: Kirigami.Units.largeSpacing
        standardButtons: Kirigami.Dialog.Cancel
        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Save")
                icon.name: "document-save"
                onTriggered: if (LibraryModel.commitEdit()) editDialog.close()
            }
        ]

        ColumnLayout {
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: LibraryModel.error !== ""
                type: Kirigami.MessageType.Error
                text: LibraryModel.error
            }

            Kirigami.FormLayout {
                Controls.TextField {
                    Kirigami.FormData.label: i18nc("@label:textbox", "Name:")
                    text: LibraryModel.editName
                    onTextEdited: LibraryModel.editName = text
                }
                Controls.TextField {
                    Kirigami.FormData.label: i18nc("@label:textbox", "Tags:")
                    placeholderText: i18nc("@info:placeholder", "comma separated")
                    text: LibraryModel.editTags
                    onTextEdited: LibraryModel.editTags = text
                }
                Controls.TextField {
                    Kirigami.FormData.label: i18nc("@label:textbox", "Carrier (Hz):")
                    placeholderText: i18nc("@info:placeholder", "Default")
                    text: LibraryModel.editCarrier
                    onTextEdited: LibraryModel.editCarrier = text
                }
                RowLayout {
                    Kirigami.FormData.label: i18nc("@label:listbox", "Repeat:")
                    Controls.ComboBox {
                        model: [i18nc("@item:inlistbox", "Default"), i18nc("@item:inlistbox", "As learned"),
                            i18nc("@item:inlistbox", "First frame only")]
                        currentIndex: LibraryModel.editRepeatIndex
                        onActivated: index => LibraryModel.editRepeatIndex = index
                    }
                    Controls.SpinBox {
                        visible: LibraryModel.editRepeatIndex === 2
                        from: 1
                        to: 100
                        value: LibraryModel.editRepeatCount
                        onValueModified: LibraryModel.editRepeatCount = value
                    }
                }
                Controls.TextArea {
                    Kirigami.FormData.label: i18nc("@label:textbox", "Notes:")
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 16
                    Layout.minimumHeight: Kirigami.Units.gridUnit * 4
                    wrapMode: TextEdit.Wrap
                    text: LibraryModel.editNotes
                    onTextChanged: if (text !== LibraryModel.editNotes) LibraryModel.editNotes = text
                }
            }
        }
    }
}
