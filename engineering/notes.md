# Field notes

What the systems this app depends on actually do, learned the hard way and kept here rather than as comments. Everything below is a fact about the outside world; if one stops being true, the code near it is what needs revisiting.

## nzbget

- It prefixes any path it judges too long with `\\?\`. Given a path that already carries the prefix, it reads the two leading backslashes as a network share and builds `\\?\UNC\?\C:\...`, which names nothing. So every path written into its config is spelled plainly (`settings_file::as_windows_writes_it`).
- On Windows it aborts on non-ASCII paths, which is why the bundled exe is copied to an ASCII folder for users with accented names (`supervisor::spelled_without_accents`).
- Its `testserver` RPC, verified against nzbget 26.2: eight parameters, the last being the certificate verification level (0 matches `CertCheck=no`); success is an empty result, failures come back as prose in the result.
- It reports download health in tenths of a percent, and answers "do you already have this?" by name against its own history, which forgets and miscounts; the app keeps its own record instead (`library.rs`).
- `DELETED/MANUAL` in a history status is somebody pressing cancel; `DELETED/COPY` is its duplicate check, which the app turns off but an older config on the machine may still have.
- Its Windows release installer bundles the `unrar.exe` and `7za.exe` it is tested with, which is where `fetch-sidecars.sh` takes all three programs from.

## Usenet

- Takedowns hit par2 repair articles too, often first: ten percent of repair data on paper collapsed to one percent in the field, and a copy was approved that could never be saved (`Nzb::effective_par`).
- Posts disguise arbitrary data as `.par2` to dodge scanners: the Joy season shipped ten fakes, nzbget found "nothing to par-check", and all damage was fatal. One fetched article answers whether the repair set is real before a byte of the release is spent (`par2::contains_packets`).
- Some posts ship no bare `.par2` index at all; every par2 volume repeats the set's vital packets, so the smallest volumes serve instead (`Nzb::par_index_segments`).
- Takedown holes spread through a post, so an evenly spaced article sample sees them and a random one is untestable (`Nzb::sample`).
- Three hundred sequential NNTP `STAT` round trips took a minute against a real server; pipelined in batches they cost one (`nntp::stat_conversation`).

## OpenSubtitles

- `/login` is limited to one request a second per address, and the token lasts 24 hours (the client keeps it 20).
- 406 on `/download` is the daily quota exhausted and names the reset in `reset_time_utc`; 429 is the per-second throttle. Conflating them once disabled subtitles until an app restart.
- The `User-Agent` must match the consumer name registered with them; they enforce it on search.
- The download link points at a plain file host and must not carry the api key or the bearer token.
- Their hash is the file size plus the first and last 64 KiB summed as u64 (`subtitles::movie_hash`).

## Subtitle timing

- A subtitle authored at 25 fps runs about 4% fast against a 23.976 copy: on a 208-minute film the last cue lands at 199 minutes, which hides inside the stop-before-the-credits tolerance, so only the fps the service reports catches it (`subtitles::frame_rate_factor`).
- A quiet film defeats coverage checks: The Red Turtle has twenty-eight lines across eighty minutes, the last at sixty-two. Judging it by runtime coverage throws away the only Spanish subtitle it has (`ENOUGH_CUES_TO_JUDGE`).

## Newznab indexers

- `t=tvsearch` with an empty `ep` means "the season itself, not one episode": thirteen packs for Gomorrah where a plain-text search answers a hundred single episodes. Some indexers read the empty `ep` as episode zero and answer nothing, hence the second rung without it (`indexer::season_packs`).
- `t=caps` costs no search hit and is how a key is validated.
- Asking by tvdb id answers with releases under every name a show was released as ("Money Heist" and "La casa de papel"), which no single name search sees.
- ureq's default `User-Agent` gets 403s from IMDb's image host and some indexers; the app's honest `MamaCine/1.0` does not.

## TMDB, TVMaze, IMDb

- TMDB hands out two credentials on one page: the v3 key travels in the query; the v4 read token is a JWT accepted only as a bearer header and answers 401 in the query. Whichever was pasted decides how it is sent (`tmdb::is_read_token`).
- TMDB's search order is not an answer: a collection with popularity 0.5 outranks the show itself at 25 when its name matches the letters better, so the name filters and popularity decides (`tmdb::best_show`).
- TVMaze's `singlesearch` answers with the one show it believes a name is; a 404 is an answer, not a failure.
- IMDb's suggestion endpoint takes the query as a path segment where `+` is a literal plus, names one primary title, and never says which title is the original.

## Containers and codecs

- The Matroska specification says an untagged track is English; releases omit the tag exactly when nobody set one, so the app reports unknown instead of claiming English. The BCP 47 element (`LanguageIETF`) wins over the three-letter one and is the only place regional varieties (`es-419`) appear.
- MP4's `mdhd` packs a language as three letters in fifteen bits, five bits per letter minus 0x60; zeroes unpack to nothing readable. `moov` sits at the front of streaming files and at the end of others, so both ends are read.
- A Segment length written across eight bytes once overflowed a shift and took the finishing thread down; every real file writes it that way.

## Scene naming

- `LATINO`/`LATAM` name the market a dub was made for, never a dialect: the release name cannot tell the studios' neutral Spanish from a local voice. The same goes for `VFF`/`VFQ` in French.
- An unmarked or `ENG`-marked release is the original, not a dub; tagging English would call every ordinary copy foreign.
- Bare `por` is an everyday Spanish word and appears in film titles; it never marks Portuguese.
- `VOSTFR` is original audio with French subtitles, a French-market marker rather than a dub.
- "Gomorra" is the Italian name and "Gomorrah" the release spelling; a one-or-two-letter tail is the same word, but "star" must not match "stargate" (`search::same_word`).

## Building and shipping

- A Nix-built binary links against store paths no other machine has, and Nix's webkit cannot reach a non-NixOS host's GPU drivers; releases are built in pinned containers against an ordinary distribution's libraries, and `just dev` goes through nixGL.
- `mksquashfs` refuses `SOURCE_DATE_EPOCH` from the environment and its own flags at once, which is why the Linux build passes timestamps by flag.
- NSIS runs natively on Linux, so the Windows installer is cross-compiled; WiX MSI cannot.
- tauri-cli's tray-icon feature probes for appindicator on the build host even when targeting Windows, so the cross-build container installs `libayatana-appindicator3-dev`.
- rustls's `ring` provider is pinned because `aws-lc` does not cross-compile to the Windows target.
- On Wayland, pinning `GDK_BACKEND=x11` routes every frame through XWayland and scrolling visibly stutters; the backend follows the session instead.
- Windows tray tooltips truncate at 127 characters (`orchestrator::TRAY_TOOLTIP_LIMIT`).
- `explorer.exe` answers exit code 1 whether or not it opened anything, so on Windows its exit code is ignored (`lib::open_with_desktop`).
- The dialog plugin uses the portal backend so the desktop's own file chooser appears on KDE instead of GNOME's.
