#!/bin/sh
# Install the latest stable Cinnaroids release without sudo.
set -eu

fail() { printf 'cinnaroids: %s\n' "$*" >&2; exit 1; }
require() { command -v "$1" >/dev/null 2>&1 || fail "Required command not found: $1"; }

version=
case "$#" in
    0) ;;
    1) if [ "$1" = --help ]; then
           printf 'Usage: install.sh [--version X.Y.Z]\n'
           exit 0
       fi
       fail 'Usage: install.sh [--version X.Y.Z]' ;;
    2) [ "$1" = --version ] || fail 'Usage: install.sh [--version X.Y.Z]'
       version=$2
       [ -n "$version" ] || fail 'Release version must look like 1.2.3.' ;;
    *) fail 'Usage: install.sh [--version X.Y.Z]' ;;
esac

require curl
[ -n "${HOME:-}" ] || fail 'HOME is not set.'
system=$(uname -s)
machine=$(uname -m)
case "$system:$machine" in
    Linux:x86_64|Linux:amd64) platform=linux; arch=x86_64; extension=tar.gz ;;
    Darwin:arm64|Darwin:aarch64) platform=macos; arch=arm64; extension=zip ;;
    Darwin:x86_64)
        platform=macos; arch=x86_64; extension=zip
        # Prefer the native arm64 app when this shell runs under Rosetta.
        if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || true)" = 1 ]; then arch=arm64; fi ;;
    *) fail "Unsupported platform: $system $machine (Linux x86_64 and macOS are supported)." ;;
esac
if command -v sha256sum >/dev/null 2>&1; then
    checksum_command=sha256sum
elif command -v shasum >/dev/null 2>&1; then
    checksum_command=shasum
else
    fail 'Install sha256sum or shasum to verify the download.'
fi
if [ "$platform" = linux ]; then require tar; else require unzip; require ditto; require codesign; fi

repository=https://github.com/RestartFU/cinnaroids
if [ -z "$version" ]; then
    release_url=$(curl --fail --silent --show-error --location --retry 3 \
        --proto '=https' --proto-redir '=https' --output /dev/null \
        --write-out '%{url_effective}' "$repository/releases/latest")
    tag=${release_url##*/}
    case "$tag" in v*) version=${tag#v} ;; *) fail 'Could not resolve the latest release.' ;; esac
fi
printf '%s\n' "$version" | LC_ALL=C grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' \
    || fail 'Release version must look like 1.2.3.'

temporary=$(mktemp -d "${TMPDIR:-/tmp}/cinnaroids.XXXXXX")
binary_stage=
app_stage=
app_destination=$HOME/Applications/Cinnaroids.app
cleanup() {
    if [ -n "$app_stage" ]; then
        # Restore the previous app if replacement was interrupted.
        if [ -e "$app_stage/previous.app" ] && [ ! -e "$app_destination" ]; then
            mv "$app_stage/previous.app" "$app_destination" || return
        fi
        rm -rf "$app_stage"
    fi
    if [ -n "$binary_stage" ]; then rm -f "$binary_stage"; fi
    rm -rf "$temporary"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

bundle=Cinnaroids-$version-$platform-$arch
archive=$bundle.$extension
url=$repository/releases/download/v$version
printf 'Downloading Cinnaroids %s for %s %s...\n' "$version" "$platform" "$arch"
curl --fail --silent --show-error --location --retry 3 --proto '=https' --proto-redir '=https' \
    --output "$temporary/$archive" "$url/$archive"
curl --fail --silent --show-error --location --retry 3 --proto '=https' --proto-redir '=https' \
    --output "$temporary/$archive.sha256" "$url/$archive.sha256"
expected=$(awk 'NR == 1 {print $1}' "$temporary/$archive.sha256")
case "$expected" in *[!0-9a-fA-F]*|'') fail 'Invalid release checksum.' ;; esac
[ "${#expected}" -eq 64 ] || fail 'Invalid release checksum.'
if [ "$checksum_command" = sha256sum ]; then
    actual=$(sha256sum "$temporary/$archive" | awk '{print $1}')
else
    actual=$(shasum -a 256 "$temporary/$archive" | awk '{print $1}')
fi
[ "$actual" = "$expected" ] || fail 'Checksum mismatch; nothing was installed.'

mkdir "$temporary/extracted"
if [ "$platform" = linux ]; then
    tar -xzf "$temporary/$archive" -C "$temporary/extracted"
    data=${XDG_DATA_HOME:-$HOME/.local/share}/Cinnaroids/launcher
else
    unzip -q "$temporary/$archive" -d "$temporary/extracted"
    data=$HOME/Library/Application\ Support/Cinnaroids/launcher
fi
payload=$temporary/extracted/$bundle
for notice in LICENSE.txt THIRD_PARTY_NOTICES.txt README.md; do
    [ -f "$payload/$notice" ] || fail "Release is missing $notice."
done
[ -d "$payload/licenses" ] || fail 'Release is missing licenses.'

if [ "$platform" = linux ]; then
    [ -f "$payload/cinnaroids" ] || fail 'Release is missing the executable.'
    bin=$HOME/.local/bin
    mkdir -p "$bin"
    binary_stage=$(mktemp "$bin/.cinnaroids.XXXXXX")
    cp "$payload/cinnaroids" "$binary_stage"
    chmod 755 "$binary_stage"
    mv -f "$binary_stage" "$bin/cinnaroids"
    binary_stage=
    printf 'Installed %s\n' "$bin/cinnaroids"
    case ":${PATH:-}:" in
        *":$bin:"*) printf 'Run: cinnaroids\n' ;;
        *) printf 'Run: "%s/cinnaroids"\nAdd "%s" to your PATH to use the cinnaroids command.\n' "$bin" "$bin" ;;
    esac
else
    [ -f "$payload/Cinnaroids.app/Contents/MacOS/cinnaroids" ] || fail 'Release is missing the app executable.'
    codesign --verify --strict "$payload/Cinnaroids.app"
    mkdir -p "$HOME/Applications"
    app_stage=$(mktemp -d "$HOME/Applications/.cinnaroids.XXXXXX")
    ditto "$payload/Cinnaroids.app" "$app_stage/Cinnaroids.app"
    if [ -e "$app_destination" ] || [ -L "$app_destination" ]; then
        mv "$app_destination" "$app_stage/previous.app"
    fi
    mv "$app_stage/Cinnaroids.app" "$app_destination"
    printf 'Installed %s\nRun: open "%s"\n' "$app_destination" "$app_destination"
fi
mkdir -p "$data"
for notice in LICENSE.txt THIRD_PARTY_NOTICES.txt README.md; do cp "$payload/$notice" "$data/"; done
cp -R "$payload/licenses" "$data/"
printf 'Cinnaroids %s installed.\n' "$version"
