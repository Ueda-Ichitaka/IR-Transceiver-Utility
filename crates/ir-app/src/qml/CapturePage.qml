// CapturePage.qml - live capture with a Signals tab and a Transfers tab.
//
// Learn once / Start / Stop / Clear act on the dongle through the
// DeviceController singleton; Export writes the capture through FileActions. Each tab is a ListView over its own
// CaptureModel with its own encoding switch; rows are appended as the
// session reports them (captureChanged -> sync).
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.Page {
    id: page

    title: i18nc("@title", "Capture")
    padding: 0

    readonly property int learnWaitSeconds: 30

    // Shows the Signals (0) or Transfers (1) tab.
    function showTab(index) {
        tabs.currentIndex = index;
    }

    actions: [
        Kirigami.Action {
            text: i18nc("@action:button", "Learn once")
            icon.name: "media-record"
            enabled: DeviceController.ready
            onTriggered: DeviceController.learnOnce(page.learnWaitSeconds)
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Start capture")
            icon.name: "media-playback-start"
            enabled: DeviceController.ready
            onTriggered: DeviceController.startCapture()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Stop")
            icon.name: "media-playback-stop"
            enabled: DeviceController.canStopCapture
            onTriggered: DeviceController.stopCapture()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Export")
            icon.name: "document-export"
            enabled: DeviceController.signalCount > 0 || DeviceController.transferCount > 0

            Kirigami.Action {
                text: i18nc("@action:inmenu", "JSON (signals and transfers)…")
                onTriggered: exportDialog.ask("json", "json", i18nc("@item:inlistbox", "JSON files (*.json)"))
            }
            Kirigami.Action {
                text: i18nc("@action:inmenu", "CSV (current tab)…")
                onTriggered: exportDialog.ask(tabs.currentIndex === 0 ? "signals-csv" : "transfers-csv", "csv",
                    i18nc("@item:inlistbox", "CSV files (*.csv)"))
            }
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Received bytes (.bin)…")
                onTriggered: exportDialog.ask("bin", "bin", i18nc("@item:inlistbox", "Binary files (*.bin)"))
            }
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Signals as LIRC config…")
                onTriggered: exportDialog.ask("lirc", "conf", i18nc("@item:inlistbox", "LIRC configs (*.conf)"))
            }
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Signals as Flipper file…")
                onTriggered: exportDialog.ask("flipper", "ir", i18nc("@item:inlistbox", "Flipper IR files (*.ir)"))
            }
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Clear")
            icon.name: "edit-clear-history"
            enabled: DeviceController.signalCount > 0 || DeviceController.transferCount > 0
            onTriggered: DeviceController.clearCapture()
        }
    ]

    Connections {
        target: DeviceController
        function onCaptureChanged() {
            signalModel.sync();
            transferModel.sync();
        }
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
        onAccepted: FileActions.exportCapture(selectedFile, format)
    }

    Connections {
        target: SettingsController
        function onSaved() {
            signalModel.reformat();
            transferModel.reformat();
            DeviceController.refreshCapture();
        }
    }

    CaptureModel {
        id: signalModel
        tab: 0
    }

    CaptureModel {
        id: transferModel
        tab: 1
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: DeviceController.banner !== ""
            type: Kirigami.MessageType.Information
            text: DeviceController.banner
        }

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
            visible: DeviceController.lastError !== ""
            type: Kirigami.MessageType.Warning
            text: DeviceController.lastError
            actions: Kirigami.Action {
                text: i18nc("@action:button", "Dismiss")
                icon.name: "dialog-close"
                onTriggered: DeviceController.clearError()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing

            Controls.TabBar {
                id: tabs
                Layout.fillWidth: true
                Controls.TabButton {
                    text: i18nc("@title:tab", "Signals (%1)", DeviceController.signalCount)
                }
                Controls.TabButton {
                    text: i18nc("@title:tab", "Transfers (%1)", DeviceController.transferCount)
                }
            }

            Controls.ComboBox {
                id: displaySwitch
                readonly property CaptureModel model_: tabs.currentIndex === 0 ? signalModel : transferModel
                model: model_.displayOptions
                currentIndex: model_.displayIndex
                onActivated: index => model_.displayIndex = index
            }

            Controls.Label {
                text: DeviceController.captureUsage
                color: DeviceController.captureFull ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.disabledTextColor
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: tabs.currentIndex

            CaptureList {
                model: signalModel
                showSummary: true
            }

            CaptureList {
                model: transferModel
                showSummary: false
            }
        }
    }
}
