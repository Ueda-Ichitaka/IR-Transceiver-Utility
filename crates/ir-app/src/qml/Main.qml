// Main.qml - the main window of the IR Transceiver Utility.
//
// Kirigami application window with the sidebar (design decision 24) leading
// to the pages, and the connection status always shown in the sidebar
// footer. On start it connects to the first dongle found (design decision 9).
// With --smoke-test it does not connect: it opens every page, logs a marker
// per page and one when done, and quits.
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import io.github.ueda.irtransceiver

Kirigami.ApplicationWindow {
    id: root

    title: i18nc("@title:window", "IR Transceiver")
    width: Kirigami.Units.gridUnit * 60
    height: Kirigami.Units.gridUnit * 40

    readonly property bool smokeTest: Qt.application.arguments.indexOf("--smoke-test") >= 0
    readonly property var pages: [
        { name: "Capture", page: capturePage },
        { name: "Send", page: sendPage },
        { name: "Device", page: devicePage }
    ]

    // Pages are created once and kept, so a running capture keeps its lists
    // while another page is shown.
    function showPage(page) {
        if (pageStack.currentItem !== page) {
            pageStack.replace(page);
        }
    }

    AppInfo {
        id: appInfo
    }

    CapturePage {
        id: capturePage
        visible: false
    }

    SendPage {
        id: sendPage
        visible: false
    }

    DevicePage {
        id: devicePage
        visible: false
    }

    globalDrawer: Kirigami.GlobalDrawer {
        isMenu: false
        modal: false
        actions: [
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Capture")
                icon.name: "media-record"
                onTriggered: root.showPage(capturePage)
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Send")
                icon.name: "document-send"
                onTriggered: root.showPage(sendPage)
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Library")
                icon.name: "view-list-details"
                enabled: false
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Device")
                icon.name: "drive-removable-media-usb"
                onTriggered: root.showPage(devicePage)
            }
        ]
        footer: ColumnLayout {
            spacing: 0
            Controls.Label {
                Layout.fillWidth: true
                padding: Kirigami.Units.smallSpacing
                text: DeviceController.stateLabel
                font.bold: true
                elide: Text.ElideRight
            }
            Controls.Label {
                Layout.fillWidth: true
                padding: Kirigami.Units.smallSpacing
                text: i18nc("@info", "Version %1", appInfo.version)
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    pageStack.initialPage: capturePage

    // Smoke test: shows one page per step so each is laid out in the scene.
    Timer {
        id: smokeSteps
        property int next: 0
        interval: 50
        repeat: true
        onTriggered: {
            if (next > 0) {
                console.info("ir-transceiver: page " + root.pages[next - 1].name + " ready");
            }
            if (next === root.pages.length) {
                stop();
                console.info("ir-transceiver: main window ready");
                Qt.exit(0);
                return;
            }
            root.showPage(root.pages[next].page);
            next += 1;
        }
    }

    Component.onCompleted: {
        if (smokeTest) {
            smokeSteps.start();
        } else {
            DeviceController.connectDevice();
        }
    }
}
