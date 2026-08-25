#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -z ${GDK_BACKEND:-} ]]; then
    if [[ ${XDG_SESSION_TYPE:-} == wayland ]]; then
        export GDK_BACKEND=wayland
    else
        export GDK_BACKEND=x11
    fi
fi
export LD_LIBRARY_PATH="${MAMACINE_LIBRARY_PATH:-}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

if [[ -e /etc/NIXOS ]]; then
    exec cargo tauri dev "$@"
fi

exec nix run --impure github:nix-community/nixGL -- cargo tauri dev "$@"
