# Distribution packages

The `dist/<version>/` directory contains the archives and `SHA256SUMS` for that release. The same packages are available on the matching GitHub Release.

| Platform | Archive | Executable inside the archive |
| --- | --- | --- |
| Windows x86-64 | `asciiflow-desktop-<version>-windows-x86_64.zip` | `asciiflow-desktop.exe` |
| Linux x86-64 | `asciiflow-desktop-<version>-linux-x86_64.tar.gz` | `asciiflow-desktop` |
| macOS Apple Silicon | `asciiflow-desktop-<version>-macos-aarch64.tar.gz` | `asciiflow-desktop` |
| macOS Intel | `asciiflow-desktop-<version>-macos-x86_64.tar.gz` | `asciiflow-desktop` |

Each archive puts the executable at its root and includes the upstream artwork notice from `assets/LICENSE-ASCIIFlow`. Build from the release tag on the target operating system with `cargo build --locked --release`. Record the archive's SHA-256 in `SHA256SUMS` as a lowercase hex digest, two spaces, and the archive basename.

The Linux x86-64 package is built on Debian 12 with glibc 2.36. It needs a compatible Linux desktop, a Vulkan driver, and the system font and X11/Wayland libraries. The native file picker also needs a desktop portal. The Windows package is a console-free GUI executable.

macOS packaging is pending native verification. See the repository issues for the handoff.
