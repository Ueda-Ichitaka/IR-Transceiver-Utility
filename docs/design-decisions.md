# Design decisions

Answers to the open design questions (2026-11-13), with notes on how each is
applied. Question 25 is still open.

## Repository and build

1. **Build system** - CMake + ECM wrapping Cargo.
2. **KDE Frameworks bindings** - `cxx-kde-frameworks` (git dependency, master uses cxx-qt 0.10).
3. **App identity** - `io.github.ueda.irtransceiver`.
4. **License** - MIT.
5. **Minimum versions** - also support Debian stable (trixie): rustc 1.85.1, Qt 6.8.2, KDE Frameworks / Kirigami 6.13.
   - Applied as `rust-version = "1.85"` for the workspace. Cargo's MSRV-aware resolver then picks `cxx` 1.0.198 (the newest that supports 1.85; 1.0.199+ need 1.88).
6. **Distribution** - `cmake --install` plus an AUR PKGBUILD.

## Device scope

7. **Huffman protocol** - left out. A device that identifies as Huffman can learn, but transmitting reports "not supported".
8. **Other ELK models** - supported: 0132, 0195, 0184, 0134.
9. **More than one dongle** - supported; the first one found is selected automatically.

## Sending

10. **Send payload types** - all four: learned code, pulse timings, NEC address/command, raw USB bytes.
    - Finding: elksmart-comm's `-n` option sends NEC bits MSB first, but standard NEC (remotes, Linux, IRremote) is LSB first. `elk-proto` supports both; the app and `elk-probe transmit --nec` use the standard order, so `--nec 0059` replays the button learned in the fixture.
11. **Raw USB bytes** - always visible.
12. **Repeat signals in learned codes** - both modes (first frame plus repeat count, or exactly as learned), configurable.
13. **Carrier frequency** - a global default, which each code can override.

## Capture

14. **Stopping capture while waiting for a signal** - following the recommendation: show a banner that the dongle is waiting for an IR signal and asks you to press any remote button or replug it. The session leaves this state on its own when the dongle answers, and identifies it again.
15. **Capture buffer limit** - configurable, default 4096 MiB; capture stops when full.
    - Memory guard: capture also stops when MemAvailable falls below a reserve, so a large limit cannot exhaust RAM on a shared machine.
16. **Captures after closing the app** - kept only in memory until exported.
17. **Export formats** - JSON, CSV, raw .bin, LIRC raw config, Pronto hex, Flipper .ir.
18. **Import formats** - the same list.

## Encodings and decoding

19. **Encoding switch** - a separate switch per view; uppercase hex; 16 bytes per line.
20. **Pulse timings in the encoding switch** - configurable.
21. **Remote protocols to recognise** - NEC, NEC-extended, RC5, RC6, Sony SIRC, Samsung, Kaseikyo/Panasonic, plus the protocols used by IR LED strip remotes.

## Library

22. **Library file** - TOML.
23. **Organisation** - a list with tags.

## UI and integration

24. **Navigation** - sidebar drawer.
25. **Outside the window** (tray icon, global shortcuts, D-Bus, CLI) - deferred; needs more information and brainstorming.

## Follow-up decisions (2026-11-14)

- **GUI blocker (cxx-kde-frameworks / cmake-package):** option A. A patched
  copy of cmake-package 0.1.15 lives in `third_party/cmake-package` and is
  used through `[patch.crates-io]`; details in `third_party/PATCHED.md`.
- **Settings store:** a TOML file,
  `$XDG_CONFIG_HOME/io.github.ueda.irtransceiver/settings.toml`, handled in
  Rust like the library file (atomic save, damaged file moved aside).
- **Upstream report:** Claude drafts the issue text in `docs/upstream/`; the
  user reviews and posts it.
- **Debian stable check:** build in a `debian:trixie` container (Podman or
  Docker) under the same memory cap.

## Export formats (2026-11-14)

- **Capture JSON:** one file with both tabs: `signals` (time, complete,
  summary, data as hex, pulses) and `transfers` (time, direction, data as
  hex).
- **Capture CSV:** the open tab only, with a header line and RFC 4180
  quoting.
- **Raw .bin:** all bytes received from the dongle (IN transfers) in order.
- **LIRC, Pronto, Flipper:** import into and export from the library, and
  export directly from the Signals tab.

## GUI (2026-11-14)

- The capture page has two tabs, received signals and USB transfers, each
  with its own encoding switch.
- GUI logic lives in the Qt-free crate `ir-view` and is unit tested; the
  cxx-qt objects only forward. An offscreen smoke test opens every page.
- Order: device and capture, then send, then library; CMake/ECM and the
  PKGBUILD at the end.

## Build and packaging (2026-11-14)

- **CMake and Cargo:** `CMakeLists.txt` runs `cargo build --release --locked`
  through a custom target and installs with ECM's KDEInstallDirs (no
  Corrosion).
- **Icon:** an own SVG, `data/io.github.ueda.irtransceiver.svg`.
- **AUR:** `packaging/aur/PKGBUILD` is a `-git` package, since there is no
  tagged release yet.

## Open issues

- The Debian check (`tools/debian-check/run.sh`) is written but has not run
  yet: Docker on the development machine cannot start until it is rebooted
  into the installed kernel.
- Question 25 (tray, shortcuts, D-Bus, CLI) is deferred.
