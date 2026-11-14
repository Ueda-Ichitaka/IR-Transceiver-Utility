# Patched third-party crates

## cmake-package 0.1.15

Source: crates.io `cmake-package` 0.1.15
(https://codeberg.org/danvratil/cmake-package-rs), MIT, see
`cmake-package/LICENSE`. Used through `[patch.crates-io]` in the workspace
`Cargo.toml`.

Change: `cmake-package-0.1.15-skip-cmake-targets.patch`. Link libraries that
still contain `::` are unresolved CMake targets (for example the header-only
`Qt6::QmlIntegration` pulled in by `KF6::I18nQml`). Unpatched, they are
printed as `cargo:rustc-link-lib=dylib=Qt6::QmlIntegration`, which rustc
reads as library renaming syntax, and cxx-kde-frameworks fails to build.

Remove this copy and the `[patch.crates-io]` entry once upstream handles
such targets.
