#!/bin/sh
set -eu

version=0.1.0
install_dir="$HOME/.local/bin"
update_path=true
user_shell=${SHELL:-/bin/sh}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version) version=${2:?--version requires a value}; shift 2 ;;
        --prefix) install_dir=${2:?--prefix requires a bin directory}; shift 2 ;;
        --no-path) update_path=false; shift ;;
        *) printf 'Usage: %s [--version VERSION] [--prefix BIN_DIR] [--no-path]\n' "$0" >&2; exit 2 ;;
    esac
done
if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    printf 'Invalid release version.\n' >&2
    exit 2
fi
case "$(uname -s):$(uname -m)" in
    Linux:x86_64) platform=linux-x86_64 ;;
    Darwin:arm64) platform=macos-aarch64 ;;
    Darwin:x86_64) platform=macos-x86_64 ;;
    *) printf 'This platform has no matching package. See the source build instructions.\n' >&2; exit 1 ;;
esac
repo_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
release_dir="$repo_dir/dist/$version"
package="asciiflow-desktop-$version-$platform.tar.gz"
archive="$release_dir/$package"
if [ ! -f "$archive" ] || [ ! -f "$release_dir/SHA256SUMS" ]; then
    printf 'Missing package or checksum for %s in %s.\n' "$platform" "$release_dir" >&2
    exit 1
fi
expected=$(awk -v package="$package" '$2 == package { print tolower($1); exit }' "$release_dir/SHA256SUMS")
if command -v sha256sum >/dev/null 2>&1; then
    checksum=$(sha256sum < "$archive")
else
    checksum=$(shasum -a 256 < "$archive")
fi
actual=${checksum%% *}
if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    printf 'SHA-256 verification failed for %s.\n' "$package" >&2
    exit 1
fi

mkdir -p -- "$install_dir"
install_dir=$(CDPATH='' cd -- "$install_dir" && pwd)
staged=$(mktemp "$install_dir/.asciiflow.XXXXXXXX")
trap 'rm -f -- "$staged"' EXIT
tar -xOf "$archive" asciiflow-desktop > "$staged"
chmod 755 "$staged"
mv -f -- "$staged" "$install_dir/asciiflow-desktop"
printf 'Installed %s/asciiflow-desktop\n' "$install_dir"

if [ "$update_path" = true ]; then
    case ":$PATH:" in
        *":$install_dir:"*) exit 0 ;;
    esac
    case "${user_shell##*/}" in
        fish) "$user_shell" -c 'fish_add_path -- $argv[1]' "$install_dir" ;;
        *)
            # Quote the destination for a future shell, including spaces and apostrophes.
            quoted=$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")
            path_line="case \":\$PATH:\" in *\":\"'$quoted'\":\"*) ;; *) export PATH='$quoted':\"\$PATH\" ;; esac"
            append_path() {
                if [ ! -f "$1" ] || ! grep -Fxq -- "$path_line" "$1"; then
                    printf '\n%s\n' "$path_line" >> "$1"
                fi
            }
            case "${user_shell##*/}" in
                bash)
                    append_path "$HOME/.bashrc"
                    if [ -f "$HOME/.bash_profile" ]; then append_path "$HOME/.bash_profile"
                    elif [ -f "$HOME/.bash_login" ]; then append_path "$HOME/.bash_login"
                    else append_path "$HOME/.profile"; fi ;;
                zsh) append_path "${ZDOTDIR:-$HOME}/.zshrc" ;;
                *) append_path "$HOME/.profile" ;;
            esac ;;
    esac
    printf 'Open a new shell to use asciiflow-desktop from PATH.\n'
fi
