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
