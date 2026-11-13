#!/usr/bin/env bash
# check.sh - full verification of the Rust workspace before committing.
#
# Runs clippy (warnings are errors) and the tests on the current stable
# toolchain, then the tests on the minimum supported Rust version (Debian
# stable's rustc) when that toolchain is installed. All cargo runs go through
# cargo-guarded.sh, so they are memory-capped.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/.."
msrv=$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)

"$here/cargo-guarded.sh" clippy --workspace --all-targets -- -D warnings
"$here/cargo-guarded.sh" test --workspace
if rustup toolchain list | grep -q "^${msrv}-"; then
    "$here/cargo-guarded.sh" "+${msrv}" test --workspace
else
    echo "check.sh: Rust ${msrv} not installed, skipping MSRV tests (rustup toolchain install ${msrv} --profile minimal)" >&2
fi
echo "check.sh: all checks passed"
