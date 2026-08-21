# The app, through nixGL so Nix's webkit can reach this machine's GPU drivers.
dev:
    ./scripts/dev.sh

# The interface alone, in an ordinary browser, with canned data and no webview.
preview:
    xdg-open ui/preview.html

# An AppImage for this machine, built in a pinned container. Runs where the Nix build cannot.
release:
    ./scripts/build-linux.sh

# The Windows installer for her machine, cross-compiled in the same way.
windows:
    ./scripts/build-windows.sh

test:
    cargo test --workspace

check:
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    node --test ui/*.test.js

# Real search results, ordered as the grid would order them. Responses are cached on disk, so a
# repeated query costs the indexer nothing. Also: --series/--film/--suggest.
probe *args:
    cargo run -p mamacine-core --example probe -- {{args}}

# Every icon, from the one SVG: the copy the window shows, the 1024 px PNG, and the set Tauri bundles.
icons:
    cp icon-source.svg ui/icon.svg
    rsvg-convert -w 1024 -h 1024 icon-source.svg -o icon-source.png
    cargo tauri icon icon-source.png -o src-tauri/icons
