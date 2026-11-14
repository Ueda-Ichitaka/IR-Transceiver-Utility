# IR-Transceiver-Utility

A KDE (Kirigami) desktop app, written in Rust, for ELK Smart USB infrared
blasters. It learns codes from remote controls, captures everything the dongle
receives, shows the data as hex, binary, decimal, Base64 or pulse timings,
recognises common remote protocols, keeps codes in a tagged library and sends
them again.

## Features

- **Capture**: learn one code or capture continuously. The Signals tab shows
  each received code with its protocol (NEC, NEC-extended, RC5/RC5X, RC6,
  Sony SIRC, Samsung32, Kaseikyo); the Transfers tab shows every USB transfer.
  Each tab has its own encoding switch.
- **Send**: learned codes, pulse timings, NEC address and command, or raw USB
  bytes. Carrier frequency and repeat mode fall back to global defaults.
- **Library**: named, tagged codes stored as TOML, with search and a tag
  filter.
- **Import / export**: LIRC `lircd.conf` (raw codes), Flipper Zero `.ir`,
  Pronto hex; captures as JSON, CSV or the raw received byte stream.

## Supported dongles

USB vendor `045c`, products `0132`, `0195` (EKX4S), `0184` (EKX5S-T) and
`0134` (EKX5S). Dongles that report the Huffman protocol can learn but not
send. The protocol follows the reference driver `elksmart-comm.c` by Přemysl
Eric Janouch.

## Device access

The app talks to the dongle directly over USB, so your user needs access to
it. Installing with CMake (or the package) adds
`/usr/lib/udev/rules.d/70-ir-transceiver.rules` for all supported dongles;
replug the dongle afterwards. When running from the source tree, add a rule
yourself, for example `/etc/udev/rules.d/70-elksmart.rules`:

```
SUBSYSTEM=="usb", ATTR{idVendor}=="045c", ATTR{idProduct}=="0132", TAG+="uaccess"
```

The file name must sort before `73-seat-late.rules`, or `uaccess` is not
applied. Reload with `sudo udevadm control --reload` and replug the dongle.

## Building

Requirements: Rust 1.85 or newer, Qt 6.8+, KDE Frameworks 6.13+ (Kirigami,
KI18n, KCoreAddons, KCrash), qqc2-desktop-style, CMake 3.25+ and
extra-cmake-modules for installing.

Run from the source tree:

```
cargo run -p ir-app
```

Build and install with CMake (it drives Cargo):

```
cmake -S . -B build -DCMAKE_INSTALL_PREFIX=/usr
cmake --build build
sudo cmake --install build
```

Arch Linux: `packaging/aur/PKGBUILD` builds the `ir-transceiver-git` package.

`third_party/cmake-package` is a patched copy of the `cmake-package` crate
that cxx-kde-frameworks needs to build; see `third_party/PATCHED.md`.

## Development

- `scripts/check.sh` runs clippy and all tests on stable Rust and on 1.85
  (Debian stable); `scripts/cargo-guarded.sh` runs cargo with a memory cap.
- `tools/debian-check/run.sh` builds and tests on Debian stable in a Docker
  container.
- `elk-probe` (in `crates/ir-transport`) talks to the dongle from the command
  line and records the traffic to `fixtures/`.

Crates:

| Crate | Purpose |
|---|---|
| `elk-proto` | Dongle protocol and remote protocol decoders/encoders, no I/O |
| `ir-codec` | Hex, binary, decimal and Base64 formatting and parsing |
| `ir-transport` | USB transport (nusb), dongle operations, `elk-probe` |
| `ir-session` | Device session state machine and worker thread |
| `ir-library` | The code library and its TOML file |
| `ir-settings` | User settings and their TOML file |
| `ir-formats` | LIRC, Flipper and Pronto formats |
| `ir-view` | Qt-free view models of the GUI pages |
| `ir-app` | The Kirigami GUI (cxx-qt bridges and QML) |

Design decisions are recorded in `docs/design-decisions.md`.

## License

MIT, see `LICENSE`.
