#!/usr/bin/env bash
# generate.sh - rebuilds the golden vectors for elk-proto from elksmart-comm.c.
#
# Compiles tools/golden/golden.c against a usb-drivers checkout (the reference
# driver) and writes crates/elk-proto/tests/data/elksmart-comm-golden.txt.
# Usage: tools/golden/generate.sh <usb-drivers checkout> [<built usb-drivers dir>]
set -euo pipefail

src="${1:?path to usb-drivers checkout}"
build="${2:-$src/build}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$root/crates/elk-proto/tests/data/elksmart-comm-golden.txt"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cc -std=c99 -Wall -Wno-unused-function -I"$src" -I"$build" -I/usr/include/libusb-1.0 \
    "$here/golden.c" -o "$tmp/golden" -lusb-1.0
learned=$(grep -m1 '^# data:' "$root/fixtures/1794596116-learn-tv-power.log" | cut -c9- | tr -d ' ')
"$tmp/golden" "$learned" > "$out"
echo "wrote $out"
