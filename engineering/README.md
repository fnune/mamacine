# Engineering principles

Principles and shared conventions for Mamá Cine. Not feature-specific, and not a changelog: anything written here should still be true a year from now.

---

## Principles

### Calm app

This app is built for one person who is not technical and will not read documentation. That constraint is the design brief, not a limitation to work around.

- Simplicity is the default. A feature earns its place or it does not ship.
- Plain words, not domain vocabulary. She should never meet "nzb", "par2", "articles", "health", or "retention". The words are ours, not hers.
- One screen at a time. Nothing that matters may sit below the fold, and an action must answer where it was made.
- Decisions we can make, we make. Choosing between two releases of the same film is our job, not hers.
- Failures are shown calmly and accurately. "This copy is incomplete, try another version" beats a status code, and both beat a spinner that never resolves.
- An unknown is reported as unknown. It is never rounded into a claim.
- A finished download is good news. Only a failure is styled as one.
- Answer the question she actually has: can I watch this, in a language I understand?

### No ambient configuration

Configuration is a value. `Settings` is built once at the composition root and passed down. No module reads the environment, a file, or a keyring on its own; a function that needs the API key takes it as an argument.

This is not a style preference. The prototype read credentials through a global helper called from deep inside the logic, and when the app began reading its own `.env`, forty-one tests silently switched to hitting the real accounts. Nothing in a signature said those functions touched configuration, so nothing warned. A compiler can enforce what discipline did not.

### Testability through seams

- Every volatile dependency crosses a boundary that can be substituted: the network, the clock, the downloader, the subtitle service, the indexer. The real implementations are constructed only at the composition root, so a test cannot reach a real service by accident.
- Fakes are hand-written and live beside the code they stand in for. No mocking framework.
- Most of the interesting behaviour is pure functions over values: ranking, rescaling, timing checks, release tagging, film grouping. Those need no fakes at all, and that is the point.
- Parsing is separated from doing. A configuration file is rendered by a pure function and written by the caller; a program's output is parsed by a pure function and the program is run elsewhere. The half that decides is the half under test.
- A test that would pass with the feature removed is not a test. When fixing a defect, first confirm the new test fails against the old behaviour.
- Tests state behaviour in their names: `a_foreign_parts_only_subtitle_is_never_offered`, not `test_rank_2`.

### Layering

- `core` holds the logic and declares the abstractions it needs. It has no interface, spawns no processes, and opens no files of its own.
- `src-tauri` is the composition root and the boundary: it decides which concrete implementation is which, supervises the downloader, and exposes commands to the window.
- `ui` is one page with no bundler and no framework, so it can be opened in an ordinary browser and worked on directly.

### Credentials and privacy

- Credentials are entered by the person using the app and stored under their own account. Never committed, never compiled in. A binary handed to someone else ships empty and asks.
- Anything written to disk that carries a password is readable by its owner alone.
- A key is sent to the service it belongs to and to nothing else. A download link that points at a plain file host is fetched without it.
- The window never talks to a third party directly. Requests go through the app, which refuses any host that is not the one configured.

### Dependency hygiene

- Vet every dependency: can the standard library do it, how large is the transitive tree, is it maintained.
- If it is under 50 lines, write it. The base64 encoder, the feed date parser and the subtitle hash are all here because each is shorter than the argument for taking a crate.
- Weigh what a dependency costs the person installing the app, not only the person building it. A 141 MB download to read two fields out of a file header is not a trade worth making.
- External programs the app drives are pinned by checksum and travel with the build that needs them.

### Respect for the services we depend on

- Every remote call is metered by somebody. Cache what does not change, never ask twice for what was already answered, and keep a floor under the interval between calls.
- Use the cheap endpoint when it exists. Validating a key is not a search.
- Identify the client honestly. Never impersonate a browser to get around a filter.
- Track what an allowance has left and stop when it is gone, rather than hammering for refusals.

### Error handling

- No silent catches. Every swallowed error either recovers meaningfully or records why it gave up.
- Errors carry what was attempted and what refused. "Download quota reached" and "no subtitles found" lead to different actions and must not look alike.
- The boundary decides what the person sees. Deeper layers return an error and trust the boundary to phrase it.

### Code comments

- Code is self-evident through naming and structure. Comments are a last resort.
- A comment is justified only by a non-obvious why: a hidden constraint, a subtle invariant, a workaround for a specific failure. The line that deletes a bundled library is worth a sentence; the line that opens a file is not.
- Comments that restate what the code does are noise, and are deleted on sight.

### Local checks before pushing

- `just check` is formatting, clippy with warnings denied, and the tests. The same command locally and in CI.
- Clippy warnings are fixed, not tolerated. Staying at zero is cheap only while it is zero.

### Toolchain choices

- Rust and [Tauri](https://tauri.app/), with the toolchain pinned in `rust-toolchain.toml`.
- The interface is plain HTML, CSS and JavaScript. No bundler and no framework: it is small enough not to need one, and staying that way is what lets it be opened in a browser and looked at.
- [nzbget](https://nzbget.com/) does the work no one should reimplement: connection pooling, decoding, repair, unpacking. It runs as a private instance with its own configuration, its own port and its own random control password.
- [Nix](https://nixos.org/) provides the development environment and nothing else.
- **Releases are built in a pinned container, never from the flake.** A Nix-built binary links against store paths that exist on no other machine. The container carries an ordinary distribution's libraries, which is what a portable bundle has to be built against. Everything it fetches is pinned by version and verified against a recorded hash.

### Writing style

- Sentence-case in headings.
- Bold sparingly.
- No em-dashes. Use a colon, comma, semicolon, period, or rephrase.
- Markdown is not hard-wrapped: one line per paragraph and per list item.
- No AI attribution in commit messages or code.

---

## Use of AI and LLMs

This project accepts the use of LLMs to write code, held to the same standard as code written without them.

- Tests at the unit and end-to-end level, against real processes where practical.
- Claims verified against evidence from the machine in front of you, not asserted from general knowledge. If it can be checked, check it before acting on it.
- Corrections stated plainly when the evidence contradicts an earlier claim.
- A script that has not been run is not finished, and saying so afterwards is not a substitute for running it.
