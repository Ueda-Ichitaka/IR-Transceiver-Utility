# CMakeLists.txt - builds and installs the IR Transceiver Utility.
#
# Wraps Cargo (design decision 1): the `ir-transceiver` binary is built by
# `cargo build --release --locked` into this build directory, then installed
# with the desktop file, the AppStream metainfo and the icon to the paths
# from ECM's KDEInstallDirs, and the udev rule that gives the logged-in user
# access to the dongle to udev's rules directory. Cargo keeps control of the Rust and cxx-qt
# build; CMake only drives it and installs.
cmake_minimum_required(VERSION 3.25)

project(ir-transceiver VERSION 0.1.0 LANGUAGES CXX)

set(APP_ID io.github.ueda.irtransceiver)

find_package(ECM 6.0 REQUIRED NO_MODULE)
list(APPEND CMAKE_MODULE_PATH ${ECM_MODULE_PATH} ${ECM_KDE_MODULE_DIR})
include(KDEInstallDirs)

find_program(CARGO cargo REQUIRED)

# udev reads rules only from its own directory, which pkg-config reports.
find_package(PkgConfig REQUIRED)
pkg_get_variable(UDEV_DIR udev udevdir)
if(NOT UDEV_DIR)
    set(UDEV_DIR /usr/lib/udev)
endif()
set(UDEV_RULES_DIR ${UDEV_DIR}/rules.d CACHE PATH "Where the udev rule is installed")

set(CARGO_TARGET_DIR ${CMAKE_BINARY_DIR}/cargo)
set(APP_BINARY ${CARGO_TARGET_DIR}/release/ir-transceiver)

add_custom_target(ir-transceiver ALL
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${CARGO_TARGET_DIR}
            ${CARGO} build --release --locked --package ir-app --bin ir-transceiver
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    USES_TERMINAL
    COMMENT "Building ir-transceiver with Cargo")

install(PROGRAMS ${APP_BINARY} DESTINATION ${KDE_INSTALL_BINDIR})
install(FILES data/${APP_ID}.desktop DESTINATION ${KDE_INSTALL_APPDIR})
install(FILES data/${APP_ID}.metainfo.xml DESTINATION ${KDE_INSTALL_METAINFODIR})
install(FILES data/${APP_ID}.svg DESTINATION ${KDE_INSTALL_ICONDIR}/hicolor/scalable/apps)
install(FILES data/70-ir-transceiver.rules DESTINATION ${UDEV_RULES_DIR})
