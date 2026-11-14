// DevicePage.qml - the dongle connection.
//
// Shows the connection state, the device line and its protocol, the
// waiting-for-IR banner and the last error, with Connect and Disconnect.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.ScrollablePage {
    title: i18nc("@title", "Device")

    actions: [
        Kirigami.Action {
            text: i18nc("@action:button", "Connect")
            icon.name: "network-connect"
            enabled: DeviceController.canConnect
            onTriggered: DeviceController.connectDevice()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Disconnect")
            icon.name: "network-disconnect"
            enabled: DeviceController.canDisconnect
            onTriggered: DeviceController.disconnectDevice()
        }
    ]

    ColumnLayout {
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: DeviceController.banner !== ""
            type: Kirigami.MessageType.Information
            text: DeviceController.banner
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

        Kirigami.FormLayout {
            Layout.fillWidth: true

            Controls.Label {
                Kirigami.FormData.label: i18nc("@label", "State:")
                text: DeviceController.stateLabel
            }
            Controls.Label {
                Kirigami.FormData.label: i18nc("@label", "Device:")
                text: DeviceController.device !== "" ? DeviceController.device : i18nc("@info", "none")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Kirigami.FormData.label: i18nc("@label", "Protocol:")
                text: DeviceController.protocol !== "" ? DeviceController.protocol : i18nc("@info", "unknown")
            }
        }
    }
}
