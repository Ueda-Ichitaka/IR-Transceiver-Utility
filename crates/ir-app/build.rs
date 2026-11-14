//! Build script of ir-app: compiles the cxx-qt bridges and registers the QML
//! module `io.github.ueda.irtransceiver` with its QML files.

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.ueda.irtransceiver").qml_files([
            "src/qml/Main.qml",
            "src/qml/CapturePage.qml",
            "src/qml/CaptureList.qml",
            "src/qml/DevicePage.qml",
        ]),
    )
    .files(["src/app_info.rs", "src/capture_model.rs", "src/device_controller.rs"])
    .build();
}
