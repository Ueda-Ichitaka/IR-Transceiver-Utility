// CaptureDetail.qml - the detail panel of the selected capture row.
//
// Shows the row's title, all its bytes in the tab's encoding (selectable),
// the pulse timing as a waveform (signals), facts such as pulse count,
// duration and protocol guess, and the actions Save to library, Send again,
// Copy and Copy as Pronto hex (the first two and the last for signals).
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Controls.Pane {
    id: panel

    property CaptureModel model
    property bool signalsTab: true
    property string notice: ""

    readonly property bool hasSelection: model !== null && model.selected >= 0
    readonly property bool smokeTest: Qt.application.arguments.indexOf("--smoke-test") >= 0

    Connections {
        target: panel.model
        function onSelectionChanged() {
            panel.notice = "";
        }
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 2
        visible: !panel.hasSelection
        text: i18nc("@info:placeholder", "Select a row to see its details")
    }

    ColumnLayout {
        anchors.fill: parent
        visible: panel.hasSelection

        Controls.Label {
            text: panel.model ? panel.model.detailTitle : ""
            font.bold: true
        }

        Controls.ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 6
            Controls.TextArea {
                id: bytesArea
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                font.family: "monospace"
                textFormat: TextEdit.PlainText
                text: panel.model ? panel.model.detailBytes : ""
            }
        }

        Controls.Label {
            visible: waveform.pulses.length > 0
            text: i18nc("@label", "Pulse timing (µs)")
            color: Kirigami.Theme.disabledTextColor
        }

        // Waveform: high for each "on" time, low for each "off" time.
        Canvas {
            id: waveform
            readonly property var pulses: panel.model ? panel.model.detailWaveform : []
            visible: pulses.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 3
            onPulsesChanged: requestPaint()
            onWidthChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                let total = 0;
                for (const us of pulses) {
                    total += us;
                }
                if (total <= 0 || width <= 0) {
                    return;
                }
                const high = 4;
                const low = height - 4;
                ctx.strokeStyle = Kirigami.Theme.highlightColor;
                ctx.lineWidth = 1.5;
                ctx.beginPath();
                ctx.moveTo(0, low);
                let x = 0;
                for (let i = 0; i < pulses.length; i++) {
                    const y = i % 2 === 0 ? high : low;
                    ctx.lineTo(x, y);
                    x += pulses[i] / total * width;
                    ctx.lineTo(x, y);
                }
                ctx.lineTo(x, low);
                ctx.stroke();
                if (panel.smokeTest) {
                    const values = panel.model.detailFactValues;
                    console.info("ir-transceiver: detail " + panel.model.detailTitle + " | " + values[values.length - 1]
                        + " | " + pulses.length / 2 + " pulses drawn");
                }
            }
        }

        GridLayout {
            columns: 2
            columnSpacing: Kirigami.Units.largeSpacing
            Repeater {
                model: panel.model ? panel.model.detailFactLabels : []
                delegate: Controls.Label {
                    required property int index
                    required property string modelData
                    Layout.row: index
                    Layout.column: 0
                    text: modelData
                    color: Kirigami.Theme.disabledTextColor
                }
            }
            Repeater {
                model: panel.model ? panel.model.detailFactValues : []
                delegate: Controls.Label {
                    required property int index
                    required property string modelData
                    Layout.row: index
                    Layout.column: 1
                    Layout.fillWidth: true
                    text: modelData
                    wrapMode: Text.Wrap
                }
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                visible: panel.signalsTab
                text: i18nc("@action:button", "Save to library")
                icon.name: "document-save"
                onClicked: {
                    if (LibraryModel.saveSignal(panel.model.selected) >= 0) {
                        panel.notice = i18nc("@info", "Saved to the library.");
                    }
                }
            }
            Controls.Button {
                visible: panel.signalsTab
                text: i18nc("@action:button", "Send again")
                icon.name: "document-send"
                enabled: DeviceController.ready
                onClicked: DeviceController.sendSignal(panel.model.selected)
            }
            Controls.Button {
                text: i18nc("@action:button", "Copy")
                icon.name: "edit-copy"
                onClicked: {
                    bytesArea.selectAll();
                    bytesArea.copy();
                    bytesArea.deselect();
                }
            }
            Controls.Button {
                visible: panel.signalsTab
                text: i18nc("@action:button", "Copy as Pronto")
                icon.name: "edit-copy"
                onClicked: {
                    const hex = FileActions.prontoForSignal(panel.model.selected);
                    if (hex !== "") {
                        clipboardHelper.text = hex;
                        clipboardHelper.selectAll();
                        clipboardHelper.copy();
                    }
                }
            }
        }

        Controls.Label {
            visible: panel.notice !== ""
            text: panel.notice
            color: Kirigami.Theme.positiveTextColor
        }

        // QML has no clipboard API; a hidden text field copies through Qt.
        Controls.TextField {
            id: clipboardHelper
            visible: false
        }

        Item {
            Layout.fillHeight: true
        }
    }
}
