// CaptureList.qml - one capture tab as a compact list (as in the mockup).
//
// One line per row: number, time (time of day or seconds since connecting,
// per the settings), a kind chip coloured by its tone and a one-line byte
// preview. Clicking a row selects it for the detail panel. The list follows
// new rows unless the user scrolled up or the view is paused.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Controls.ScrollView {
    id: scroll

    property CaptureModel model
    property bool paused: false

    readonly property bool smokeTest: Qt.application.arguments.indexOf("--smoke-test") >= 0

    // Chip colour per tone: 0 infrared, 1 control, 2 problem.
    function toneColor(tone) {
        return tone === 0 ? Kirigami.Theme.positiveTextColor
            : tone === 2 ? Kirigami.Theme.negativeTextColor
            : Kirigami.Theme.disabledTextColor;
    }

    ListView {
        id: list

        model: scroll.model
        clip: true
        reuseItems: true
        currentIndex: scroll.model ? scroll.model.selected : -1
        property bool follow: true
        onMovementEnded: follow = atYEnd || contentHeight <= height
        onCountChanged: if (follow && !scroll.paused) Qt.callLater(positionViewAtEnd)

        headerPositioning: ListView.OverlayHeader
        header: Controls.Control {
            z: 2
            width: ListView.view.width
            padding: Kirigami.Units.smallSpacing
            background: Rectangle {
                color: Kirigami.Theme.backgroundColor
            }
            contentItem: RowLayout {
                Controls.Label {
                    text: "#"
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 2.5
                    font.bold: true
                }
                Controls.Label {
                    text: i18nc("@title:column", "Time")
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    font.bold: true
                }
                Controls.Label {
                    text: i18nc("@title:column", "Kind")
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    font.bold: true
                }
                Controls.Label {
                    text: i18nc("@title:column", "Bytes")
                    Layout.fillWidth: true
                    font.bold: true
                }
            }
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: list.count === 0
            text: i18nc("@info:placeholder", "Nothing captured yet")
            explanation: i18nc("@info:placeholder", "Use Start capture or Learn once and press a button on a remote.")
        }

        delegate: Controls.ItemDelegate {
            id: row
            required property int index
            required property int number
            required property string time
            required property real wallMs
            required property string kind
            required property int tone
            required property string preview

            width: ListView.view.width
            highlighted: ListView.isCurrentItem
            onClicked: scroll.model.select(index)
            Component.onCompleted: {
                if (scroll.smokeTest) {
                    console.info("ir-transceiver: row " + number + " | " + kind + " | " + preview.substring(0, 11));
                }
            }

            contentItem: RowLayout {
                Controls.Label {
                    text: row.number
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 2.5
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Label {
                    text: scroll.model.timeOfDay && row.wallMs >= 0
                        ? Qt.formatTime(new Date(row.wallMs), "hh:mm:ss.zzz") : row.time
                    font.family: "monospace"
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                }
                Item {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    implicitHeight: chip.implicitHeight
                    Rectangle {
                        anchors.fill: chip
                        radius: height / 2
                        color: scroll.toneColor(row.tone)
                        opacity: 0.18
                    }
                    Controls.Label {
                        id: chip
                        leftPadding: Kirigami.Units.smallSpacing * 2
                        rightPadding: Kirigami.Units.smallSpacing * 2
                        text: row.kind
                        color: row.highlighted ? Kirigami.Theme.highlightedTextColor : scroll.toneColor(row.tone)
                        font: Kirigami.Theme.smallFont
                    }
                }
                Controls.Label {
                    text: row.preview
                    font.family: "monospace"
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
            }
        }
    }
}
