// SendPage.qml - send a code typed or pasted by the user.
//
// One form for the four payload kinds (learned code, pulse timings, NEC,
// raw USB bytes). Carrier and repeat fall back to the global defaults when
// left at "Default". The bytes field uses the Send view's encoding switch.
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

    actions: [
        Kirigami.Action {
            text: i18nc("@action:button", "Send")
            icon.name: "document-send"
            enabled: DeviceController.ready
            onTriggered: form.send()
        }
    ]

    ColumnLayout {
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
            showCloseButton: true
            onVisibleChanged: if (!visible && DeviceController.lastError !== "") DeviceController.clearError()
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: DeviceController.lastSent !== "" && form.error === ""
            type: Kirigami.MessageType.Positive
            text: DeviceController.lastSent
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.ComboBox {
                Kirigami.FormData.label: i18nc("@label:listbox", "Payload:")
                model: form.kindOptions
                currentIndex: form.kind
                onActivated: index => form.kind = index
            }

            Controls.ComboBox {
                Kirigami.FormData.label: i18nc("@label:listbox", "Encoding:")
                visible: form.kind === page.kindLearned || form.kind === page.kindRaw
                model: form.encodingOptions
                currentIndex: form.encodingIndex
                onActivated: index => form.encodingIndex = index
            }

            Controls.TextArea {
                Kirigami.FormData.label: form.kind === page.kindRaw
                    ? i18nc("@label:textbox", "USB bytes:")
                    : i18nc("@label:textbox", "Learned bytes:")
                visible: form.kind === page.kindLearned || form.kind === page.kindRaw
                Layout.fillWidth: true
                Layout.minimumHeight: Kirigami.Units.gridUnit * 6
                font.family: "monospace"
                wrapMode: TextEdit.WrapAnywhere
                text: form.bytesText
                onTextChanged: form.bytesText = text
            }

            Controls.TextArea {
                Kirigami.FormData.label: i18nc("@label:textbox", "Pulses (on/off µs):")
                visible: form.kind === page.kindPulses
                Layout.fillWidth: true
                Layout.minimumHeight: Kirigami.Units.gridUnit * 6
                font.family: "monospace"
                wrapMode: TextEdit.Wrap
                placeholderText: "9000/4500 560/560 560/1690"
                text: form.pulsesText
                onTextChanged: form.pulsesText = text
            }

            Controls.TextField {
                Kirigami.FormData.label: i18nc("@label:textbox", "Address (hex):")
                visible: form.kind === page.kindNec
                placeholderText: form.necExtended ? "EF00" : "00"
                text: form.necAddress
                onTextChanged: form.necAddress = text
            }

            Controls.TextField {
                Kirigami.FormData.label: i18nc("@label:textbox", "Command (hex):")
                visible: form.kind === page.kindNec
                placeholderText: "59"
                text: form.necCommand
                onTextChanged: form.necCommand = text
            }

            Controls.CheckBox {
                visible: form.kind === page.kindNec
                text: i18nc("@option:check", "Extended (16-bit address)")
                checked: form.necExtended
                onToggled: form.necExtended = checked
            }

            Controls.TextField {
                Kirigami.FormData.label: i18nc("@label:textbox", "Carrier (Hz):")
                visible: form.kind !== page.kindRaw
                placeholderText: i18nc("@info:placeholder", "Default: %1", form.defaultCarrier)
                text: form.carrierText
                onTextChanged: form.carrierText = text
            }

            RowLayout {
                Kirigami.FormData.label: i18nc("@label:listbox", "Repeat:")
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
            }
        }
    }
}
