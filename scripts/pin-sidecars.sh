#!/usr/bin/env bash
# Downloads the Windows programs the app drives and prints their checksums, so build-release.sh
# can verify what it fetches instead of trusting a URL. Run this once, paste the output in.
set -euo pipefail

declare -A sources=(
    [nzbget]="https://github.com/nzbgetcom/nzbget/releases/download/v25.3/nzbget-25.3-bin-windows-64bit.7z"
    [unrar]="https://www.rarlab.com/rar/unrarw64.exe"
    [7z]="https://www.7-zip.org/a/7z2501-x64.exe"
    [ffprobe]="https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.7z"
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for name in "${!sources[@]}"; do
    url="${sources[$name]}"
    printf '%-9s %s\n' "$name" "$url" >&2
    curl --fail --location --silent --show-error --output "$work/$name" "$url"
    printf '    [%s]="%s"\n' "$name" "$(sha256sum "$work/$name" | cut -d' ' -f1)"
done
