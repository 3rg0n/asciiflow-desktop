# ASCIIFlow Desktop 0.1.0 on macOS Apple Silicon

This record covers the native build and end-to-end checks for `asciiflow-desktop-0.1.0-macos-aarch64.tar.gz`. All checks ran on 27 November 2026. The drawings, endpoint, model name, and key are synthetic.

The current archive is a rebuild from `main` at `b4f93fd`. It includes the macOS Quit command and the app-wide Settings with Keychain storage. The version stays 0.1.0, and tag `0.1.0` still points at `be68e8a`. The first section below covers the rebuild. The sections after it record the first package, built from the tag.

## Rebuild from `main` at `b4f93fd`

The environment matches the first build below. The source was a clean detached worktree at `b4f93fd6da2cf0cd69aa86fcfb3437dffac85974`.

| Command | Result |
| --- | --- |
| `xcrun metal --version` | Apple metal 32023.922 |
| `cargo fmt --all --check` | Pass |
| `cargo test --locked` | Pass: 24 passed, 1 ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass |
| `cargo build --locked --release` | Pass |

The release executable ran with temporary absolute `ASCIIFLOW_CONFIG_DIR` and `ASCIIFLOW_DATA_DIR` paths, a synthetic API key, and a local mock endpoint at `http://127.0.0.1:18080/v1`.

| Area | Check | Result |
| --- | --- | --- |
| Settings | The endpoint, model, and schema mode were restored after quitting and relaunching with a second drawing library. | Pass |
| Keychain | `settings.json` holds only `{"store": "credential_manager", "account": "<uuid>"}`. A login Keychain generic password exists for service `ASCIIFlow Desktop` and that account. After the restart, the app read the key back and sent it as the bearer token. | Pass |
| Endpoint change | Editing the endpoint cleared the key field and showed "Endpoint changed. Enter its API key again if needed." | Pass |
| Key removal | Clearing the restored key and saving removed the key entry from `settings.json` and the Keychain item. No `ASCIIFlow Desktop` items remained. | Pass |
| Architecture review | The architecture template rendered through the mock. After **Confirm source review**, the exported SVG parses as XML. The HTML has a doctype, contains a valid SVG, and has no scripts or external URLs. | Pass |
| Cmd+Q | The app menu shows **Quit ASCIIFlow Desktop ⌘Q**. An edit followed by `Cmd+Q` was saved and reopened after relaunch. | Pass |
| Save failure | With an unwritable drawing directory, `Cmd+Q` showed "Close without saving?", and **Keep open** left the app running. After write access was restored, `Cmd+Q` saved and quit. | Pass |
| Installer | `sh ./install.sh --version 0.1.0 --prefix "$TEMP_BIN" --no-path` installs an executable identical to the archive, without extended attributes. It launches and quits with `Cmd+Q`. | Pass |

The drawing, Mermaid, Open, and TXT, SVG, and PNG export checks below ran on the first build and weren't repeated.

The rebuilt executable is ad-hoc signed by the linker and isn't notarized.

| File | SHA-256 |
| --- | --- |
| `asciiflow-desktop-0.1.0-macos-aarch64.tar.gz` | `930966cdf436f0db195d0c58bd731e5621e690e077147d275353b8789fa9ddd3` |
| `asciiflow-desktop` inside the archive | `1ad8a4660b96cbff62783b93b7366fd5c9d86e87d1650ccc900ccca1e1707f12` |

| Screenshot | Shows |
| --- | --- |
| `11-settings-restored-second-library.png` | Settings restored in a second library, with the key masked |
| `12-architecture-review-confirmed.png` | Architecture review after **Confirm source review** |
| `13-quit-save-failure-prompt.png` | The prompt `Cmd+Q` shows when autosave fails |

## First build from tag `0.1.0`

### Environment

| Item | Value |
| --- | --- |
| macOS | 27.3 (26C5047e) |
| CPU | Apple M4 Max, `arm64` |
| Xcode | 27.2 (27B52), SDK 27.2, selected with `xcode-select` |
| Metal compiler | Apple metal 32023.922 (metalfe-32023.922.4), Metal Toolchain 27B47 |
| Rust | rustc 1.100.0, cargo 1.100.0 |
| Source | Tag `0.1.0` at `be68e8a`, in a clean detached worktree |

### Build and static checks

| Command | Result |
| --- | --- |
| `cargo build --locked --release` | Pass |
| `cargo test --locked` | Pass: 22 passed, 1 ignored (the opt-in live endpoint smoke test) |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass |

The executable is a Mach-O `arm64` binary with a minimum deployment target of macOS 11.0. It links only system frameworks and libraries, including AppKit, Metal, QuartzCore, CoreText, and `libSystem`. The linker signed it ad hoc. It isn't notarized, so `spctl` rejects it and Gatekeeper blocks a quarantined copy. Clearing `com.apple.quarantine` lets it launch. Installing with `install.sh`, either from a checkout or piped from `curl`, never sets the flag.

Cargo reports a future-incompatibility warning for the transitive `block` 0.1.6 and `proc-macro-error2` 2.0.1 crates. The warning doesn't affect this build.

### End-to-end checks

The release executable ran with `ASCIIFLOW_DATA_DIR` set to an empty temporary directory. Input came from native macOS mouse, keyboard, and pasteboard events.

| Area | Check | Result |
| --- | --- | --- |
| Drawing | Box, Line, Arrow, Draw, and Erase with the `B`, `L`, `A`, `D`, `E`, `S`, and `R` shortcuts | Pass |
| Text input | Text tool placement and typing, and `Esc` returning to Select | Pass |
| Shortcuts | `Cmd+Z`, `Cmd+Shift+Z`, `Cmd+A`, `Cmd+N`, `Cmd+O`, `Cmd+S`, `Cmd+scroll` zoom, `Shift+scroll` and middle-drag pan | Pass |
| Clipboard | `Cmd+C` with and without a selection, `Cmd+X`, `Cmd+V` of internal and external pasteboard text | Pass |
| Library | Rename, switch drawings, and reopen a named drawing | Pass |
| Autosave | Edits are written to `drawings.json`. After closing and relaunching, the named drawing reopens with its content. | Pass |
| Templates | Sequence diagram template, selecting its region with the Region tool | Pass |
| Mermaid | Preview, Copy Mermaid, and Export `.mmd` produce `sequenceDiagram` with three participants and four messages | Pass |
| Native dialogs | Open imports a UTF-8 text file as a new drawing. Save writes TXT, SVG, PNG, `.mmd`, HTML, and SVG architecture output. | Pass |
| Export | TXT, SVG, and PNG are cropped to the occupied cells. The PNG is 720 × 171. | Pass |
| Architecture review | See the next section | Pass |
| Installer | `sh ./install.sh --prefix "$TMPDIR/asciiflow-bin" --no-path` verifies the checksum and installs an executable identical to the build. The installed executable launches and reads the library. | Pass |

#### Architecture review

A local mock of an OpenAI-compatible endpoint listened on `http://127.0.0.1:18080/v1`. The System architecture template was the source, and the model name and key were entered in **Diagram rendering** with JSON Schema output on. The app sent one `POST /v1/chat/completions` request. It carried the bearer key, the model name, a `json_schema` response format, and the exact ASCII source.

The review screen showed the returned group, nodes, edges, and warning. Changing Browser's group re-laid out the preview. The exported HTML and SVG include that change. The HTML is self-contained, with no scripts or external resources.

### Findings

- The build defines no macOS app menu, so `Cmd+Q` and the app menu have no Quit command. Closing the window saves and quits. The rebuild adds the Quit command.
- The native Save dialog shows the suggested `diagram.txt` as `diagram`, because macOS hides the known extension. The saved file is named `diagram.txt`.

### Screenshots

| File | Shows |
| --- | --- |
| `01-drawing-and-text.png` | Boxes, an arrow, and typed text |
| `02-reopened-after-restart.png` | The named drawing reopened after the app relaunched |
| `03-sequence-template-region.png` | Sequence template with its region selected |
| `04-mermaid-preview.png` | Mermaid preview of the sequence region |
| `05-native-save-dialog.png` | Native Save dialog for a text export, pointed at the test output folder |
| `06-native-open-import.png` | Text file imported through the native Open dialog |
| `07-architecture-review.png` | Architecture review from the mock endpoint |
| `08-architecture-group-adjusted.png` | Review after moving Browser into the private network |
| `09-installed-executable-library.png` | Installed executable showing the drawing library |
| `10-export-png.png` | PNG export of the sequence diagram |

### Artifact

| File | SHA-256 |
| --- | --- |
| `asciiflow-desktop-0.1.0-macos-aarch64.tar.gz`, replaced by the rebuild | `e65c51f9de787a00543b0c8f3f11e574dce057843dc1e397bba5bbd228714b40` |
| `asciiflow-desktop` inside the archive | `6f8bc7f671db5c6ca81be9d3c425275958563b29f18e1ee6583336dcbc83a8d5` |

The archive holds `asciiflow-desktop` (mode 755) and `LICENSE-ASCIIFlow` (mode 644) at its root, owned by `root:root`, in ustar format without extended attributes.

macOS Intel wasn't built or tested, because no native Intel Mac was available.
