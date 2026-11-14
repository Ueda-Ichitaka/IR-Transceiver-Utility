// Main.qml - the main window of the IR Transceiver Utility.
//
// Kirigami application window with the sidebar (design decision 24) leading
// to the Capture, Send, Library and Device pages. With --smoke-test it logs
// a marker and quits once loaded.
import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.ApplicationWindow {
    id: root

    title: i18nc("@title:window", "IR Transceiver")
    width: Kirigami.Units.gridUnit * 60
    height: Kirigami.Units.gridUnit * 40

    AppInfo {
        id: appInfo
    }

    globalDrawer: Kirigami.GlobalDrawer {
        isMenu: false
        modal: false
        actions: [
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Capture")
                icon.name: "media-record"
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Send")
                icon.name: "document-send"
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Library")
                icon.name: "view-list-details"
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Device")
                icon.name: "drive-removable-media-usb"
            }
        ]
        footer: Controls.Label {
            padding: Kirigami.Units.smallSpacing
            text: i18nc("@info", "Version %1", appInfo.version)
            color: Kirigami.Theme.disabledTextColor
        }
    }

    pageStack.initialPage: Kirigami.Page {
        title: i18nc("@title", "Capture")
    }

    Component.onCompleted: {
        if (Qt.application.arguments.indexOf("--smoke-test") >= 0) {
            console.info("ir-transceiver: main window ready");
            Qt.exit(0);
        }
    }
}
