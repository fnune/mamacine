#!/usr/bin/env bash
# Fetches the programs the app drives and places them in src-tauri/binaries/, each verified
# against a recorded checksum. nzbget's releases carry the unrar and 7za they are tested with,
# so one pinned download per platform provides all three. Moving to a new version means
# updating the URLs and the checksums below, deliberately.
#
# Takes which platform to fetch for: "linux", "windows", or nothing for both. The Windows
# installer needs 7z (p7zip) to unpack; the Linux one unpacks itself.
set -euo pipefail

windows_url="https://github.com/nzbgetcom/nzbget/releases/download/v25.3/nzbget-25.3-bin-windows-setup.exe"
windows_sha256="531e32d48a7282f972282149a1d89c5c04b890ae9d66d1222c6f03cd3a019eba"
linux_url="https://github.com/nzbgetcom/nzbget/releases/download/v25.3/nzbget-25.3-bin-linux.run"
linux_sha256="dc3b57e1e5ffae78c28edcddb87950f9f9b426716c7e3ec24a44a824e77c87fc"

declare -A windows_programs=(
    [nzbget]="b1c2c853d526d00afbdbf70c8822d643ab481e9f1aa4fc5cda4be191a3d9995e"
    [unrar]="0d6b63ab2cf10c9b55eabc046580fcbf36c85f9459d3c2316493315674aadc72"
    [7za]="2c39b62edf81f576dcb1a80679b0ebfc67787761236e38c78466af026c8a60fa"
)
declare -A linux_programs=(
    [nzbget]="1bd52f7e26bedc435ccf84c1751f89b7caf5cf82e71ddaf59b2612fa502aedc6"
    [unrar]="0a984c6f20f8ed893724ea68f0822a4e11cefadfd686ca98113585be38ca1497"
    [7za]="55d1f66078d950beadc77cbb672436fc8d8cb2deb59975450ba4c4dedca60c9e"
)

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/src-tauri/binaries"
wanted="${1:-all}"

verify() {
    local file="$1" want="$2" got
    got="$(sha256sum "$file" | cut -d' ' -f1)"
    if [[ $got != "$want" ]]; then
        echo "$file does not match its recorded checksum" >&2
        echo "  expected $want" >&2
        echo "  received $got" >&2
        return 1
    fi
}

present() {
    local suffix="$1" extension="$2"
    local -n programs="$3"
    for name in "${!programs[@]}"; do
        [[ -f "$out/$name-$suffix$extension" ]] || return 1
        verify "$out/$name-$suffix$extension" "${programs[$name]}" 2>/dev/null || return 1
    done
}

fetch() {
    local url="$1" want="$2" destination="$3"
    echo "fetching $url" >&2
    curl --fail --location --silent --show-error --output "$destination" "$url"
    verify "$destination" "$want"
}

fetch_windows() {
    local suffix="x86_64-pc-windows-msvc"
    if present "$suffix" ".exe" windows_programs; then
        echo "windows sidecars already in place and matching" >&2
        return 0
    fi
    command -v 7z >/dev/null || {
        echo "7z (p7zip) is needed to unpack the nzbget Windows installer" >&2
        exit 1
    }
    local work
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' RETURN
    fetch "$windows_url" "$windows_sha256" "$work/installer.exe"
    # the 64-bit programs sit first in the archive, so with auto-rename they keep their plain
    # names; the per-file checksums catch any change in that order
    7z x -aou -o"$work/unpacked" "$work/installer.exe" >/dev/null
    mkdir -p "$out"
    for name in "${!windows_programs[@]}"; do
        verify "$work/unpacked/$name.exe" "${windows_programs[$name]}"
        install -m 644 "$work/unpacked/$name.exe" "$out/$name-$suffix.exe"
        echo "placed $name-$suffix.exe" >&2
    done
}

fetch_linux() {
    local suffix="x86_64-unknown-linux-gnu"
    if present "$suffix" "" linux_programs; then
        echo "linux sidecars already in place and matching" >&2
        return 0
    fi
    local work
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' RETURN
    fetch "$linux_url" "$linux_sha256" "$work/installer.run"
    (cd "$work" && sh installer.run --arch x86_64 --destdir "$work/unpacked" --unpack >/dev/null)
    mkdir -p "$out"
    for name in "${!linux_programs[@]}"; do
        verify "$work/unpacked/$name-x86_64" "${linux_programs[$name]}"
        install -m 755 "$work/unpacked/$name-x86_64" "$out/$name-$suffix"
        echo "placed $name-$suffix" >&2
    done
}

case "$wanted" in
    linux) fetch_linux ;;
    windows) fetch_windows ;;
    all)
        fetch_linux
        fetch_windows
        ;;
    *)
        echo "usage: fetch-sidecars.sh [linux|windows]" >&2
        exit 1
        ;;
esac
