//! ir-app - the IR Transceiver Utility GUI as a library.
//!
//! Exists so the cxx-qt bridges live in a library that both the
//! `ir-transceiver` binary and the tests link. [`run`] sets up the KDE
//! application (KI18n domain, KCrash, KAboutData, desktop file name, Breeze
//! style), loads the Kirigami main window from the QML module (exiting with
//! an error when it cannot be loaded) and, when the window closes, stops the
//! session worker.
//! `--smoke-test` makes the window quit right after loading (used by
//! tests/smoke.rs).

pub mod app_info;
pub mod capture_model;
pub mod device_controller;
pub mod file_actions;
pub mod library_model;
pub mod settings_controller;
pub mod send_controller;
pub mod state;

use cxx_kde_frameworks::kcoreaddons::{KAboutData, License};
use cxx_kde_frameworks::kcrash::KCrash;
use cxx_kde_frameworks::ki18n::{self, KLocalizedString, i18nc};
use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QByteArray, QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};
use cxx_qt_lib_extras::QApplication;

const APP_ID: &str = "io.github.ueda.irtransceiver";
const MAIN_QML: &str = "qrc:/qt/qml/io/github/ueda/irtransceiver/src/qml/Main.qml";

/// Runs the application until the main window closes.
pub fn run() {
    let mut app = QApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    KLocalizedString::set_application_domain(&QByteArray::from("ir-transceiver"));
    KCrash::initialize();
    QGuiApplication::set_desktop_file_name(&QString::from(APP_ID));
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        QQuickStyle::set_style(&QString::from("org.kde.desktop"));
    }

    let about = KAboutData::from(
        QString::from("ir-transceiver"),
        i18nc("@title", "IR Transceiver"),
        QString::from(env!("CARGO_PKG_VERSION")),
        i18nc("@info", "Learn, capture and send infrared codes with an ELK Smart USB dongle"),
        License::MIT,
    );
    KAboutData::set_application_data(&about);

    if let Some(mut engine) = engine.as_mut() {
        ki18n::setup_localized_context(engine.as_mut().upcast_pin());
        // Without its main window the app would keep running invisibly.
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| {
                eprintln!("ir-transceiver: loading {url} failed");
                std::process::exit(1);
            })
            .release();
        engine.load(&QUrl::from(MAIN_QML));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
    state::shutdown();
}
