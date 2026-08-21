#!/usr/bin/env bash
# Builds an AppImage that runs on this machine, and on any other Linux, in a pinned container.
#
# Not from the Nix flake: a Nix-built binary links against store paths no other machine has, and its
# webkit cannot reach the graphics drivers of a host that is not NixOS. The container carries an
# ordinary distribution's libraries, which is what a portable bundle needs to be built against.
set -euo pipefail

image="docker.io/library/ubuntu:22.04"
rust_version="1.90.0"

appimagetool_version="1.9.0"
appimagetool_sha256="46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1"

# Every timestamp the bundle would otherwise take from the clock, so the same source produces the
# same bytes here and in CI.
# Not exported as SOURCE_DATE_EPOCH: mksquashfs refuses to take a timestamp from the environment
# and from its own flags at once, and the flags below are the ones that make the bundle reproducible.
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

cargo install --locked tauri-cli --version "^2" || true

dist="/src/dist"
rm -rf "$dist"
mkdir -p "$dist"

# Tauri builds the AppDir and its own AppImage. The AppDir is what we want; the bundle it produces
# is repacked below, because two things in it have to be corrected first.
cargo tauri build --bundles appimage -- --locked

bundle="$CARGO_TARGET_DIR/release/bundle/appimage"
appdir="$(find "$bundle" -maxdepth 1 -name '*.AppDir' | head -1)"
if [[ -z $appdir ]]; then
    echo "Tauri left no AppDir behind in $bundle" >&2
    exit 1
fi

export APPIMAGE_EXTRACT_AND_RUN=1

# The bundle carries its own libwayland, but the host's mesa is what ends up loaded and it needs the
# host's symbols. Leaving these in is what makes the window come up blank with EGL_BAD_PARAMETER.
rm -f "$appdir"/usr/lib/libwayland-*.so.*

# The GTK hook pins the backend to x11 outright. On a Wayland session that puts every frame through
# XWayland, and scrolling stutters visibly for it. The window loses the desktop's own decorations
# and draws its own instead, which is the cheaper of the two costs.
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

# What went into the bundle, so two builds that disagree can be compared without moving 80 megabytes.
{
    (cd "$appdir" && find . -type f -exec sha256sum {} + | sort -k2)
    (cd "$appdir" && find . -type l -printf '%p -> %l\n' | sort)
} > "$dist/appdir-manifest.txt"

cd "$dist"
sha256sum MamaCine-x86_64.AppImage > checksums.txt
cat checksums.txt
