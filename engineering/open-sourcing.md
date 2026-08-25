# Open-sourcing: findings and progress

Working checklist for readying the repository for the public, from the readiness analysis of 2026-08-25. Delete this file when everything is done, or fold what remains into issues.

## Status

| # | Item | Status |
| --- | --- | --- |
| 1 | MIT license, plus third-party notices for the bundled programs | done |
| 2 | CI running `just check` on a clean checkout; fix the stale-target breakage | done |
| 3 | Sidecar binaries fetched and verified at build time instead of committed | done |
| 4 | OpenSubtitles: handle 406 as quota, let 429 recover, stop `rebuild` discarding caches | done |
| 5 | A per-host floor under the interval between calls, at the `HttpClient` seam | done |
| 6 | Test the subtitle fetch-and-place path in `finishing.rs` with a fake that answers | done |
| 7 | Subtitle language and metadata locale become settings, not constants | done |
| 8 | Generalise the Spanish hard-coding in ranking, tagging and media inspection | done |

What the eight items became:

1. `LICENSE` (MIT), `THIRD-PARTY-NOTICES.md`, `license` in the crate metadata, a license section in the README.
2. `.github/workflows/check.yml`: `nix develop --command just check` on push and pull request, with a cargo cache. The stale-target breakage was a `target/` cache artifact; `cargo clean` cleared it.
3. `scripts/fetch-sidecars.sh` downloads nzbget's release installer, verifies it and the three 64-bit programs inside it against recorded checksums, and places them; `build-windows.sh` calls it, `.gitignore` covers `src-tauri/binaries/`, the committed binaries and the stale `pin-sidecars.sh` are gone from the tip.
4. `opensubtitles.rs` treats 406 as the day's allowance (blocking until the reset instant the service names, or a day), treats 429 as a fifteen-minute pause instead of a process-lifetime latch, and prunes expired search-cache entries. The client survives `rebuild` when its settings did not change, and the settings screen's check button reuses the running client's login token.
5. `Throttle` in `core/src/http.rs`: a per-host floor under the interval between calls, wrapped around every remote client at the composition root (250 ms for TMDB, 500 ms elsewhere; the local nzbget is exempt).
6. `finishing.rs` gained an `Answering` fake and tests over fetch-and-place: placement beside the film, numbered alternatives, hash-match economy, duplicate skipping, mid-season allowance exhaustion, and the refetch button.
7. `subtitles_language` and `tmdb_language` in the settings file, empty meaning the computer's own language (`system_language()` reads the locale; the pure parser is tested). Reachable through the settings-file door in the interface.
8. `Tag::{Dub(code), Variant(code), Dual, Subbed, OtherLanguage}` with a per-language scene-vocabulary table (English deliberately absent: ENG marks the original), `Preference::Language(code)`, generalised scoring in `films.rs`/`series.rs`, `MediaInfo::has_language`, and custom serde so her existing library records still read. The versions screen now says "Idioma desconocido" instead of claiming "Versión original" over a marker it cannot name, and the `por` false positive in language naming is gone.

Refined after review of item 8:

- Asking for a language now means "audio in that language, whichever variety": a LATINO or VFQ copy matches the es or fr preference instead of being excluded, and the named peninsular dub simply ranks 150 points first. The fall-through still keeps the chosen variety before switching.
- The `LATINO` marker is understood as naming a market, not a dialect (the name cannot tell the studios' neutral dub from a local voice), and the screen now calls it "español latinoamericano".
- Regional track tags are read and repeated instead of flattened: Matroska's BCP 47 element already won over the three-letter code, now proven by test; `MediaInfo::has_language("es")` counts `es-419`/`es-MX` as Spanish; the window shows "español latinoamericano" for `es-419` and "español (MX)" for `es-MX` rather than a raw code, and regional Spanish subtitles count as readable Spanish.

Deferred, deliberately:

- The IMDb suggestion fallback (an undocumented endpoint, currently the default provider). Decision postponed.
- Purging the committed binaries from git history. The tip no longer carries them (item 3), but the ten existing commits do; rewriting published history needs a force-push, which is a separate decision.
- ~~Translating the interface out of Spanish~~. Done since: the interface is internationalized (Spanish and English), follows the computer's language, and is selectable in Ajustes. Backend sentences live in `src-tauri/src/text.rs` behind `enum Lang` (the compiler enforces completeness); window sentences live in `STRINGS` in `ui/app.js`. Still untranslated: the technical `why` hover details in a download's story, and the two English `Error::Setup` sentences core produces about OpenSubtitles credentials.
- Persisting the poster, synopsis and search caches to disk so a nightly restart does not re-fetch the shelf. The in-memory caches now survive settings saves; surviving restarts is the next step.
- Knowing a film's original language. An unmarked release of a Mexican film says nothing, so the es preference cannot see that its original audio is already Spanish; TMDB's `original_language` could tell the ranking. Only matters when marked and unmarked copies of the same film compete, which is why it can wait.
- A preference for the Latin American dub over the peninsular one (the mirror of today's ranking). Both now match; which ranks first is fixed at castellano.

## The analysis, condensed

What blocks release is not the code. The architecture is publishable: every volatile dependency crosses a trait seam, the composition root is real, and 410 tests run offline in under two seconds.

1. **No license.** Without one the repository is all-rights-reserved. MIT for our code; the bundled programs (nzbget GPL-2.0, unrar, 7-Zip, ffprobe GPL) are driven as separate processes, so this is aggregation, not linking, and MIT is compatible. They need a notices file with source links.
2. **No CI**, while `engineering/README.md` claims "the same command locally and in CI". A stale absolute path from the `mama-cine` to `mamacine` rename also left `just check` red locally until a `cargo clean`.
3. **12 MB of Windows binaries committed to git.** They bloat every clone and carry redistribution obligations. `scripts/pin-sidecars.sh` already records checksums; fetch-and-verify at build time like `build-linux.sh` does for everything else.
4. **OpenSubtitles quota handling misses the actual quota signal.** 406 (daily quota exhausted) falls through as a generic refusal, reproducing the mass-refusal bug the 429 branch was written to prevent; a single transient 429 disables subtitles for the life of the process; and every settings save discards every cache and token, so the check button is a fresh `/login` each press.
5. **No floor under the interval between calls**, anywhere in the HTTP path, despite the stated principle. The only real floor is the 60 s news-server recheck.
6. **The subtitle fetch-and-place half of `finishing.rs` has no test coverage**: the fake returns empty for every query, and a `catch_unwind` turns any bug there into one log line.
7. **Subtitle language and TMDB locale are constants** (`"es"`, `"es-ES"`) although both are already plumbed as parameters end to end.
8. **Spanish is welded into the logic**: `Preference::Spanish` as an enum variant, `Tag::Spanish`/`Tag::Latino` regexes with every other language collapsed into one bucket, scoring sites keyed on them, and `MediaInfo::has_spanish()`. `finishing.rs` also ignores the configured language in one of its two checks.

Also noted for later: a legal-responsibility paragraph in the README (the user brings their own indexer and provider), CONTRIBUTING.md distilled from the engineering principles, platform honesty (no macOS build), the 651-line `App()` in `ui/app.js`, `check_settings` doing orchestrator-grade work in the untestable layer, and the silent failure writing the library file and the credentials-file permissions.
