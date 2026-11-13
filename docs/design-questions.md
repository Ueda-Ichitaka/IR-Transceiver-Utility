# Open design questions

Decisions needed before the parts of the app that depend on them can be built.
Work that does not depend on these answers continues in the meantime.
Recommendations are marked where one exists; none of them is applied yet.

## Repository and build

1. **Build system** - CMake + ECM wrapping Cargo (KDE tutorial: install, .desktop, icon) or Cargo only?
2. **KDE Frameworks bindings** - `cxx-kde-frameworks` (git dependency, work in progress; KAboutData, KConfig, KCrash) or plain Qt (QSettings) only?
3. **App identity** - reverse-DNS app id and binary name, for example `io.github.<you>.irtransceiver` / `ir-transceiver`?
4. **License** - keep MIT? Linking LGPL Qt/Kirigami is fine; KDE-hosted projects expect GPL/LGPL.
5. **Minimum versions** - only current Qt 6.12 / KF 6.31, or older distributions too?
6. **Distribution** - `cmake --install` to `~/.local` only, or also an AUR PKGBUILD or Flatpak (needs a USB device permission)?

## Device scope

7. **Huffman protocol** - the 0132 here uses the plain protocol. Recommendation: leave Huffman out of v1.
8. **Other ELK models** (0195, 0184, 0134) - try to support them, or 0132 only?
9. **More than one dongle** - support several dongles at once with a picker, or just the first one found?

## Sending

10. **Send payload types** - learned code, pulse timings, NEC address/command, raw USB bytes: all four or a subset?
    - Finding: elksmart-comm's `-n` option sends NEC bits MSB first, but standard NEC (remotes, Linux, IRremote) is LSB first. `elk-proto` supports both; `elk-probe transmit --nec` and the decoder use the standard order, so `--nec 0059` replays the button learned in the fixture.
11. **Raw USB bytes** - only behind an "advanced" switch, or always visible?
12. **Repeat signals in learned codes** - by default cut a learned code down to its first signal and send repeats with a count, or send exactly what was learned?
13. **Carrier frequency** - editable per code (default 38 kHz), or one global setting?

## Capture

14. **Stopping capture while waiting for a signal** - the dongle stays stuck until it sees IR. Just show a banner, or also ask you to press any remote button?
15. **Capture buffer limit** - is the smaller of 64 MiB and 5 % of free RAM OK? When full, drop the oldest rows or stop capturing?
16. **Captures after closing the app** - keep only in memory until exported, or save automatically?
17. **Export formats** - JSON, CSV, raw .bin, LIRC raw config, Pronto hex, Flipper .ir - which ones?
18. **Import formats** - the same list, or fewer?

## Encodings and decoding

19. **Encoding switch** - one switch for the whole app or one per view? Uppercase or lowercase hex? 16 bytes per line?
20. **Pulse timings in the encoding switch** - should hex/bin/dec/Base64 also apply to timings, or only to bytes?
21. **Remote protocols to recognise** - NEC, NEC-extended, RC5, RC6, Sony SIRC, Samsung, Kaseikyo/Panasonic - which ones in v1?

## Library

22. **Library file** - JSON or TOML, stored in `~/.local/share/<app-id>/`?
23. **Organisation** - a flat list, devices with buttons ("Living room TV" > "Power"), or tags?

## UI and integration

24. **Navigation and languages** - sidebar drawer as in the mockup, or also a phone layout (Plasma Mobile)? Translations (KI18n, for example German), or English only?
25. **Outside the window** - tray icon, global shortcuts, D-Bus, or a CLI such as `ir-transceiver send "TV Power"`? Which of these, if any?
