# Agent instructions

The principles this repository is held to are in [engineering/README.md](engineering/README.md). Read that first.

## Layout

| Path | What it holds |
| --- | --- |
| `core/` | All the logic, with no interface and no ambient configuration. Tests need no network. |
| `src-tauri/` | The composition root (`lib.rs`, thin commands only), the orchestrator that owns every decision (testable with fakes), nzbget supervision, the record of what was downloaded, the finisher, the log. |
| `ui/` | One page, one stylesheet, one script. `preview.html` runs it in a browser with canned data. |
| `scripts/` | `dev.sh` runs the app; `build-linux.sh` and `build-windows.sh` produce releases. |

## Commands

```sh
just dev        # the app
just preview    # the interface alone, in a browser, no webview and no credentials
just check      # fmt, clippy with -D warnings, tests
just release    # an AppImage, in a pinned container
just windows    # the Windows installer, cross-compiled in the same way
```

Layout and wording belong in `just preview`. Rebuilding a desktop app to move a button is a slow way to find out it is still in the wrong place.

## When editing by matching text

Verify the edit landed, by grepping for what it should have added. That a file still parses proves nothing: a patch anchored on a line that formatting has since rewritten, or on text that appears twice, fails silently or applies in the wrong place. Both have happened here, and both looked exactly like the feature not working.
