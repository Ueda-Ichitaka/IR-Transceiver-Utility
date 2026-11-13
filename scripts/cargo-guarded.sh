#!/usr/bin/env bash
# cargo-guarded.sh - runs cargo inside a memory-capped systemd user scope.
#
# Exists so builds and tests on a shared machine cannot exhaust RAM: it checks
# available memory before starting, caps the cargo process tree with
# MemoryMax (no swap), limits parallel jobs, and reports an OOM kill clearly.
#
# Usage: scripts/cargo-guarded.sh <cargo args...>
# Env:   GUARD_MEM_MAX (default 6G), GUARD_MIN_AVAIL_MIB (default 4096),
#        GUARD_JOBS (default 4)
set -euo pipefail

mem_max="${GUARD_MEM_MAX:-6G}"
min_avail_mib="${GUARD_MIN_AVAIL_MIB:-4096}"
jobs="${GUARD_JOBS:-4}"

avail_kib=$(awk '/^MemAvailable:/ {print $2}' /proc/meminfo)
avail_mib=$((avail_kib / 1024))
if (( avail_mib < min_avail_mib )); then
    echo "cargo-guarded: only ${avail_mib} MiB available (need ${min_avail_mib} MiB), not starting" >&2
    exit 75
fi

set +e
CARGO_BUILD_JOBS="$jobs" systemd-run --user --scope -q \
    -p MemoryMax="$mem_max" -p MemorySwapMax=0 \
    cargo "$@"
status=$?
set -e

if (( status == 137 )); then
    echo "cargo-guarded: cargo was killed (exit 137), most likely by the ${mem_max} memory cap" >&2
fi
exit "$status"
