#!/usr/bin/env bash
# run.sh - builds and tests the workspace on Debian stable in a container.
#
# Exists to check design decision 5 without a Debian machine: builds the
# image from tools/debian-check/Dockerfile, copies the repository (read-only
# mount, without target/) into the container, then runs the tests (including
# the offscreen GUI smoke test) and the CMake build with Debian's toolchain.
# The container is capped at 6 GiB without swap and 4 build jobs; the script
# refuses to start below 8 GiB available memory.
#
# Usage: tools/debian-check/run.sh   (needs access to the Docker daemon)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
image=ir-transceiver-debian-check

avail_mib=$(( $(awk '/^MemAvailable:/ {print $2}' /proc/meminfo) / 1024 ))
if (( avail_mib < 8192 )); then
    echo "debian-check: only ${avail_mib} MiB available (need 8192 MiB), not starting" >&2
    exit 75
fi

docker build -t "$image" "$here"

docker run --rm --memory=6g --memory-swap=6g --cpus=4 \
    -e CARGO_BUILD_JOBS=4 -e QT_QPA_PLATFORM=offscreen -e QT_FORCE_STDERR_LOGGING=1 \
    -v "$repo:/src:ro" "$image" bash -euo pipefail -c '
        tar -C /src --exclude=./target --exclude=./build --exclude="./build-*" -cf - . | tar -C /work -xf -
        rustc --version
        cargo test --workspace --locked
        cmake -S . -B build -DCMAKE_INSTALL_PREFIX=/usr
        cmake --build build
        DESTDIR=/tmp/stage cmake --install build
        echo "debian-check: all checks passed"
    '
