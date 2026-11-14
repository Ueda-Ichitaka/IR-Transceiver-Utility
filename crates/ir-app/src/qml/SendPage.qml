// SendPage.qml - send a code, laid out as in the mockup.
//
// Header: Load from library and the encoding switch (segmented buttons). The
// form: payload type (learned code, pulse timings, NEC, raw USB bytes), the
// data with a live hint, carrier and repeat (both fall back to the global
// defaults) and the wire preview of the USB transfers. Below: Transmit and
// Save to library, and the last send with its outcome.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title", "Send")

    readonly property int kindLearned: 0
    readonly property int kindPulses: 1
    readonly property int kindNec: 2
    readonly property int kindRaw: 3

    SendController {
        id: form
    }

    // Smoke test: loads the first library code and logs the hint.
    function smokeLoadFirstCode() {
        form.loadCode(form.libraryIds()[0]);
        console.info("ir-transceiver: send hint " + form.hint);
    }

    Connections {
        target: SettingsController
        function onSaved() {
            form.refreshDefaults();
        }
    }

    actions: [
        Kirigami.Action {
            text: i18nc("@action:button", "Load from library…")
            icon.name: "document-open"
            onTriggered: loadDialog.open()
        }
    ]

    Kirigami.Dialog {
        id: loadDialog
        title: i18nc("@title:dialog", "Load from Library")
        preferredWidth: Kirigami.Units.gridUnit * 20
        standardButtons: Kirigami.Dialog.Cancel
        onOpened: {
            codeList.model = form.libraryNames();
            codeList.ids = form.libraryIds();
        }

        ListView {
            id: codeList
            property var ids: []
            implicitHeight: Kirigami.Units.gridUnit * 14
            clip: true

            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.gridUnit * 2
                visible: codeList.count === 0
                text: i18nc("@info:placeholder", "No saved codes")
            }

            delegate: Controls.ItemDelegate {
                required property int index
                required property string modelData
                width: ListView.view.width
                text: modelData
                onClicked: {
                    form.loadCode(codeList.ids[index]);
                    loadDialog.close();
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: saveDialog
        title: i18nc("@title:dialog", "Save to Library")
        standardButtons: Kirigami.Dialog.Save | Kirigami.Dialog.Cancel
        onAccepted: {
            if (form.saveToLibrary(nameField.text) >= 0) {
                LibraryModel.reload();
                savedNotice.visible = true;
            }
        }
        Controls.TextField {
            id: nameField
            placeholderText: i18nc("@info:placeholder", "Name")
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: form.error !== ""
            type: Kirigami.MessageType.Error
            text: form.error
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: DeviceController.lastError !== ""
            type: Kirigami.MessageType.Warning
            text: DeviceController.lastError
            actions: Kirigami.Action {
                text: i18nc("@action:button", "Dismiss")
                icon.name: "dialog-close"
                onTriggered: DeviceController.clearError()
            }
        }

        Kirigami.InlineMessage {
            id: savedNotice
            Layout.fillWidth: true
            visible: false
            type: Kirigami.MessageType.Positive
            text: i18nc("@info", "Saved to the library.")
            actions: Kirigami.Action {
                text: i18nc("@action:button", "Dismiss")
                icon.name: "dialog-close"
                onTriggered: savedNotice.visible = false
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Row {
                Kirigami.FormData.label: i18nc("@label", "Encoding:")
                visible: form.kind === page.kindLearned || form.kind === page.kindRaw
                Repeater {
                    model: form.encodingOptions
                    delegate: Controls.Button {
                        required property int index
                        required property string modelData
                        text: modelData
                        checkable: true
                        checked: form.encodingIndex === index
                        onClicked: form.encodingIndex = index
                    }
                }
            }

            ColumnLayout {
                Kirigami.FormData.label: i18nc("@label", "Payload type:")
                Kirigami.FormData.labelAlignment: Qt.AlignTop
                Repeater {
                    model: form.kindOptions
                    delegate: Controls.RadioButton {
                        required property int index
                        required property string modelData
                        text: modelData
                        checked: form.kind === index
                        onToggled: if (checked) form.kind = index
                    }
                }
            }

            ColumnLayout {
                Kirigami.FormData.label: i18nc("@label", "Data:")
                Kirigami.FormData.labelAlignment: Qt.AlignTop
                Layout.fillWidth: true

                Controls.TextArea {
                    visible: form.kind === page.kindLearned || form.kind === page.kindRaw
                    Layout.fillWidth: true
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 20
                    Layout.minimumHeight: Kirigami.Units.gridUnit * 5
                    font.family: "monospace"
                    wrapMode: TextEdit.WrapAnywhere
                    placeholderText: form.kind === page.kindRaw ? "FC FC FC FC" : "FF FF 37 FF 17 26 …"
                    text: form.bytesText
                    onTextChanged: if (text !== form.bytesText) form.bytesText = text
                }

                Controls.TextArea {
                    visible: form.kind === page.kindPulses
                    Layout.fillWidth: true
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 20
                    Layout.minimumHeight: Kirigami.Units.gridUnit * 5
                    font.family: "monospace"
                    wrapMode: TextEdit.Wrap
                    placeholderText: i18nc("@info:placeholder", "on/off in µs, e.g. 9000/4500 560/560 560/1690")
                    text: form.pulsesText
                    onTextChanged: if (text !== form.pulsesText) form.pulsesText = text
                }

                RowLayout {
                    visible: form.kind === page.kindNec
                    Controls.Label {
                        text: i18nc("@label:textbox", "Address (hex)")
                    }
                    Controls.TextField {
                        placeholderText: form.necExtended ? "EF00" : "04"
                        text: form.necAddress
                        onTextChanged: if (text !== form.necAddress) form.necAddress = text
                    }
                    Controls.Label {
                        text: i18nc("@label:textbox", "Command (hex)")
                    }
                    Controls.TextField {
                        placeholderText: "08"
                        text: form.necCommand
                        onTextChanged: if (text !== form.necCommand) form.necCommand = text
                    }
                    Controls.CheckBox {
                        text: i18nc("@option:check", "Extended (16-bit address)")
                        checked: form.necExtended
                        onToggled: form.necExtended = checked
                    }
                }

                Controls.Label {
                    text: form.hint
                    color: form.valid ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.negativeTextColor
                    wrapMode: Text.Wrap
                    Layout.fillWidth: true
                }
            }

            RowLayout {
                Kirigami.FormData.label: i18nc("@label", "Carrier:")
                visible: form.kind !== page.kindRaw
                Controls.TextField {
                    placeholderText: form.defaultCarrier
                    text: form.carrierText
                    onTextChanged: if (text !== form.carrierText) form.carrierText = text
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                }
                Controls.Label {
                    text: i18nc("@info", "Hz (empty: default %1 Hz; 38 kHz fits almost all remotes)", form.defaultCarrier)
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            RowLayout {
                Kirigami.FormData.label: i18nc("@label", "Repeat:")
                visible: form.kind !== page.kindRaw
                Controls.ComboBox {
                    model: form.repeatOptions
                    currentIndex: form.repeatIndex
                    onActivated: index => form.repeatIndex = index
                }
                Controls.SpinBox {
                    visible: form.repeatIndex === 2
                    from: 1
                    to: 100
                    value: form.repeatCount
                    onValueModified: form.repeatCount = value
                }
                Controls.Label {
                    visible: form.repeatIndex === 2
                    text: i18nc("@info", "times")
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            Controls.TextArea {
                Kirigami.FormData.label: i18nc("@label", "Wire preview:")
                Kirigami.FormData.labelAlignment: Qt.AlignTop
                Layout.fillWidth: true
                Layout.minimumHeight: Kirigami.Units.gridUnit * 4
                readOnly: true
                font.family: "monospace"
                wrapMode: TextEdit.WrapAnywhere
                textFormat: TextEdit.PlainText
                text: form.wirePreview
            }

            RowLayout {
                Controls.Button {
                    text: i18nc("@action:button", "Transmit")
                    icon.name: "document-send"
                    enabled: DeviceController.ready && form.valid
                    highlighted: true
                    onClicked: if (form.send()) DeviceController.refreshStatus()
                }
                Controls.Button {
                    text: i18nc("@action:button", "Save to library…")
                    icon.name: "document-save"
                    enabled: form.valid
                    onClicked: {
                        nameField.text = "";
                        saveDialog.open();
                    }
                }
            }
        }
    }

    footer: Controls.Label {
        padding: Kirigami.Units.smallSpacing
        visible: DeviceController.lastSendText !== ""
        text: DeviceController.lastSendWallMs >= 0
            ? i18nc("@info", "Last send %1 · %2", Qt.formatTime(new Date(DeviceController.lastSendWallMs), "hh:mm:ss"),
                DeviceController.lastSendText)
            : ""
        color: Kirigami.Theme.disabledTextColor
        font: Kirigami.Theme.smallFont
    }
}
