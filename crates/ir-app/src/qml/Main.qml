// Main.qml - the main window of the IR Transceiver Utility.
//
// Kirigami application window with the sidebar (design decision 24) leading
// to the pages, and the connection status always shown in the sidebar
// footer. On start it connects to the first dongle found (design decision 9).
// With --smoke-test it does not connect: it opens every page, replays the
// recorded fixture into the capture page, logs markers and quits.
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
        { name: "Library", page: libraryPage },
        { name: "Device", page: devicePage },
        { name: "Settings", page: settingsPage }
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

    LibraryPage {
        id: libraryPage
        visible: false
    }

    DevicePage {
        id: devicePage
        visible: false
    }

    SettingsPage {
        id: settingsPage
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
                onTriggered: root.showPage(libraryPage)
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Device")
                icon.name: "drive-removable-media-usb"
                onTriggered: root.showPage(devicePage)
            },
            Kirigami.Action {
                text: i18nc("@action:inmenu", "Settings")
                icon.name: "settings-configure"
                onTriggered: root.showPage(settingsPage)
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
        property bool replayed: false
        property bool savedToLibrary: false
        property bool imported: false
        interval: 50
        repeat: true
        onTriggered: {
            if (next > 0) {
                console.info("ir-transceiver: page " + root.pages[next - 1].name + " ready");
            }
            if (next === root.pages.length) {
                // Last step: replay the fixture into the capture page and let
                // both tabs render their rows.
                if (!replayed) {
                    replayed = true;
                    root.showPage(capturePage);
                    DeviceController.replayFixture();
                    capturePage.showTab(1);
                    return;
                }
                // Then save the first signal and let the library render it.
                if (!savedToLibrary) {
                    savedToLibrary = true;
                    LibraryModel.saveSignal(0);
                    root.showPage(libraryPage);
                    return;
                }
                // Then import the file given by the test, replacing the capture.
                if (!imported) {
                    imported = true;
                    const arg = Qt.application.arguments.find(a => a.startsWith("--smoke-import="));
                    if (arg) {
                        root.showPage(capturePage);
                        capturePage.showTab(0);
                        capturePage.importCapture("file://" + arg.substring("--smoke-import=".length), true);
                        console.info("ir-transceiver: imported " + DeviceController.signalCount + " signals");
                    }
                    return;
                }
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
