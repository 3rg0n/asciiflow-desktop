# Distribution packages

The `dist/<version>/` directory contains the archives and `SHA256SUMS` used by the project site and its installers.

| Platform | Archive | Executable inside the archive |
| --- | --- | --- |
| Windows x86-64 | `asciiflow-desktop-<version>-windows-x86_64.zip` | `asciiflow-desktop.exe` |
| Windows ARM64 | `asciiflow-desktop-<version>-windows-aarch64.zip` | `asciiflow-desktop.exe` |
| Linux x86-64 | `asciiflow-desktop-<version>-linux-x86_64.tar.gz` | `asciiflow-desktop` |
| macOS Apple Silicon | `asciiflow-desktop-<version>-macos-aarch64.tar.gz` | `asciiflow-desktop` |
| macOS Intel | `asciiflow-desktop-<version>-macos-x86_64.tar.gz` | `asciiflow-desktop` |

Each archive puts the executable at its root and includes the upstream artwork notice from `assets/LICENSE-ASCIIFlow`. Build from the release tag on the target operating system with `cargo build --locked --release`. Record the archive's SHA-256 in `SHA256SUMS` as a lowercase hex digest, two spaces, and the archive basename.

The Linux x86-64 package is built on Debian 12 with glibc 2.36. It needs a compatible Linux desktop, a Vulkan driver, and the system font and X11/Wayland libraries. The native file picker also needs a desktop portal. The Windows package is a console-free GUI executable.

For Windows ARM64, build with `cargo build --locked --release --target aarch64-pc-windows-msvc`. The executable is in `target/aarch64-pc-windows-msvc/release/`. Smoke-test the package on Windows ARM64 before publishing it. Version 0.1.0 ships the Windows x86-64 package.

The macOS Apple Silicon package is built natively with full Xcode and the Metal toolchain. Its minimum deployment target is macOS 11. It is ad-hoc signed without notarization. The macOS Intel package is pending native verification. See [docs/verification](../docs/verification) for platform test records.

`install.sh` uses the package in `dist/<version>/` when it runs from a checkout. Otherwise it downloads the package and `SHA256SUMS` from the repository over HTTPS. Set `ASCIIFLOW_RELEASE_URL` to a directory URL that holds both files to install from a mirror.
