// CapturePage.qml - live capture, laid out as in the mockup.
//
// Header actions: start/stop capture (one toggle), learn once, pause view,
// clear, import (replace or append) and export. Below: the Signals and
// Transfers tabs with each tab's encoding switch as segmented buttons, a
// split view of the compact row list and the detail panel of the selected
// row (it moves under the list on narrow windows), and the status bar.
// Pause view stops the lists from following new rows while capture goes on.
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
    readonly property CaptureModel currentModel: tabs.currentIndex === 0 ? signalModel : transferModel
    property bool paused: false

    // Loads an exported capture, replacing the current one or appending.
    function importCapture(url, replace) {
        if (FileActions.importCapture(url, replace)) {
            signalModel.reformat();
            transferModel.reformat();
            DeviceController.refreshCapture();
            return true;
        }
        return false;
    }

    // Shows the Signals (0) or Transfers (1) tab.
    function showTab(index) {
        tabs.currentIndex = index;
    }

    // Selects a row of the current tab for the detail panel.
    function selectRow(row) {
        currentModel.select(row);
    }

    actions: [
        Kirigami.Action {
            text: DeviceController.canStopCapture ? i18nc("@action:button", "Stop capture") : i18nc("@action:button", "Start capture")
            icon.name: DeviceController.canStopCapture ? "media-playback-stop" : "media-record"
            enabled: DeviceController.canStopCapture || DeviceController.ready
            onTriggered: DeviceController.canStopCapture ? DeviceController.stopCapture() : DeviceController.startCapture()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Learn once")
            icon.name: "media-playback-start"
            enabled: DeviceController.ready
            onTriggered: DeviceController.learnOnce(page.learnWaitSeconds)
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Pause view")
            icon.name: "media-playback-pause"
            checkable: true
            checked: page.paused
            onToggled: {
                page.paused = checked;
                if (!checked) {
                    signalModel.sync();
                    transferModel.sync();
                }
            }
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Clear")
            icon.name: "edit-clear-history"
            enabled: DeviceController.signalCount > 0 || DeviceController.transferCount > 0
            onTriggered: DeviceController.clearCapture()
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Import")
            icon.name: "document-import"
            enabled: !DeviceController.canStopCapture

            Kirigami.Action {
                text: i18nc("@action:inmenu", "Replace current capture…")
                onTriggered: importDialog.ask(true)
            }
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Append to current capture…")
                onTriggered: importDialog.ask(false)
            }
        },
        Kirigami.Action {
            text: i18nc("@action:button", "Export…")
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
        }
    ]

    Connections {
        target: DeviceController
        function onCaptureChanged() {
            if (!page.paused) {
                signalModel.sync();
                transferModel.sync();
            }
        }
    }

    Connections {
        target: SettingsController
        function onSaved() {
            signalModel.reformat();
            transferModel.reformat();
            DeviceController.refreshCapture();
        }
    }

    // Keeps the capture duration in the status bar current.
    Timer {
        interval: 1000
        repeat: true
        running: DeviceController.canStopCapture
        onTriggered: DeviceController.refreshCapture()
    }

    Dialogs.FileDialog {
        id: importDialog
        property bool replace: true
        function ask(replace) {
            importDialog.replace = replace;
            open();
        }
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [i18nc("@item:inlistbox", "Captures (*.json *.csv *.bin)"), i18nc("@item:inlistbox", "All files (*)")]
        onAccepted: page.importCapture(selectedFile, replace)
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

            // Encoding switch of the current tab, as segmented buttons.
            Row {
                spacing: 0
                Repeater {
                    model: page.currentModel.displayOptions
                    delegate: Controls.Button {
                        required property int index
                        required property string modelData
                        text: modelData
                        checkable: true
                        checked: page.currentModel.displayIndex === index
                        onClicked: page.currentModel.displayIndex = index
                    }
                }
            }
        }

        Controls.SplitView {
            id: split
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: page.width < Kirigami.Units.gridUnit * 40 ? Qt.Vertical : Qt.Horizontal

            StackLayout {
                Controls.SplitView.fillWidth: true
                Controls.SplitView.fillHeight: true
                Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 16
                Controls.SplitView.minimumHeight: Kirigami.Units.gridUnit * 8
                currentIndex: tabs.currentIndex

                CaptureList {
                    model: signalModel
                    paused: page.paused
                }

                CaptureList {
                    model: transferModel
                    paused: page.paused
                }
            }

            CaptureDetail {
                model: page.currentModel
                signalsTab: tabs.currentIndex === 0
                Controls.SplitView.preferredWidth: Kirigami.Units.gridUnit * 22
                Controls.SplitView.preferredHeight: Kirigami.Units.gridUnit * 14
                Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 14
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
        }

        // Status bar.
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: DeviceController.captureStatus
                delegate: Controls.Label {
                    required property int index
                    required property string modelData
                    text: modelData
                    color: index === 4 && DeviceController.captureFull ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.disabledTextColor
                    font: Kirigami.Theme.smallFont
                }
            }
            Item {
                Layout.fillWidth: true
            }
        }
    }
}
