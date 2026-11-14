# Draft issue for cmake-package-rs

Where: https://codeberg.org/danvratil/cmake-package-rs/issues
Status: draft, not posted.

---

**Title:** `link()` emits unresolved CMake targets like `Qt6::QmlIntegration` as `rustc-link-lib`, which rustc rejects

**Versions:** cmake-package 0.1.15 (0.3.0 has the same `link_write`), CMake
4.5.0, Qt 6.12.0, KDE Frameworks 6.31.0 (Arch Linux), rustc 1.100.0.

**What happens**

Building cxx-kde-frameworks (master, 5c51af42) fails in its build script
output:

```
error: renaming of the library `Qt6` was specified, however this crate contains no `#[link(...)]` attributes referencing this library
```

The build script asks cmake-package for the link libraries of
`KF6::I18nQml`. Their interface link libraries include the CMake target
`Qt6::QmlIntegration`, which is header-only (an INTERFACE target without a
library file). `link_write` prints it unchanged:

```
cargo:rustc-link-lib=dylib=Qt6::QmlIntegration
```

`rustc -l` reads `NAME:RENAME`, so this asks to rename a library `Qt6` and
the build fails.

**Expected**

Names that are still `Namespace::Target` after CMake evaluation are not
libraries and should not be passed to rustc (or should be resolved to their
own link libraries).

**Possible fix** (applied locally, cxx-kde-frameworks then builds and the
Kirigami app links and runs):

```diff
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -287,6 +287,10 @@
             writeln!(io, "cargo:rustc-link-arg={}", opt).unwrap();
         });
         self.link_libraries.iter().for_each(|lib| {
+            // Unresolved CMake targets (e.g. header-only Qt6::QmlIntegration) are not libraries.
+            if lib.contains("::") {
+                return;
+            }
             if lib.starts_with("-") {
                 writeln!(io, "cargo:rustc-link-arg={}", lib).unwrap();
             } else {
```

A more complete fix would follow INTERFACE targets recursively and collect
their `INTERFACE_LINK_LIBRARIES` instead of dropping them.

---

# Draft note for cxx-kde-frameworks

Where: https://invent.kde.org/libraries/cxx-kde-frameworks/-/issues
Status: draft, not posted.

**Title:** Build fails with KF 6.31 / Qt 6.12: `renaming of the library Qt6` (via cmake-package)

Building master (5c51af42) on Arch Linux (Qt 6.12.0, KF 6.31.0) fails with
the error above. The cause is in cmake-package (link to the issue above once
posted): the link libraries of `KF6::I18nQml` contain the header-only target
`Qt6::QmlIntegration`, which is emitted as `rustc-link-lib`. Patching
cmake-package as shown above fixes the build. Until upstream is fixed,
`[patch.crates-io]` with a patched cmake-package works around it.
