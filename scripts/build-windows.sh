#!/usr/bin/env bash
set -euo pipefail

image="docker.io/library/ubuntu:24.04"
rust_version="1.90.0"
target="x86_64-pc-windows-msvc"

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
apt-get install --yes --no-install-recommends \
    build-essential ca-certificates clang curl git llvm-dev libclang-dev nsis \
    libayatana-appindicator3-dev p7zip-full pkg-config

if [[ ! -x /root/.cargo/bin/rustup ]]; then
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain "$rust_version" --profile minimal
fi
rustup target add "$target"

cargo install --locked cargo-xwin --version "=0.22.0" || true
cargo install --locked tauri-cli --version "=2.11.4" || true

/src/scripts/fetch-sidecars.sh windows

cargo tauri build \
    --runner cargo-xwin \
    --target "$target" \
    --bundles nsis \
    -- --locked

installer="$(find "$CARGO_TARGET_DIR/$target/release/bundle/nsis" -name '*-setup.exe' -print -quit)"
if [[ -z $installer ]]; then
    echo "the bundler left no installer behind" >&2
    exit 1
fi

dist="/src/dist"
mkdir -p "$dist"
cp "$installer" "$dist/MamaCine-x64-setup.exe"

cd "$dist"
checksum_all() {
    find . -maxdepth 1 -type f ! -name checksums.txt -printf '%P\0' | sort -z | xargs -0 -r sha256sum
}
checksum_all > checksums.txt
cat checksums.txt
