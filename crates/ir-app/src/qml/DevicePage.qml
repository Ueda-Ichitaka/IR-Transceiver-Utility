// DevicePage.qml - the dongle connection, laid out as in the mockup.
//
// Header actions: Connect or Disconnect, and Identify. The waiting-for-IR
// banner, then three cards: Connection (device facts, protocol, identify
// reply), Environment check (udev access, interface driver, lircd, hotplug)
// and the USB log (the latest transfers). The status line says when the
// dongle was plugged in and how many errors occurred since.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title", "Device")

    onVisibleChanged: if (visible) DeviceController.refreshEnvironment()

    actions: [
        Kirigami.Action {
            text: DeviceController.canConnect ? i18nc("@action:button", "Connect") : i18nc("@action:button", "Disconnect")
            icon.name: DeviceController.canConnect ? "network-connect" : "network-disconnect"
            onTriggered: DeviceController.canConnect ? DeviceController.connectDevice() : DeviceController.disconnectDevice()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Identify")
            icon.name: "dialog-information"
            enabled: DeviceController.ready
            onTriggered: DeviceController.identify()
        }
    ]

    CaptureModel {
        id: usbLog
        tab: 1
    }

    Connections {
        target: DeviceController
        function onCaptureChanged() {
            usbLog.sync();
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

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

        GridLayout {
            Layout.fillWidth: true
            columns: page.width > Kirigami.Units.gridUnit * 40 ? 2 : 1
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.largeSpacing

            Kirigami.Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                header: Kirigami.Heading {
                    text: i18nc("@title:group", "Connection")
                    level: 4
                    padding: Kirigami.Units.smallSpacing
                }
                contentItem: GridLayout {
                    columns: 2
                    columnSpacing: Kirigami.Units.largeSpacing
                    Controls.Label {
                        visible: DeviceController.factLabels.length === 0
                        Layout.columnSpan: 2
                        text: i18nc("@info", "No dongle connected.")
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Repeater {
                        model: DeviceController.factLabels
                        delegate: Controls.Label {
                            required property int index
                            required property string modelData
                            Layout.row: index + 1
                            Layout.column: 0
                            text: modelData
                            color: Kirigami.Theme.disabledTextColor
                        }
                    }
                    Repeater {
                        model: DeviceController.factValues
                        delegate: Controls.Label {
                            required property int index
                            required property string modelData
                            Layout.row: index + 1
                            Layout.column: 1
                            Layout.fillWidth: true
                            text: modelData
                            font.family: "monospace"
                            wrapMode: Text.WrapAnywhere
                        }
                    }
                }
            }

            Kirigami.Card {
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.alignment: Qt.AlignTop
                header: Kirigami.Heading {
                    text: i18nc("@title:group", "Environment check")
                    level: 4
                    padding: Kirigami.Units.smallSpacing
                }
                contentItem: ColumnLayout {
                    Repeater {
                        model: DeviceController.envTexts
                        delegate: RowLayout {
                            id: checkRow
                            required property int index
                            required property string modelData
                            readonly property int level: DeviceController.envLevels[index]
                            Kirigami.Icon {
                                implicitWidth: Kirigami.Units.iconSizes.small
                                implicitHeight: Kirigami.Units.iconSizes.small
                                source: checkRow.level === 0 ? "emblem-ok-symbolic"
                                    : checkRow.level === 1 ? "emblem-warning" : "emblem-error"
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: checkRow.modelData
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                }
            }

            Kirigami.Card {
                Layout.fillWidth: true
                Layout.columnSpan: parent.columns
                header: Kirigami.Heading {
                    text: i18nc("@title:group", "USB log")
                    level: 4
                    padding: Kirigami.Units.smallSpacing
                }
                contentItem: ListView {
                    id: logList
                    implicitHeight: Kirigami.Units.gridUnit * 8
                    clip: true
                    model: usbLog
                    onCountChanged: Qt.callLater(positionViewAtEnd)
                    delegate: Controls.Label {
                        required property string time
                        required property real wallMs
                        required property string kind
                        required property string preview
                        width: ListView.view.width
                        font.family: "monospace"
                        elide: Text.ElideRight
                        text: (usbLog.timeOfDay && wallMs >= 0 ? Qt.formatTime(new Date(wallMs), "hh:mm:ss.zzz") : time)
                            + "  " + kind.padEnd(14, " ") + preview
                    }
                }
            }
        }
    }

    footer: Controls.Label {
        padding: Kirigami.Units.smallSpacing
        color: Kirigami.Theme.disabledTextColor
        font: Kirigami.Theme.smallFont
        text: DeviceController.connectedSinceMs >= 0
            ? i18ncp("@info", "Plugged in %2 · %1 error", "Plugged in %2 · %1 errors", DeviceController.errorCount,
                Qt.formatTime(new Date(DeviceController.connectedSinceMs), "hh:mm:ss"))
            : i18nc("@info", "Not connected")
    }
}
