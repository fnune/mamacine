#!/usr/bin/env bash
set -euo pipefail

installer_url="https://github.com/nzbgetcom/nzbget/releases/download/v25.3/nzbget-25.3-bin-windows-setup.exe"
installer_sha256="531e32d48a7282f972282149a1d89c5c04b890ae9d66d1222c6f03cd3a019eba"

declare -A programs=(
    [nzbget]="b1c2c853d526d00afbdbf70c8822d643ab481e9f1aa4fc5cda4be191a3d9995e"
    [unrar]="0d6b63ab2cf10c9b55eabc046580fcbf36c85f9459d3c2316493315674aadc72"
    [7za]="2c39b62edf81f576dcb1a80679b0ebfc67787761236e38c78466af026c8a60fa"
)

target="x86_64-pc-windows-msvc"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/src-tauri/binaries"

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

all_present() {
    for name in "${!programs[@]}"; do
        [[ -f "$out/$name-$target.exe" ]] || return 1
        verify "$out/$name-$target.exe" "${programs[$name]}" 2>/dev/null || return 1
    done
}

if all_present; then
    echo "sidecars already in place and matching" >&2
    exit 0
fi

command -v 7z >/dev/null || {
    echo "7z (p7zip) is needed to unpack the nzbget installer" >&2
    exit 1
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "fetching $installer_url" >&2
curl --fail --location --silent --show-error --output "$work/installer.exe" "$installer_url"
verify "$work/installer.exe" "$installer_sha256"

7z x -aou -o"$work/unpacked" "$work/installer.exe" >/dev/null

mkdir -p "$out"
for name in "${!programs[@]}"; do
    verify "$work/unpacked/$name.exe" "${programs[$name]}"
    install -m 644 "$work/unpacked/$name.exe" "$out/$name-$target.exe"
    echo "placed $name-$target.exe" >&2
done
