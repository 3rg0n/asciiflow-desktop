# ASCIIFlow Desktop

A desktop editor for plain-text diagrams, inspired by ASCIIFlow and built in Rust with Zed’s GPUI. Use it to draw diagrams you can paste into a README, source comment, or terminal. [ASCIIFlow](https://github.com/lewish/asciiflow) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)

The [product walkthrough](docs/index.html) lives in `docs/`. Preview it locally with `python -m http.server 8000 --bind 127.0.0.1 --directory docs`, then open `http://127.0.0.1:8000/`. Publish `main` → `/docs` with GitHub Pages. Before publishing, make the repository public and verify that the release includes all archives linked from the page and their checksums. [GitHub Pages documentation](https://docs.github.com/en/pages/getting-started-with-github-pages/creating-a-github-pages-site)

## Install a release

Packages for each release are on the [releases page](https://github.com/3rg0n/asciiflow-desktop/releases). The installers verify each package against the release's `SHA256SUMS` before replacing the executable.

**Linux x86-64 and macOS Apple Silicon**

```sh
curl -fsSL https://raw.githubusercontent.com/3rg0n/asciiflow-desktop/main/install.sh | bash
# Open a new shell, then run:
asciiflow-desktop
```

The script downloads the package for your platform from the release, installs it into `~/.local/bin`, and updates your shell startup configuration as needed. To pass options, use `bash -s --` followed by them, for example `| bash -s -- --prefix "$HOME/bin" --no-path`. Use `--prefix` to choose a different bin directory, `--no-path` to skip the PATH change, or `--version` to install another release. From a checkout with the corresponding package, `sh ./install.sh` uses `dist/`.

The Linux package targets glibc 2.36 or later. On Debian or Ubuntu, ensure `libfontconfig1`, `libfreetype6`, `libxkbcommon0`, `libxkbcommon-x11-0`, `libxcb1`, `libwayland-client0`, `libwayland-cursor0`, and `libvulkan1` are installed, along with a Vulkan driver.

The macOS package is built for macOS 11 or later and verified on macOS 27.3. The executable is ad-hoc signed and not notarized. The installer doesn't mark it as quarantined, so it opens directly. If you download the archive with a browser instead, clear the quarantine flag after extracting it with `xattr -d com.apple.quarantine asciiflow-desktop`. Otherwise macOS refuses to open it. Quit with **Quit ASCIIFlow Desktop** in the app menu or `Cmd+Q`.

**macOS Intel** packages are pending native verification.

**Windows x86-64**

Once the repository and release are public, run this in PowerShell.

```powershell
irm https://raw.githubusercontent.com/3rg0n/asciiflow-desktop/main/docs/install.ps1 | iex
```

For custom install options or while the repository is private, download the 0.1.0 release source or clone the repository at tag `0.1.0`, then run the installer from that directory.

```powershell
.\install.ps1
asciiflow-desktop
```

The script installs the GUI executable in `%LOCALAPPDATA%\Programs\ASCIIFlow Desktop` and adds that directory to your user PATH. Use `-InstallDir` to choose a different directory or `-NoPath` to skip the PATH change. Close any running copy before upgrading.

## Build and run

Install the latest stable Rust toolchain and the prerequisites for your platform. Run these commands from the repository root.

```sh
cargo run --locked
cargo test --locked
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
```

Build an optimized executable with `cargo build --locked --release`. Cargo puts it in `target/release/` as `asciiflow-desktop.exe` on Windows and `asciiflow-desktop` elsewhere.

### Windows

Use the MSVC Rust toolchain with Visual Studio Build Tools, the **Desktop development with C++** workload, and a Windows SDK. For release builds, GPUI uses the SDK’s `fxc.exe`. If it’s outside `PATH`, set `GPUI_FXC_PATH` to the full executable path before building. [Zed Windows build guide](https://zed.dev/docs/development/windows)

To cross-compile for Windows ARM64, install the Visual Studio ARM64 C++ build tools and run the following in an x64-to-ARM64 developer shell.

```powershell
rustup target add aarch64-pc-windows-msvc
cargo build --locked --release --target aarch64-pc-windows-msvc
```

### Linux

On Debian or Ubuntu, install the build dependencies with:

```sh
sudo apt-get install build-essential clang cmake pkg-config \
  libfontconfig1-dev libssl-dev libwayland-dev libx11-xcb-dev \
  libxkbcommon-dev libxkbcommon-x11-dev
```

Run the app in a Wayland or X11 session with a Vulkan driver. Native Open and Export dialogs use the desktop’s XDG file chooser portal. [Zed Linux build guide](https://zed.dev/docs/development/linux)

The lockfile keeps `libc` and `mio` on versions compatible with GPUI 0.2.2’s Linux dependencies. Recheck the Linux build when updating either package. [GPUI upstream issue](https://github.com/zed-industries/zed/issues/67211)

### macOS

Install full Xcode and select it with `xcode-select`. Confirm that `xcrun metal --version` works. If the Metal compiler component is missing, install it with `xcodebuild -downloadComponent MetalToolchain`. [Zed macOS build guide](https://zed.dev/docs/development/macos)

## Editing

| Tool | Key | Gesture |
| --- | --- | --- |
| Select | `S` | Drag a region, then drag inside it to move it |
| Box | `B` | Drag between opposite corners |
| Line | `L` | Drag between endpoints |
| Arrow | `A` | Drag from the tail to the arrowhead |
| Draw | `D` | Drag to draw freehand |
| Text | `T` | Click to place the cursor and type |
| Erase | `E` | Drag across cells to clear them |
| Region | `R` | Drag around a diagram, or click an existing region to select it |

Use `Cmd` on macOS and `Ctrl` on Windows or Linux.

| Action | Shortcut |
| --- | --- |
| Undo / redo | `Mod+Z` / `Mod+Shift+Z` or `Mod+Y` |
| Cut / copy / paste | `Mod+X` / `Mod+C` / `Mod+V` |
| Select all | `Mod+A` |
| New drawing | `Mod+N` |
| Open / export text file | `Mod+O` / `Mod+S` |
| Pan | Scroll, `Shift+scroll` horizontally, or middle-button drag |
| Zoom | `Mod+scroll` |
| Return to Select | `Esc` |
| Quit (macOS) | `Cmd+Q` |

**Drawings** opens the local library. Choose a drawing to reopen it, or rename, duplicate, or delete the current drawing. Edits autosave locally and the last active drawing reopens at launch. **Open** imports a UTF-8 text file as a new drawing. Set `ASCIIFLOW_DATA_DIR` to an absolute path to use a separate local library.

**Export** offers UTF-8 plain text, SVG, and PNG. All three are cropped to the occupied cells. Copy uses the selection when one is active and the whole canvas otherwise.

## Regions and templates

Use **Region** to mark the cells that belong to one diagram. The region outline is saved with the drawing and stays out of ordinary exports. Select its type, then use **Mermaid…** to preview and copy or save `.mmd` source from those cells.

**Templates** starts a new drawing with an editable flowchart, sequence diagram, swimlane, or architecture sketch and a region around it. Use the templates as layout examples for the offline Mermaid converter. It recognizes bracketed nodes, explicit arrows, and the shown lane or lifeline conventions. Follow the preview message when a connection is ambiguous. Swimlanes and architecture groups use Mermaid flowchart subgraphs.

## AI architecture rendering

Open **Settings** to configure the OpenAI-compatible model for all drawings. Enter a base URL ending in `/v1`, a model name, and a key if the provider requires one. HTTP is supported on loopback for local engines. Enable **JSON Schema output** when the endpoint supports it, then select **Save settings**. Select cells or a diagram region and open **Diagram rendering** to use the model.

**Send and render architecture** sends the displayed ASCII to the chosen endpoint. Compare the generated diagram with the source, review inference warnings, and adjust groups or connections. Select **Confirm source review** to enable standalone HTML and SVG exports. A changed diagram requires a new confirmation. The review shows the requested and provider-reported model names. The provider controls the deployment behind those names. Choosing local TXT, SVG, PNG, or Mermaid export requires no model request.

The endpoint, model, and schema mode are saved in the app’s `settings.json` under `%APPDATA%\ASCIIFlow Desktop` on Windows, `~/Library/Application Support/ASCIIFlow Desktop` on macOS, or `$XDG_CONFIG_HOME/ASCIIFlow Desktop` on Linux (`~/.config` by default). `ASCIIFLOW_CONFIG_DIR` can override this directory with an absolute path. Older endpoint and model settings carry over when you next save.

The API key is saved to Windows Credential Manager, macOS Keychain, or Linux Secret Service. If the app detects no credential manager, it stores the key in plain text in `settings.json` and displays a notice. The key is tied to its endpoint and the form clears it when the URL changes. To remove a saved key, clear the field and save.

Startup overrides can come from `ASCIIFLOW_AI_BASE_URL`, `ASCIIFLOW_AI_MODEL`, and `ASCIIFLOW_AI_API_KEY`. Supply the explicit base URL alongside an environment API key. The environment key is used for the session, including when saving other settings.

With those variables set, run the opt-in compatibility smoke test against your endpoint. It sends a synthetic ASCII flowchart and checks the returned connection direction.

```sh
cargo test --locked architecture::tests::live_openai_compatible_smoke -- --ignored
```

## Artwork

The header uses the original ASCIIFlow mark, and the Windows executable uses the upstream favicon. Both are from Lewis Hemens’s ASCIIFlow project and are covered by the [included MIT license](assets/LICENSE-ASCIIFlow). [Upstream artwork](https://github.com/lewish/asciiflow/tree/758bcb00fde489542baebcd0cabd2a4af1e586fe/client/public)
