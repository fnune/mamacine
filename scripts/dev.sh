#!/usr/bin/env bash
# Runs the app in development.
#
# On a system that is not NixOS the Nix webkitgtk cannot reach the system GPU drivers, so the app
# goes through nixGL, as clack does.
#
# The backend follows the session. Clack pins x11 to get the desktop's own window decorations, but
# on a Wayland session that means every frame goes through XWayland and scrolling visibly stutters.
# Smooth scrolling is worth more here than server-side decorations.
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
