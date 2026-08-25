#!/usr/bin/env bash
set -euo pipefail

image="docker.io/library/ubuntu:22.04"
rust_version="1.90.0"

appimagetool_version="1.9.0"
appimagetool_sha256="46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1"

epoch=1700000000

root="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -z ${MAMACINE_RELEASE_BUILDER:-} ]]; then
    runtime="$(command -v podman || command -v docker)" || {
        echo "podman or docker is needed to build a release" >&2
        exit 1
    }

    cache="${XDG_CACHE_HOME:-$HOME/.cache}/mama-cine/linux"
    mkdir -p "$cache/cargo" "$cache/rustup" "$cache/target" "$cache/apt" "$cache/tauri"

    exec "$runtime" run --rm \
        --env MAMACINE_RELEASE_BUILDER=1 \
        --volume "$root:/src:z" \
        --volume "$cache/cargo:/root/.cargo:z" \
        --volume "$cache/rustup:/root/.rustup:z" \
        --volume "$cache/target:/target:z" \
        --volume "$cache/apt:/var/cache/apt/archives:z" \
        --volume "$cache/tauri:/root/.cache/tauri:z" \
        --workdir /src \
        "$image" /src/scripts/build-linux.sh "$@"
fi

export CARGO_TARGET_DIR=/target
export PATH="/root/.cargo/bin:$PATH"
export DEBIAN_FRONTEND=noninteractive

fetch() {
    local url="$1" want="$2" destination="$3"
    curl --fail --location --silent --show-error --output "$destination" "$url"
    local got
    got="$(sha256sum "$destination" | cut -d' ' -f1)"
    if [[ $got != "$want" ]]; then
        echo "$url does not match its recorded checksum" >&2
        echo "  expected $want" >&2
        echo "  received $got" >&2
        exit 1
    fi
}

apt-get update
apt-get install --yes --no-install-recommends \
    build-essential ca-certificates curl file libgtk-3-dev librsvg2-dev libssl-dev \
    desktop-file-utils libwebkit2gtk-4.1-dev libayatana-appindicator3-dev patchelf pkg-config \
    squashfs-tools wget zsync

if [[ ! -x /root/.cargo/bin/rustup ]]; then
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain "$rust_version" --profile minimal
fi

if [[ ! -x /usr/local/bin/appimagetool ]]; then
    fetch "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-x86_64.AppImage" \
        "$appimagetool_sha256" /usr/local/bin/appimagetool
    chmod +x /usr/local/bin/appimagetool
fi

cargo install --locked tauri-cli --version "=2.11.4" || true

dist="/src/dist"
mkdir -p "$dist"
rm -f "$dist/MamaCine-x86_64.AppImage" "$dist/appdir-manifest.txt"

cargo tauri build --bundles appimage -- --locked

bundle="$CARGO_TARGET_DIR/release/bundle/appimage"
appdir="$(find "$bundle" -maxdepth 1 -name '*.AppDir' | head -1)"
if [[ -z $appdir ]]; then
    echo "Tauri left no AppDir behind in $bundle" >&2
    exit 1
fi

export APPIMAGE_EXTRACT_AND_RUN=1

rm -f "$appdir"/usr/lib/libwayland-*.so.*

if [[ -f "$appdir/apprun-hooks/linuxdeploy-plugin-gtk.sh" ]]; then
    # shellcheck disable=SC2016 # the hook expands these, not this script
    sed -i 's|^export GDK_BACKEND=x11|if [ -z "${GDK_BACKEND:-}" ]; then\n  if [ "${XDG_SESSION_TYPE:-}" = wayland ]; then export GDK_BACKEND=wayland; else export GDK_BACKEND=x11; fi\nfi|' \
        "$appdir/apprun-hooks/linuxdeploy-plugin-gtk.sh"
fi

find "$appdir" -exec touch --no-dereference --date="@$epoch" {} +

appimagetool \
    --no-appstream \
    --mksquashfs-opt -mkfs-time --mksquashfs-opt "$epoch" \
    --mksquashfs-opt -all-time --mksquashfs-opt "$epoch" \
    --mksquashfs-opt -all-root \
    "$appdir" "$dist/MamaCine-x86_64.AppImage"

{
    (cd "$appdir" && find . -type f -exec sha256sum {} + | sort -k2)
    (cd "$appdir" && find . -type l -printf '%p -> %l\n' | sort)
} > "$dist/appdir-manifest.txt"

cd "$dist"
checksum_all() {
    find . -maxdepth 1 -type f ! -name checksums.txt -printf '%P\0' | sort -z | xargs -0 -r sha256sum
}
checksum_all > checksums.txt
cat checksums.txt
