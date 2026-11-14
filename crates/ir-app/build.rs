//! Build script of ir-app: compiles the cxx-qt bridges and registers the QML
//! module `io.github.ueda.irtransceiver` with its QML files.

use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("io.github.ueda.irtransceiver").qml_files(["src/qml/Main.qml"]),
    )
    .files(["src/app_info.rs"])
    .build();
}
