// CaptureList.qml - one capture tab: a scrolling list of rows.
//
// Shows time, kind, length and the data column of a CaptureModel; with
// showSummary also the protocol description and the Save to library and
// Send again buttons (Signals tab). Every row can copy its data column.
// Follows the end of the list while the user has not scrolled up.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Controls.ScrollView {
    id: scroll

    property alias model: list.model
    property bool showSummary: true

    // QML has no clipboard API; a hidden text field copies through Qt.
    function copy(text) {
        clipboardHelper.text = text;
        clipboardHelper.selectAll();
        clipboardHelper.copy();
    }

    Controls.TextField {
        id: clipboardHelper
        visible: false
    }

    ListView {
        id: list

        clip: true
        reuseItems: true
        readonly property bool atEnd: atYEnd || contentHeight <= height
        property bool follow: true
        onMovementEnded: follow = atEnd
        onCountChanged: if (follow) Qt.callLater(positionViewAtEnd)

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: list.count === 0
            text: i18nc("@info:placeholder", "Nothing captured yet")
            explanation: i18nc("@info:placeholder", "Use Learn once or Start capture and press a button on a remote.")
        }

        delegate: Controls.ItemDelegate {
            id: row
            required property int index
            required property string time
            required property string kind
            required property string summary
            required property string bytes
            required property string length

            width: ListView.view.width
            contentItem: ColumnLayout {
                spacing: 0
                RowLayout {
                    Controls.Label {
                        text: row.time
                        font.family: "monospace"
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 4
                    }
                    Controls.Label {
                        text: row.kind
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 4
                    }
                    Controls.Label {
                        text: i18ncp("@info", "%1 byte", "%1 bytes", row.length)
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                    }
                    Controls.Label {
                        visible: scroll.showSummary
                        text: row.summary
                        font.bold: true
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                    }
                    Item {
                        visible: !scroll.showSummary
                        Layout.fillWidth: true
                    }
                    Controls.ToolButton {
                        visible: scroll.showSummary
                        icon.name: "document-save"
                        text: i18nc("@action:button", "Save to library")
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: LibraryModel.saveSignal(row.index)
                    }
                    Controls.ToolButton {
                        visible: scroll.showSummary
                        icon.name: "document-send"
                        text: i18nc("@action:button", "Send again")
                        display: Controls.AbstractButton.IconOnly
                        enabled: DeviceController.ready
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: DeviceController.sendSignal(row.index)
                    }
                    Controls.ToolButton {
                        icon.name: "edit-copy"
                        text: i18nc("@action:button", "Copy")
                        display: Controls.AbstractButton.IconOnly
                        Controls.ToolTip.text: text
                        Controls.ToolTip.visible: hovered
                        onClicked: scroll.copy(row.bytes)
                    }
                }
                Controls.Label {
                    text: row.bytes
                    font.family: "monospace"
                    wrapMode: Text.WrapAnywhere
                    Layout.fillWidth: true
                    textFormat: Text.PlainText
                }
            }
        }
    }
}
