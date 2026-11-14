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

## Open issues

### cxx-kde-frameworks does not build with current KDE Frameworks (blocks the GUI)

The app skeleton is on branch `wip/app-skeleton`. Building it fails inside
cxx-kde-frameworks (master 5c51af42, the only branch):

```
error: renaming of the library `Qt6` was specified, however this crate contains no `#[link(...)]` attributes referencing this library
```

Cause: its build script asks the `cmake-package` crate (0.1.15) for the link
libraries of `KF6::I18nQml`. These include the CMake target
`Qt6::QmlIntegration` (header-only), which cmake-package prints unchanged as
`cargo:rustc-link-lib=dylib=Qt6::QmlIntegration`; rustc reads the colon as
renaming syntax. cmake-package 0.3.0 has the same code. Seen with Qt 6.12 /
KF 6.31 (Arch); not verified on Debian (KF 6.13).

Options (a decision is needed):

- **A. Patch cmake-package locally** - a copy under `third_party/` with
  a `[patch.crates-io]` entry that skips unresolved `Namespace::Target`
  names; report the bug upstream and drop the patch once it is fixed.
  Smallest change.
- **B. Fork cxx-kde-frameworks** - point the git dependency at a fork with the
  fix in its build script.
- **C. Go without cxx-kde-frameworks for now** - plain Qt (no KAboutData,
  KCrash, KI18n) until upstream is fixed. Contradicts decision 2.

Recommendation: A, plus an upstream report to cmake-package and
cxx-kde-frameworks.

Option A was tried out (2026-11-14) and works on this machine: with
`docs/patches/cmake-package-0.1.15-skip-cmake-targets.patch` (on branch
`wip/app-skeleton`) applied through `[patch.crates-io]`, the app builds,
shows the Kirigami window with KAboutData, KCrash and KI18n from
cxx-kde-frameworks, and the offscreen smoke test passes. The branch holds
that working skeleton without the patch applied.
