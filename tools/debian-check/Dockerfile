# Dockerfile - Debian stable (trixie) image for checking the build there.
#
# Exists for design decision 5 (support Debian stable): installs Debian's
# own Rust 1.85, Qt 6.8 and KDE Frameworks 6.13 development packages and the
# QML modules the app loads, so tools/debian-check/run.sh can build and test
# the workspace with exactly those versions. Package names were checked on
# packages.debian.org for trixie.
FROM debian:trixie

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      build-essential ca-certificates cargo clang cmake extra-cmake-modules git libclang-dev \
      libkf6coreaddons-dev libkf6crash-dev libkf6i18n-dev libkirigami-dev pkg-config \
      qml6-module-org-kde-desktop qml6-module-org-kde-kirigami qml6-module-qtqml \
      qml6-module-qtqml-workerscript qml6-module-qtquick qml6-module-qtquick-controls \
      qml6-module-qtquick-dialogs qml6-module-qtquick-layouts qml6-module-qtquick-templates \
      qml6-module-qtquick-window qt6-base-dev qt6-base-private-dev qt6-declarative-dev \
      qt6-qpa-plugins rustc \
 && rm -rf /var/lib/apt/lists/*

# A one-off build container: nothing to probe.
HEALTHCHECK NONE

WORKDIR /work
