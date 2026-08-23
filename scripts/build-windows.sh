#!/usr/bin/env bash
# Builds the Windows installer in a pinned container. `just windows`.
#
# Nix is for development: a Nix-built binary links against store paths that exist on no other
# machine. The release is built here instead, against fixed versions of everything, so the same
# source produces the same installer on this machine and in CI.
set -euo pipefail

# Ubuntu 24.04, pinned by digest. Replace when deliberately moving forward, never silently.
image="docker.io/library/ubuntu:24.04"
rust_version="1.90.0"
target="x86_64-pc-windows-msvc"

# Every timestamp the bundle would otherwise take from the clock.
export SOURCE_DATE_EPOCH=1700000000

root="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -z ${MAMACINE_RELEASE_BUILDER:-} ]]; then
    runtime="$(command -v podman || command -v docker)" || {
        echo "podman or docker is needed to build a release" >&2
        exit 1
    }

    cache="${XDG_CACHE_HOME:-$HOME/.cache}/mama-cine/release"
    mkdir -p "$cache/cargo" "$cache/rustup" "$cache/xwin" "$cache/target"

    exec "$runtime" run --rm \
        --env MAMACINE_RELEASE_BUILDER=1 \
        --env SOURCE_DATE_EPOCH \
        --volume "$root:/src:z" \
        --volume "$cache/cargo:/root/.cargo:z" \
        --volume "$cache/rustup:/root/.rustup:z" \
        --volume "$cache/xwin:/root/.cache/cargo-xwin:z" \
        --volume "$cache/target:/target:z" \
        --workdir /src \
        "$image" "/src/scripts/$(basename "$0")" "$@"
fi

export CARGO_TARGET_DIR=/target
export PATH="/root/.cargo/bin:$PATH"
export DEBIAN_FRONTEND=noninteractive

apt-get update
# libayatana-appindicator3-dev and pkg-config are not for the Windows binary. tauri-cli gates its
# appindicator probe on the host it runs on rather than the target, and the tray-icon feature makes
# it probe, so bundling for Windows from Linux panics without them.
apt-get install --yes --no-install-recommends \
    build-essential ca-certificates clang curl git llvm-dev libclang-dev nsis \
    libayatana-appindicator3-dev pkg-config

if [[ ! -x /root/.cargo/bin/rustup ]]; then
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain "$rust_version" --profile minimal
fi
rustup target add "$target"

# already installed on a warm cache, so a failure here is not fatal
cargo install --locked cargo-xwin --version "=0.22.0" || true
cargo install --locked tauri-cli --version "^2" || true

# The programs the app drives have to travel with it: Windows has none of them.
if [[ ! -d /src/src-tauri/binaries ]]; then
    echo "src-tauri/binaries is missing. Run scripts/pin-sidecars.sh and place the" >&2
    echo "unpacked programs as <name>-$target.exe before building." >&2
    exit 1
fi

# NSIS runs natively on Linux, so an installer can be produced without Windows.
# WiX MSI cannot, and is deliberately not built here.
cargo tauri build \
    --runner cargo-xwin \
    --target "$target" \
    --bundles nsis \
    -- --locked

echo
echo "installer:"
find "$CARGO_TARGET_DIR/$target/release/bundle" -name '*.exe' -print
