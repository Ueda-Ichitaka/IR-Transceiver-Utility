// SettingsPage.qml - the user settings.
//
// Default carrier and repeat mode for sending, the capture limit and the
// display options shared by every view's encoding switch. Fields are loaded
// when the page is shown and saved with Apply.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.ScrollablePage {
    title: i18nc("@title", "Settings")

    onVisibleChanged: if (visible) SettingsController.load()

    actions: [
        Kirigami.Action {
            text: i18nc("@action:button", "Apply")
            icon.name: "dialog-ok-apply"
            onTriggered: SettingsController.save()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Reset")
            icon.name: "edit-undo"
            onTriggered: SettingsController.load()
        }
    ]

    ColumnLayout {
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: SettingsController.error !== ""
            type: Kirigami.MessageType.Error
            text: SettingsController.error
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: i18nc("@title:group", "Sending")
            }
            Controls.TextField {
                Kirigami.FormData.label: i18nc("@label:textbox", "Default carrier (Hz):")
                text: SettingsController.carrierText
                onTextEdited: SettingsController.carrierText = text
            }
            RowLayout {
                Kirigami.FormData.label: i18nc("@label", "Learned codes:")
                Controls.ComboBox {
                    model: [i18nc("@item:inlistbox", "Send as learned"), i18nc("@item:inlistbox", "Send first frame only")]
                    currentIndex: SettingsController.firstFrameOnly ? 1 : 0
                    onActivated: index => SettingsController.firstFrameOnly = index === 1
                }
                Controls.SpinBox {
                    visible: SettingsController.firstFrameOnly
                    from: 1
                    to: 100
                    value: SettingsController.repeatCount
                    onValueModified: SettingsController.repeatCount = value
                }
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: i18nc("@title:group", "Capture")
            }
            Controls.TextField {
                Kirigami.FormData.label: i18nc("@label:textbox", "Capture limit (MiB):")
                text: SettingsController.limitMibText
                onTextEdited: SettingsController.limitMibText = text
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: i18nc("@title:group", "Display")
            }
            Controls.CheckBox {
                Kirigami.FormData.label: i18nc("@label", "Hex:")
                text: i18nc("@option:check", "Uppercase")
                checked: SettingsController.uppercase
                onToggled: SettingsController.uppercase = checked
            }
            Controls.TextField {
                Kirigami.FormData.label: i18nc("@label:textbox", "Bytes per line (0 = one line):")
                text: SettingsController.bytesPerLineText
                onTextEdited: SettingsController.bytesPerLineText = text
            }
            Controls.CheckBox {
                text: i18nc("@option:check", "Offer pulse timings in the encoding switch")
                checked: SettingsController.offerPulses
                onToggled: SettingsController.offerPulses = checked
            }
        }
    }
}
