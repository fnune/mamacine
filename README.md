# Mamá Cine

A desktop app for one person who wants to watch a film tonight, and who is not going to configure anything, read a release name, or find out what a par2 file is.

She types a title. The app finds a copy, downloads it over usenet, fetches Spanish subtitles when the copy has none she can read, and puts it on a shelf with a poster and one button that plays it. Whole seasons of television work the same way, with an episode list she can choose from. Everything else — which of nine copies is worth the bandwidth, whether the repair data is real, what to do when a copy arrives broken — is the app's job, and it is done without asking her.

The interface is in Spanish, because that is the language of the person it was built for.

## What it needs

Accounts are entered once, on the settings screen, and stored under the user's own account. Nothing is compiled in.

- A usenet provider (host, user, password).
- At least one indexer with a Newznab API, such as NZBGeek.
- An [OpenSubtitles](https://www.opensubtitles.com/) key and login, for subtitles.
- Optionally a free [TMDB](https://www.themoviedb.org/) key, which buys titles, synopses and episode names in Spanish. Without it the app falls back to keyless lookups (IMDb for titles, TVMaze for shows) and works with nothing configured.

[nzbget](https://nzbget.com/) does the downloading. It travels with the build, runs as a private instance on its own port with a random control password, and is supervised by the app.

## Building and running

[Nix](https://nixos.org/) provides the development environment. `nix develop`, then:

```sh
just dev        # the app
just preview    # the interface alone, in a browser, with canned data and no credentials
just check      # fmt, clippy with warnings denied, and every test
just release    # an AppImage, built in a pinned container
just windows    # the Windows installer, cross-compiled the same way
```

Releases are built in a pinned container rather than from the flake: a Nix-built binary links against store paths that exist on no other machine.

## Layout

| Path | What it holds |
| --- | --- |
| `core/` | Every decision, with no interface and no ambient configuration. The tests need no network. |
| `src-tauri/` | The composition root, the orchestrator that owns what happens next, nzbget supervision, the record of what was downloaded, the log. |
| `ui/` | One page, one stylesheet, one script. No bundler, no framework. `preview.html` runs it in an ordinary browser. |
| `scripts/` | Running the app in development, and the two release builds. |

`engineering/README.md` is what the code is held to: why configuration is a value rather than an ambient read, why every volatile dependency crosses a seam, and what the app is allowed to say to the person using it.
