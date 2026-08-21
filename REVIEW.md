# Review: where the app stands and where it fails

A full read of the codebase (2026-08-21), with every claim verified against the code rather than the docs. Supersedes NEXT.md, TASKS.md and BUGS.md, whose keepable content is salvaged at the bottom.

## Status after the fix round (same day)

Everything in this file was addressed in one pass, except the items listed under "deliberately not done". The line references in the findings below describe the code as it was when the review was written; the findings themselves are all fixed and each fix carries a test:

- Findings 1 to 10 and all the smaller ones: fixed. The settings pipeline applies on save and parses tolerantly; the orchestrator extraction happened (`src-tauri/src/orchestrator.rs`, all trait objects, 39 backend tests); failures map to Spanish at the boundary (`messages.rs`); the chase is capped at 3 and checks the news account before blaming copies; the retry flash is gone (`retrying` on `Finished`); a dead nzbget is a calm banner; there is a rotating log (`log.rs`, `mamacine.log` beside `library.json`); single instance and double-grab are guarded; the disk verdict is computed server-side by the one rule.
- UX section: done. OS notifications on ready and on giving up; delete from the shelf (recycle bin via `trash`); in-app episode list; one search box with lookup-as-she-types (IMDb keyless endpoint, `core/src/lookup.rs`); language preference moved to settings; room and time in words; story collapsed behind "qué ha pasado"; footer only when the disk is tight; light/dark follows the OS; technical settings behind a fold.
- Salvage section: MP4 parsing done (`core/src/mp4.rs`). The rest of the salvage remains as recorded.

Deliberately not done, in the open list below: the updater, Windows Credential Manager, signing/SmartScreen, and the real-Windows installation test.

## Driven against the real services (same day, second pass)

`just probe "<query>"` replays the exact search pipeline against the real indexer and prints the grid in the order the window would show it, with every response cached on disk (`target/probe-cache`) so a repeated query costs the services nothing. `--series`/`--film` mirror the suggestion-pick intent, `--suggest` hits the title lookup, `PROBE_LANG=es` applies the Spanish preference. Queries driven: game of thrones, das boot, coco, el sur, campeones, roma, el secreto de sus ojos, el espíritu de la colmena, cuéntame cómo pasó, los serrano, la casa de papel / money heist, plus suggestion runs including misspellings and Spanish names for English-named content.

Found and fixed, each with a test citing the real listing:

- Picking a "serie" suggestion still searched films and showed them first, burying the seasons (the reported bug). A suggestion now carries its kind, and the other category is not asked.
- No relevance ordering: the seasons of Game of Thrones sat below a parody, a documentary and an episode review. Results are now ordered by how much they look like the query (`search::relevance`), and results sharing no word with it at all (Tinker Bell for "el sur") are dropped.
- "Game Of Thrones Complete" and "Money Heist 2017" appeared as duplicate shows: packaging words and release years before the season marker are now stripped from show names.
- "Coco Sing-Along" carried Coco's own IMDb id and sat in the group as an ordinary fallback copy; sideshow markers (sing-along, making-of, soundtrack…) now sink a release.
- A German.DL season pack sat in the Spanish fall-through; a dub into a third language now sinks under the Spanish preference.
- Accented queries matched nothing: releases are filed in scene ASCII, so queries are diacritic-folded before searching and matching (language-agnostic).
- "campeones" found the film but the card scored zero relevance because the film database titles it "Champions": release names now get a vote in relevance.
- An empty answer to a suggestion-picked title said "prueba a escribirlo de otra manera", blaming her spelling for the indexer's catalogue; it now says the title exists but is not carried.

Learned, not code:

- The suggestion → IMDb-id path is the load-bearing one. "roma" as text finds only romance junk; Roma by id finds 20 releases and the Spanish preference picks `Roma.2018.Spanish`. "El espíritu de la colmena" as text finds nothing in any spelling; by id it is "The Spirit of the Beehive", 4 releases, sane pick. Relatos Salvajes by id picks the `Relatos.Salvajes MULTi` release under the Spanish preference.
- This indexer carries no Spanish-television packs at all: Cuéntame, Los Serrano and El Ministerio del tiempo are simply absent, while La casa de papel exists as Money Heist and is found through the suggestion flow. A second, Spanish-focused indexer is the real fix, and the multi-indexer support already exists for it.
- Display titles come back in the film database's language ("Champions", "Wild Tales", "The Spirit of the Beehive"), which argues concretely for the TMDB item below: Spanish titles and summaries at display time.
- Cover art comes from `api.nzbgeek.info` itself, so the sister-host allowance is confirmed safe headroom.

## The calm-app sweep (same day, third pass)

"Hay sitio de sobra" hid the size, the free space and the disk behind a verdict, and it was not alone. Every user-facing sentence in the app was audited against one rule: show the real information and let her decide; a verdict may accompany the facts, never replace them. Fixed:

- The decision screen shows what it occupies, what is free, how big the disk is, and how long it will take. The warning is additive and carries its own number: the working room a download needs while it unpacks (the real reason behind every refusal). Descargar is never disabled; only the backend's refusal, with numbers, says no.
- Giving up states the counts: "he probado 3 copias y todas venían dañadas; quedan 4 sin probar".
- "Otras copias" no longer silently caps at ten, and the button says how many there are. The per-copy download counts are back on the rows.
- Quality words carry their resolution: "Alta definición (1080p)", "Buena calidad (720p)".
- The free-space footer is always visible, free of total; running low adds urgency and the action, never replaces the numbers. The paused-for-disk hint carries the free number.
- The story's "otra versión" line carries the new copy's size; a skipped too-big copy names its size.
- Per-indexer search failures say what actually happened to each (key rejected vs quota vs unreachable): `gather` now carries the structured error out instead of a pre-flattened string.
- "Guardado. Ya se puede buscar" is no longer said when the rebuild on the new settings failed; the failure is reported.
- "Comprueba que internet funciona" about nzbget (a local process) is replaced by the action that helps.
- Play/open/reveal/episode buttons no longer swallow errors.
- "Se ha quitado del ordenador" now says papelera, because recoverable is a fact she should have.
- Sizes use the es-ES decimal comma ("1,7 GB").

Kept deliberately, on record: the discard line ("Esa descarga venía dañada…") stays without article counts, because mid-download part-counts were the original complaint that started the story mechanism; the counts stay in the tooltip and the log. Browse cards stay minimal (title, year/season); the decide screen carries the full facts.

## Localized titles (same pass)

The rule: show the localized title with the original in parentheses exactly when both are available and the original is known outright, never guessed. TMDB is the one source that states `original_title`, so it is now the suggestion provider when a TMDB key is configured (Ajustes técnicos → Fichas de películas; free key from themoviedb.org): suggestions arrive in Spanish with the original beside them, picking one resolves to the IMDb id (films) or the international name (series, which is what season packs are filed under), and the picked name follows the film through the cards, the library, the shelf and the notifications. Without a key the keyless IMDb lookup remains, which never claims an original. Untested against the live TMDB API (no key on this machine): create the key, paste it, press Comprobar.

## Found live, fourth pass: the vanished download

The running (previous-build) app sat on "Empezando la descarga…" forever. The library believed copy 5 of 7 was downloading; nzbget had never heard of the id — not in the queue, not in history (a rejected nzb, or a lost queue directory). Neither the old build nor the new one handled an id that exists nowhere: the poll found it in neither `active` nor `finished` and the screen froze on its placeholder. Fixed with a test: the chase now treats an id nzbget knows nothing about as a dead copy (story line "se ha perdido por el camino", fall-through to the next), and `progress` synthesizes the missing row so the window can follow the chain instead of waiting on a ghost. The same live session also showed the pre-fix chase discarding a `WARNING/HEALTH` season (9.1 GB, ~98% health) that today's rules count as hers — that fix was already in, now confirmed against a real casualty.

## Surviving updates (fifth pass)

The vanished-download incident was an update meeting yesterday's state. The rule now built in: every piece of persisted state is one of three kinds, each with one law.

- **Derived state** (nzbget.conf): regenerated on every start, never migrated. Was already true.
- **Claims about the world** (what is downloading, what is on the shelf): never trusted across a restart. Reconciled continuously — `present()` against the disk, the chase against nzbget's queue and history, including ids nzbget has never heard of.
- **Records** (`library.json`, `settings.json`): version-stamped. An older file is migrated stepwise with the original kept beside it (`library.v1.json`), so a migration bug can never cost her shelf. A **newer** file — a downgrade — is refused whole and said plainly ("los datos son de una versión más nueva"), and nothing may write over it: the old behaviour was `.ok().unwrap_or_default()`, which silently turned a newer or corrupt file into an empty library and then overwrote it on the next save. A corrupt file is set aside as `*.broken-<epoch>.json` for whoever debugs it, never replaced with nothing.

The enforcement is a pinned fixture: `src-tauri/fixtures/library.v1.json` is a real version-1 file, and a test opens it and checks the migration end to end. Every future schema change must bump the version, add its migration step, add its own fixture, and keep the old fixtures passing. nzbget's queue directory migrates itself across the bundled nzbget's own versions.

This principle (derived / claims / records) is a candidate for `engineering/README.md`, which is yours to edit.

## Found live, sixth pass: the terminal verdict that was neither true nor actionable

The give-up screen blamed the server ("no consigo conectarme al servidor de descargas") while Eweka was fine. The log named the culprit in one line: nzbget 26.2 answered `Invalid parameters` about our own `testserver` call (it takes eight parameters now), and the classifier read an error about *our call* as an answer about *the server*. Worse, the design treated a server problem — inherently transient — as terminal, and the screen gave an instruction with no door.

Fixed, all tested:

- `testserver` speaks nzbget 26.2's real dialect, captured live: eight parameters, success is an empty result, auth refusal and connection failure are distinct prose. Any RPC-level error is `Unknown` — never an answer about the server. The false-positive shape from the incident is a pinned test case.
- A server problem no longer consumes the film. The chase stalls the affected downloads, says so once (story + notification + banner, with "sigo yo solo" so she knows nothing is asked of her), and rechecks every minute; the moment the server answers, it resumes by itself and writes that down too. Refused-account and unreachable-server get their different sentences and remedies.
- The give-up message counts what was actually tried (`entry.attempt`), not what was planned: "he probado 3 copias" had been said after one.
- The copies beyond the chase limit are now kept, not discarded, and the give-up screen offers "Probar más copias (quedan N)": one button that continues the chase under a fresh allowance, numbering intact, her decision written into the story. A gave-up film never resumes silently on restart; only the button resumes it.

## Found live, seventh pass: presentation must follow decisions, not bookkeeping

The screen flashed "No he podido conseguirla" and then carried on. Cause: `retrying` was derived from the in-memory attempts map, which goes empty for the seconds the chase spends fetching the next copy — a poll landing in that window saw "failed, no successor". The rule now: **a film reads as failed only once the library records the decision** (`gave_up`, written atomically); internal bookkeeping never drives presentation. Three companions, found by walking the whole watching lifecycle under that rule:

- An undecided failure nobody is handling (crash window, older build's state) would read "working on it" forever, so the chase now adopts orphans: any non-deliberate failed copy with a known, undecided library entry gets decided on the next tick. "Retrying" is provably temporary.
- The bar froze at the dead copy's percent during "buscando otra versión", then jumped to zero — the original "bar went backwards" complaint in miniature. It now empties exactly once, as the words change.
- While the server is down, "buscando otra versión" under the server banner was a small lie; the status now says "Esperando a que el servidor vuelva…".

All pinned by tests, including one that reproduces the mid-decision window itself.

## Eighth pass: the tray, and what the window no longer owns

She asked for the app to live like the clock does. The window is now a view, not the app: a tray icon (left-click opens, "Salir del todo" quits honestly), a mom-level "al cerrar la ventana, seguir con las descargas" switch (on by default — a download she started must not die because she tidied a window away, and a notification says where the app went when something is still coming down), an "abrir sola al encender" autostart switch (the OS entry is derived state, re-asserted on every start and save), and a tray tooltip that answers "is anything coming down?" without opening the window. Quitting fully stops nzbget; and because a *killed* app used to orphan its nzbget — downloading unseen, wedging the next start — the supervisor now writes its own pidfile and reclaims the orphan at startup, with a name check so a reused pid is never killed (that test is itself the innocent process).

Also found by use, all fixed with tests: a suggestion lookup in flight when she pressed Enter landed afterwards and reopened the popover over her results (in-flight answers are now generation-invalidated, not merely debounce-cleared); a fresh window said "no se está descargando nada" over a film mid-chase and the detail screen offered Descargar for a season downloading at that moment (the poll adopts mid-chase films; `have` answers on-the-shelf *and* on-its-way, and the detail says "Ya se está descargando" with "Ver cómo va"); the give-up sentence was said twice in a row above the fold.

## The Game of Thrones verdict, from the provider's own log

The index has Temporada 1 — eight listings. nzbget's log for the failed attempts: 2,695 × `430 No Such Article` and zero connection errors, with the decision line "health 93.9% below critical 94.0%". The articles have been taken down on the provider (GoT is the most DMCA'd content there is) and these packs carry ~6% par2 — a tenth of a percent short of repairable. The app's message is accurate; nothing here is a bug. The structural fix is the long-listed backlog item, now with evidence: **a second provider on a different backbone as a fill server** — takedown holes rarely overlap across backbones, and the multi-server config support (`Server2.*` as Level 1 fill) is a small change to `render_config` plus a settings row.

## Ninth pass: ask the server before downloading

"Why does it take so long to see the health is bad?" Because nzbget can only prove an article missing by attempting it, and with takedown holes spread evenly, certainty of "beyond repair" arrives only after ~95% of the volume — nine gigabytes and twenty minutes per dead copy, in the field. The app now asks first: it holds the full nzb before appending, so it samples ~300 message-ids with pipelined NNTP `STAT` against the news server (seconds, kilobytes), reads the par2 coverage out of the nzb itself, and skips a copy whose loss clearly exceeds its repair headroom — silently, because nothing visible changed; the numbers go to the log. A skip costs none of the chase allowance (the cap bounds bandwidth, and a skip spends none), the verdict is deliberately conservative (uncertain goes to nzbget, whose slow answer remains ground truth), and any probe failure steps aside rather than becoming a new way to fail.

Proven live against the exact pack that ate the morning: the PSA copy is 100% scrubbed from the provider (skipped in one batch), the 720p is 16% gone (skipped), while the Joy copy sits at 1.7% missing against 10% par2 — the probe hands nzbget the copy that will actually finish. Found along the way, all fixed with tests: real nzb files carry a DOCTYPE that roxmltree rejects by default; sequential STATs took a minute where a pipelined batch takes a round trip (with early exit once the verdict cannot change); the in-flight plan was not carrying the attempt count forward, so the chase cap could be evaded via the vanished-copy path; and a download started within a chase tick could be judged "vanished" before it had time to exist anywhere.

## Tenth pass: burying the corpses

"What happens to failed downloads?" Their partial gigabytes were quietly kept: nzbget parks a failed item's files in its hidden work folder (`nzbget/inter`) until somebody deletes the history entry, and nothing ever did — 48 GB of dead Game of Thrones attempts, invisible to her, counted against the free space the footer reports. Failed downloads never reach her films folder (only finished films are moved there); the leak was internal. The chase now forgets (`HistoryFinalDelete`) every copy it discards — files deleted the moment the copy is replaced or given up — and the same loop sweeps decided corpses left by older builds, so the 48 GB clears within a tick of the next launch. The library and the synthetic rows already carry the whole chain, so the screen loses nothing. A film stalled behind a server outage keeps its history row until the outage resolves, then is buried a tick later.

## Eleventh pass: the disk drawn, not narrated

"445 GB libres de 1861 GB" asks her to divide. Everywhere the free/total relationship appears it is now a small capacity bar — used space filled, the number beside it, the percentage on the tooltip — with the right item in each place: the footer carries the whole-disk bar (turning warn-coloured with the action sentence when low), and the decide screen's bar additionally draws the download's own slice onto the disk, so "will it fit, and with how much margin" is visible instead of arithmetic. Plain sentences (the paused-disk hint, the disk-full refusal) stay sentences, because a bar inside prose is decoration. Single quantities ("Ocupa 1,7 GB") stay numbers — they were never the problem.

## Twelfth pass: the copy that fooled the probe, and the memory that outlives it

"It got discarded 6 minutes in!" The probe had approved the Joy copy honestly: 1.7% of its data missing, all ten par2 volumes present on the server. nzbget's log held the missing piece — "Nothing to par-check": the par2 exists but covers none of the damaged files, so its effective repair capacity for this damage is zero, and nzbget (correctly) cancelled at the earliest moment its math allowed (~4 GB instead of 9). File-level par coverage is invisible to any remote probe.

Two fixes. The probe now samples the **par articles** too and counts only surviving repair data — proven live: the PSA copy's par is 100% taken down and the 720p's 20% (both now skipped in a second), while Joy's is intact (which is exactly why only nzbget could refuse it). And because the downloader's verdict is ground truth the probe cannot always predict, **a copy nzbget refuses is burned**: remembered by its nzb address in the versioned library (v2 → v3 migration, tested), skipped silently forever after, across sessions and re-grabs — a proven corpse never costs bandwidth twice. When every remaining copy is burned or visibly incomplete, the answer is one honest sentence: "Ninguna de las copias que quedan funciona ahora mismo."

## Handoff: language chips on the detail screen (backend ready, UI pending)

The per-search language chips are coming back as per-film chips on the detail screen, showing what *this* film actually has. The backend contract is in place and tested; the UI half is left for the ongoing UI overhaul to implement:

- Every `Version` now carries `voice: "es" | "latino" | "original"` (derived server-side from release tags; Dual counts as "es", the same bet the ranking makes). Build the chips from the distinct voices present, in that order, labelled "En español", "Español latino", "Versión original" — a chip only exists if copies exist in it.
- Selecting a chip selects the first version of that voice (versions are already quality-sorted) and everything downstream follows: the "Se descargará en…" line, size, room meter, minutes, and `grab`'s `version` index. Default selection: her Ajustes language when that voice is available, else the backend's chosen copy's voice.
- When no "es" voice exists, show instead of the chip (id suggestion: `#no-spanish`, calm not red): **"No está en español. Se puede ver en versión original, y le buscaré subtítulos en español."** — a true promise, the finisher fetches them. Hide chips in the ya-la-tienes / ya-se-está-descargando states.
- The fall-through already honours the choice server-side: `candidates_from` orders a dead copy's successors same-voice-first, so a Spanish pick never silently continues in English (tested: `the_fall_through_keeps_her_language_before_it_keeps_the_ranking`).

## Thirteenth pass: front-loading the repairs

"Can't we front-load the repairs, or lower the threshold?" The threshold cannot move — it is the arithmetic point where damage exceeds spares and only a broken file exists past it. But front-loading the repair *knowledge* proved out completely, and unmasked the Joy mystery: hexdumping a fetched repair article showed its ten "par2" volumes are **disguised data, not par2 at all** — a scanner-dodging trick — which is why nzbget found "nothing to par-check" and burned gigabytes twice on 1.7% damage.

The probe now fetches one small repair article (NNTP `BODY`, yEnc-decoded — both written here, ~40 lines each, tested against scripted servers) and checks the repair data is *real* (`par2::contains_packets`). A copy with damage and fake repair files is skipped before a byte of the release is spent; verified live: "Joy → par2 is FAKE → SKIP" in seconds. Name-based coverage checking was built, tested live, and deliberately **rejected**: obfuscated posts routinely name their inner files differently (the 720p's par covers `got1sea.part*` while its posted names differ entirely), so name matching would produce false skips — nzbget resolves those by content hash, which no remote probe can. The one remaining false-pass class (real par2 covering only some episodes, the German.DL case) stays with nzbget as ground truth, bounded to one attempt ever by the burn list.

## Fourteenth pass: the season search asked a question packs could not answer

Reported live: picking the Gomorrah series answered "existe, pero ahora mismo no está en los sitios donde busco", while the same NZBGeek account has carried all five seasons for years. The television query appended the word "complete" to the name and searched it as text, and `q=` matches the release name literally: `q=gomorrah+complete` returns one release, a mislabelled BluRay disc, because season packs are named `Gomorrah.S02.1080p.BluRay.x264` and contain no such word. The workaround had been measured once against a show whose packs happen to carry it, and generalised.

What newznab actually offers, and what Sonarr uses (`NewznabRequestGenerator.cs`, read for this pass): `t=tvsearch` with an id, and `season`/`ep` as the unit of the question. Measured against the real account, per show, one request each:

- `t=search&q=gomorrah+complete&cat=5000` → 1 release, and it is not a season.
- `t=tvsearch&q=gomorrah&ep=&cat=5000` → 13 releases, every one a pack. The empty `ep` is what says "the season itself, not one episode of it".
- `t=tvsearch&tvdbid=281342&ep=&cat=5000` → 12 packs, all five seasons plus `S01-S05`, and no sibling show: the name form also drags in *Gomorrah: The Origins*.
- La casa de papel, by name: 14 packs. By tvdb id: **45**, because the id finds the show under every name it was released as, and most of its copies are named *Money Heist*. A Spanish name search never sees them.

So the search now asks by identity. A picked show carries its ids (`indexer::ShowIds`): TMDB states `tvdb_id` in the same answer as the international name (`append_to_response=external_ids`, still one request), and the keyless path gets them from TVMaze (`core/src/tvmaze.rs`, free, no key, one request per pick), which is what keeps this working before anything is configured. The indexer sends whichever id the site's own `t=caps` says it accepts, in Sonarr's order of preference, asked once per run and remembered; an indexer that accepts none is left on the name, and an indexer that answers nothing to the empty `ep` is asked again without it. An id and a name are never mixed: falling back from "this show" to "this spelling" would file another show's packs under the name she picked.

Because an id search answers with one show, its packs are one card per season under the name she picked, however they were named. "La casa de papel · Temporada 1" is now eleven copies deep instead of two cards of one, and the relevance filter is switched off for it: there is nothing left to second-guess about a show the indexer identified.

Two defects fell out of reading the real listings:

- `Game.of.Thrones.S01.EP01.2160p...` was read as a whole season. Twenty of the hundred results for that show are named that way, each a single 4 GB episode, and each was being offered as an evening of television. `shape_of` now reads `EP01` as the episode it is.
- Season zero (extras, bloopers, unaired pieces) was offered as "Temporada 0". Dropped.

Verified with `just probe --series`: Gomorrah (5 seasons + the complete-series pack), La casa de papel (5 seasons, Spanish and English copies in the same cards), Game of Thrones (8 seasons, no specials, no `EP01` impostors). Cuéntame cómo pasó still answers nothing, by id and by name both: this indexer genuinely has no packs for it, which is the honest "no" the message was written for.

## Fifteenth pass: a spelling thrown away, and what a season actually holds

Two things found by using the fixed search.

**"Gomorra" showed two seasons where "Gomorrah" showed five.** Not the indexer: asked either way it answers with both spellings' packs, and with more for "gomorra" than for "gomorrah". It was `search::relevance`, which compares whole words: "gomorra" scored zero against every pack named `Gomorrah.Sxx`, and the only survivors were the two seasons that happen to have an Italian-named release in their group. A word now matches a word that is the same but for a tail of one or two letters, which is the letter a language adds or drops (Gomorra/Gomorrah, casa/casas) and not a different word that merely starts the same: "star" still scores zero against Stargate. Her spelling exactly still outranks the variant. Picking the suggestion was already immune, because an id search does no name matching at all.

**"Son varios episodios" was less than the app knew.** A season card now says how many episodes it holds and names them, from the same show database that identified the show: TVMaze when there is no TMDB key (one request per show, and it answers for every season at once), TMDB's season endpoint when there is, so the names arrive in Spanish. A pack of several seasons is answered with the count alone, from one request in both providers: fifty names whose numbering starts over five times is not a list she can read. A season the search could not identify says nothing rather than guessing, exactly as a film without a synopsis does.

IMDb keeps no page for a season, so the season's button opens the show's episode list at that season (`/title/tt0302447/episodes?season=1`). A link per episode is not on offer: TVMaze states no IMDb id per episode and TMDB states one only at the cost of a request per episode, and neither is worth a screen that already names them.

## Sixteenth pass: the subtitle line that meant nothing, and the button behind it

"Sin subtítulos en español en 2 de 12 episodios." Which episode, and what can she do about it? Neither question had an answer, and the number was not true either. Her Gomorrah folder holds a Spanish subtitle for all twelve episodes, and between two and four copies of each: `...es.srt`, `...es.2.srt`, `...es.3.srt`, `...es.4.srt`.

The app never looked at the subtitles it had written itself. `subtitle_note` asked two questions, whether the file carries a Spanish track and whether the *pack* shipped one, and the file it fetched last week answered neither: `save_subtitles` writes `<name>.es.srt` and records nothing anywhere. So every "Buscar subtítulos" downloaded all twelve episodes again, and the count it reported was how many downloads that attempt failed to make, not how many episodes were without subtitles. Three clicks, three different numbers in the log: 1 de 12, then 4 de 12, then 2 de 12, with forty-nine `429 Too Many Requests` between them. The day's allowance went on subtitles she already had.

What changed:

- **What is already there is the record.** `media::subtitle_beside` asks the folder: a file named after the episode, in her language, beside it, which is the only thing a player loads anyway. It survives a restart, a hand-dropped file and a deletion, where bookkeeping would not. Nothing is fetched for an episode that has one.
- **The allowance is tracked, not discovered twelve times.** A 429 from `/download` now sets the remaining count to zero, so the client refuses locally instead of asking again; the season stops after the first refusal instead of spending one per episode, three candidates deep. "Track what an allowance has left and stop when it is gone" was already the rule here; it was written against a counter the service only returns on success.
- **A refusal is said as itself.** "El servicio de subtítulos no deja descargar más por hoy. Mañana se puede volver a intentar" is a different sentence from "no hay subtítulos", and only one of them is worth acting on.
- **The line names the episodes**: "Faltan los subtítulos del episodio 7", or "de los episodios 4 y 7". Above three, it goes back to a count, because naming ten of them is a paragraph and half a list is worse than a count: the ones it leaves out are the ones she would go looking for.
- **The episode list marks them**, which is where the question is actually asked: she is choosing tonight's episode, and the one with nothing to read says "sin subtítulos" beside its name. Computed from the folder each time it opens, so a file she added by hand counts.
- **The button answers with what it changed**, not with the sentence already on the card: "Ya estaban todos los subtítulos" (and it asked nobody), "Ya están los subtítulos del episodio 4. Todavía faltan los del episodio 7", or the allowance sentence.

Checked against the real folder: all twelve episodes are now seen to have Spanish subtitles, so the card reads "Subtítulos en español en todos los episodios" and the button spends nothing.

Left alone deliberately: the duplicate `.srt` files already on the disk. They are her files, and the app that made the mess should not be trusted to delete things without being asked.

## Seventeenth pass: the name she knows is not the name the release carries

"Game of Thrones" found nine seasons; "juego de tronos" found nothing. Verified against the real indexer before touching anything: NZBGeek answers "juego de tronos" with five behind-the-scenes clips from a Spanish fan site and no packs, and answers "los simpson" with The Simpsons, every one of which `search::relevance` scored zero against her words and threw away. Two separate failures behind one empty screen, and both only happen when she types the name and presses Buscar rather than tapping a suggestion: a tapped suggestion has always arrived in the name the indexer knows.

Matching a name in any language to the one thing it names is exactly what the title providers do, and the app was already asking them on every keystroke and then not using the answer. So a name she typed herself now goes through them before it is asked of an indexer:

- **The translation is a second question, never a replacement.** Her words are still searched as she typed them; the answers join, deduplicated by `Gathered::absorb`. A provider guessing wrong costs noise the ranking already filters, and can never take her own words off the table.
- **One translation per kind**, because a typed name is asked of both films and television, and translating only one leaves the other still asking in the wrong language.
- **Nothing is asked twice.** The window fetches suggestions on every keystroke and submitting asks the same question again, so the suggestions are kept with the text they answer and reused. A name that is already the name releases carry is not resolved at all: the check is against the original where the provider states one, since a provider answering in her language names the very thing she just typed.
- **Relevance is scored over every name the title goes by**, which is the half of the fix that "los simpson" needed: the releases are all named "The Simpsons" and scoring her words alone discarded the show she asked for.
- **A show the translation identified is asked for by id**, which is the exact question, and its season cards take the name it was identified as. A film translates to its IMDb id, which is better than any name.

Driven against the real services, keyless: "juego de tronos" resolves through IMDb to Game of Thrones, TVMaze turns tt0944947 into tvdb 121361, and NZBGeek answers with all eight seasons. "un lugar tranquilo" finds nothing as typed and 94 releases as tt6644200.

Not done, and it needs a TMDB key to do: the other direction, where she types "Game of Thrones" and a release filed as `Juego.de.Tronos.T01.ESP` is what she would want. That needs the list of names a title goes by rather than the one name it resolves to, which is TMDB's `alternative_titles` and nothing keyless offers for films. Worth doing when this indexer carries Spanish-named releases worth finding; today it carries almost none, and the pass above already records that a second, Spanish-focused indexer is the real fix.

## Eighteenth pass: the shelf was a list of errands, not a shelf

Her own films carried two buttons on every card: "Buscar subtítulos" and "Quitar". The first stood over films whose subtitles were already there and had been for weeks, which made it an errand with no point and no way to judge whether pressing it had helped. And a card was a trapdoor: touching one started a film, or opened a list of "Episodio 1, Episodio 2, Episodio 3" that started one the moment it was touched. Nothing on either screen said what the next touch would do.

Everything she owns now has a page of its own, the way it does in the players she already knows:

- **The grid is covers and names.** One card, one thing, one action: open it. No card decides anything any more.
- **The page says what it is** (poster, title, year, what it is spoken in), **what it is about** (the same synopsis the search screen shows, asked for by the id kept with the film rather than by a place in results long since replaced: `library_synopsis`), and carries **one big button that plays it**.
- **An episode is a page too**, with its name, what happens in it, and its own play button. The row that opens it says the episode's name where the show database has one, and "sin subtítulos" where that is the answer.
- **Episodes are named on her own shelf**, not only in the search results that found them. The season now remembers the show's ids and which seasons the folder holds, so the same database that named them at download time can name them a month later. A season downloaded before this pass keeps its numbers: a name nobody stated is still never invented.
- **The subtitles are a state, not an errand.** The page says what is there ("Subtítulos en español listos", "Faltan los subtítulos del episodio 7"), in the colour of the answer, with one small "buscar otra vez" beside it. Looking again reloads the episode list afterwards, because which episodes she can understand is exactly what may have changed.
- **Quitar moved to the page**, with the same confirm, and lands her back among her films.

The decision band — facts on one side, the button on the other — is now the same on the three screens that decide something: download, play, play this episode.

## Open

- **Install it on a Windows machine and see what happens.** Building is not running: WebView2 must be present, the sidecars must be found beside the binary, notifications must appear, and the settings screen has to be filled in from scratch. This gates everything else.
- **Prove the chase against reality.** The orchestrator tests cover the logic with fakes; one real dead-copy fall-through on Linux is still the cheapest confidence available.
- **An update path.** Every later fix currently means walking her through an installer. A Tauri updater needs signing keys and a hosting decision; decide together with the SmartScreen question, since both are about signing.
- **Passwords still live in plain `settings.json`** (owner-only permissions on unix, profile ACL on Windows). Windows Credential Manager remains the upgrade; accepted for now.
- **Unsigned, so first run shows SmartScreen.** Certificate or written click-through instructions.
- Verify against a live NZBGeek feed that covers really do come from a sister host of the API (the check now allows the whole site, not just the exact host).

## What is here and what is good

- `core/` is pure logic behind seams (`Indexer`, `Downloader`, `SubtitleSource`, `Clock`, `HttpClient`) with hand-written fakes and genuinely good tests. Keep it as is.
- The product thesis is right and mostly executed: one card per film, the app picks the release, dead copies are chased automatically, every self-initiated change writes a dated Spanish line with the technical `why` kept out of sight. The wording discipline is enforced by tests.
- The UI is one Preact page with no build step, tested in jsdom, and the tests assert behaviour ("the page renders as a page", "she is not made to count copies") rather than implementation.
- Almost everything below lives in one place: `src-tauri/src/lib.rs`, the layer that is simultaneously composition root, orchestrator and translator, and the one layer with no tests.

## Findings, ranked

### 1. Settings do not take effect until restart, and nothing says so

`App` (indexers, nzbget config, destination) is built once in `setup` (`lib.rs:1150`). `save_settings` only writes the file, then says "Guardado. Ya se puede buscar películas.", which is false: the running app still has the old indexers, and nzbget still runs with the old news server and old destination. "Comprobar" calls `check_indexer`, which reads `app.indexers` from startup (`lib.rs:1018`), so it validates the credentials that were just replaced. The first-run flow is therefore: fill in settings, be told it works, search, get "No hay nada con ese nombre" (an empty indexer list is not a total failure, `search.rs:17`), which is a confident lie.

Fix: rebuild the app state on save (swap indexers, restart nzbget, re-read destination), and make "Comprobar" test the values currently typed, passed in as arguments, not whatever state was loaded at startup.

### 2. The settings screen silently drops edits

- `news_port` arrives as a JS string (`event.target.value`), and `save_settings` reads it with `as_u64` (`lib.rs:1001`), which fails on strings. Editing the port is silently ignored.
- `news_connections` is rendered, returned by `read_settings`, and never read at all in `save_settings`. Edits are dropped.

The UI tests did not catch this because they fake `invoke`; the serde contract between `app.js` and the commands is tested nowhere. See the structural suggestion below.

### 3. A false "No he podido conseguirla" flashes when a copy dies

The chase loop runs every 4s (`lib.rs:1223`), the poll every 1.5s. Between a copy failing and `chase_failures` starting the next one, `progress_now` reports the item as finished, not ok, `next_id: None`, so `app.js:504` shows the failed headline, which then flips to "Buscando otra versión…". This is precisely the class of bug the story mechanism was built to kill, reintroduced by a race.

Fix cheaply: in `progress_now`, an unsuccessful history item whose id is still in `attempts` is retrying, not failed.

### 4. The give-up story can be false

- In `chase_failures`, when `start()` fails for the remaining copies (`lib.rs:629`), the real reason (disk full, indexer unreachable) goes to `eprintln` only; no note is written, and the screen then shows `exhausted()`: "todo lo que había venía dañado". If the disk was full, that sentence is wrong.
- `start()` silently skips copies (too big, unknown indexer, fetch failure) with no story line, violating the "if the app changes what it is doing, it writes a line" rule.
- A news-server auth failure is indistinguishable in code from seven dead copies: every non-deliberate failure folds into the same path, so an expired password makes the app chase 200 GB to conclude the wrong thing and tell her to wait a few days.

### 5. A dead or wedged nzbget is invisible

- If `progress` errors, the UI does `catch { return }` (`app.js:484`): the screen freezes on stale state forever, nothing said.
- If nzbget fails to start, `setup` errors and the whole app exits via `.expect("the application starts")` (`lib.rs:1259`): she double-clicks the icon and nothing appears. That path needs a visible Spanish message.
- `Status::Paused` maps to "Últimos detalles…" (`lib.rs:712`), and nzbget's default `DiskSpace=250MB` (not set in `render_config`) pauses the queue when the disk fills, so a full disk mid-download reads as "finishing…" forever.

### 6. Only one download exists on screen

`watching` is a single film; a second simultaneous download is a badge number with no way to see or reach it. Either serialize (one download at a time, honestly queued, probably right for her) or make Descargando a list. Decide, then model it.

### 7. English leaks onto her screen

`Error`'s `Display` is English ("cannot reach…", "refused the request (429)…"). A total search failure shows `problems.join("\n")` verbatim (`lib.rs:294` → `app.js:563`); grab failures can surface `failure.to_string()` (`lib.rs:535`). The boundary should map `Error` variants to Spanish sentences and push the English detail into the story's `why` and the log. Doing this properly is also the fix for finding 4's taxonomy: `Unreachable` or `Refused(401)` from the downloader means "revisa los ajustes", not "estaba dañado".

### 8. There are no logs

Five `eprintln` sites, and a Windows GUI app has no stderr, so "me checking the logs" is impossible on her machine. The persisted stories in `library.json` are the only record, and the paths that most need diagnosis (startup failure, chase dead-ends, subtitle refusals) are exactly the ones that only eprintln. Add a small rotating log file beside `library.json`; route every note's `why` and every swallowed error through it.

### 9. Two launches, one queue

No single-instance guard: two app instances start two nzbgets on different ports sharing the same `QueueDir`. A double-double-click on a slow Windows machine is the realistic path. `tauri-plugin-single-instance` is the boring fix. Related: double-clicking Descargar can start the same film twice, because `grab_now` only checks `library.present` (settled films), not in-flight attempts with the same key.

### 10. The two disk-space rules disagree

The UI warns at `bytes * 1.4 > free` (`app.js:216`); the backend refuses at 2.2× plus a 5 GB reserve (`space.rs`). Between 1.4× and 2.2× she sees no warning, presses Descargar, and is refused. Compute the verdict server-side with `space::room_for` and ship it on the version and the card.

### Smaller, still real

- The chase is uncapped: `candidates_from` keeps every release the indexers returned. Nobody chose "all of them"; pick a number or a byte budget, and have the story say it stopped trying.
- `cover()` refuses any host other than the indexer's API host (`indexer.rs:187`). If NZBGeek serves covers from a different domain, posters silently never load. Verify against a real feed.
- `cmd /C start "" <path>` breaks on `&` in a folder name (`lib.rs:1127`); use `explorer` or `ShellExecuteW`.
- Quitting can hang up to 10s waiting on nzbget shutdown on the main thread (`supervisor.rs:72`).
- Subtitle failures settle permanently with a warning and no "buscar subtítulos" button, against the "prefer the action over the warning" rule.
- The settings screen has no encryption toggle, and `save_settings` defaults `news_encrypted` to true, so a plain-text port 119 server cannot be configured. Fine while the server is Eweka; a trap later.
- The default language chip is "Versión original". Confirm that is actually her preference; if she watches dubs, the default is wrong for the person the app is for.
- Story timestamps show only hour and minute; a story spanning days reads wrong.
- A cancelled download leaves no record at all, and the story is only visible while a film is downloading. A small "qué pasó" view would cover both, if only for diagnosis.

## The UX itself, assuming every bug above were fixed

The structure is right: one screen at a time, big targets, plain Spanish, the tap answers where it was made. But several flows still hand her the machine, or dead-end her, even when they work as designed.

### She is never told the film is ready

A download takes twenty minutes to an evening. She will not stare at a progress bar; she will do something else, and the app has no way to call her back: no OS notification, no sound, nothing on the taskbar. "Ya puedes ver El Sur" as a Windows toast when a film settles is probably the single highest-value UX change in this list. Related: closing the window stops nzbget (`lib.rs:1233`), so closing the app mid-download silently pauses the film. Either keep downloading from the tray, or say plainly on the Now screen that the app must stay open.

### The app tells her to delete films and gives her no way to do it

The disk-full message says "Borra alguna película que ya hayas visto y vuelve a intentarlo" (`lib.rs:573`), and the shelf offers no delete. She would need Windows Explorer, which is exactly the place this app exists to keep her out of. The shelf needs "Quitar esta película", with a confirm, wired to the library so the shelf and the disk stay agreed.

### A season ends in a file manager

"Se abre la carpeta y se elige el episodio" opens Explorer on `Show.S01E03.1080p.WEB-DL.x264-GRP.mkv`, sample files and a Subs folder: release names and paths, the exact guts the app promises to never show. `episode_of` (`series.rs:82`) can already number the files, so an in-app episode list ("Episodio 1", "Episodio 2", each a play button) is cheap and removes the file manager from her life entirely.

### The search screen asks her to make the app's decisions

Before typing she faces two taxonomies: Películas vs Series, and Versión original vs En español vs Cualquiera. The kind toggle exists because the backend needs different queries, which is the app's problem, not hers: search both categories and show films and seasons in one result list, labelled. The language preference is a set-once fact about her, not a per-search decision: move it to settings, let Fausto set it, and drop the chips from her path. "Decisions we can make, we make."

### A typo is a dead end

"No hay nada con ese nombre" after a misspelling or a missing accent is false and leaves her nowhere to go. The film-lookup-as-she-types popover (salvaged below) is not a nice-to-have; it is the fix for the most common failure a non-typist will hit. Elevate it.

### Sizes are the wrong words on the decide screen

"Ocupa 1.7 GB · quedan 412 GB libres" asks her to hold units she has no feel for, on the one screen where she decides. The app already knows her typical download speed; say what she can plan around: "Tardará una media hora" with "hay sitio de sobra" or "puede que no quepa", and keep the gigabytes in the story's `why`. The same logic already applied to `remaining_in_words` (`lib.rs:817`); the decide screen never got it.

### Smaller UX notes

- The story renders as a timestamped log, growing downwards. The mechanism is right; the presentation is a debug view. Show the latest line under the headline, collapse the rest behind "qué ha pasado".
- The free-space footer is a permanently visible number she cannot act on. Show it only when the disk is actually getting tight, in words, with the delete action next to it.
- Search cards carry quality and size at browse time, before she has expressed interest. Title, year and poster are the card; the technical facts belong to the detail screen.
- The theme is forced dark (`styles.css:2`). For an older reader on Windows, light is usually easier; at minimum decide deliberately rather than inherit the developer aesthetic, or follow the OS.
- Ajustes sits in her nav bar as a peer of Buscar, both tempting and scary. She needs at most the folder chooser; consider a plain screen for that and the credentials behind a disclosure, since a mis-tap in there breaks the whole app.
- There is no update path. When the app needs fixing on her machine, someone has to walk her through an installer. A Tauri updater, or at least a "hay una versión nueva" pointer, is what keeps every later fix in this file deliverable.

The last several rounds of bugs, and almost everything above, live in `lib.rs`, which is untestable as written: `App` holds concrete `Newznab<Network>` and `NzbgetRpc<Network>` even though `core` already defines the traits.

- Extract an orchestrator generic over `Indexer + Downloader + Clock`, owning `start`, `chase_failures`, `progress_now` and the message mapping. Tauri commands become one-liners over it; the composition root goes back to being just composition.
- Findings 3, 4, 5 and 7 then each become a plain test with `FakeHttp` and a fake downloader.
- Add one integration test that drives the real command layer (serde included) against fakes at the HTTP seam. That is the test that would have caught the `news_port` string bug, which the jsdom tests structurally cannot see.

## Suggested order

1. Settings pipeline (findings 1 and 2): it poisons first contact and every remote debugging session.
2. The orchestrator extraction: most later fixes become small and testable once it exists.
3. Failure taxonomy and story completeness (findings 4 and 7), plus the chase cap.
4. The retry flash (finding 3).
5. Liveness and logging (findings 5 and 8).
6. Single instance and double-grab (finding 9).
7. The download model decision (finding 6) and the disk-space rule (finding 10).
8. The four UX changes that change what the app is for her: the ready notification, delete from the shelf, the in-app episode list, and one search box with the language preference moved to settings. The lookup-as-she-types popover rides along with the search work.

## Salvaged from TASKS.md and BUGS.md

Worth keeping, verified or still-relevant:

- **Windows build knowledge** (TASKS): cross-compiled with cargo-xwin and bundled by NSIS on Linux, no Windows machine and no CI; WiX MSI would need wine, so NSIS only. Sidecars (`nzbget.exe`, `unrar.exe`, `7za.exe`) taken from nzbget's own Windows installer, pinned by sha256 via `scripts/pin-sidecars.sh`. Licence edges: nzbget GPLv2 and 7-Zip LGPL need the source offer honoured; unrar is redistributable unmodified but not free.
- **Still untested on a real Windows machine** (TASKS): WebView2 presence, sidecars found beside the binary, settings from scratch. This gates everything else; the graphics workarounds in `scripts/dev.sh` are Linux-dev-machine problems that will not follow the app to her.
- **SmartScreen** (TASKS): unsigned, so first run warns. Decide between a signing certificate and written click-through instructions for her.
- **Passwords in plain `settings.json`** (TASKS): move to Windows Credential Manager, or accept and document the risk.
- **Film lookup as she types** (TASKS): search TMDB (key present, Spanish metadata) with IMDb's keyless suggestion endpoint as fallback; picking a suggestion yields an IMDb id, making the indexer search exact and giving a properly spelled title and poster for free.
- **Television, reconsidered** (TASKS): the pack-only model depends on packs existing, which the fourteenth pass improved a great deal (asking by id, not by name) without changing the model itself. Sonarr's series-and-episode-list model is the alternative. Deliberately deferred; revisit before building more TV features.
- **MP4 is not parsed** (BUGS, open): an `.mp4` release reports unknown languages and loses the subtitle timing check and frame-rate rescaling. Read it the way Matroska is read.
- **An incomplete copy is only discovered by downloading it** (BUGS, open): the indexer publishes no completeness figure; votes and age rank the risk, and the fall-through spends the attempt. Inherent; the chase cap bounds the cost.
- **The interface rules learned from the prototype** (TASKS): already codified in `engineering/README.md`, which is the canonical home. Nothing to copy.

Dropped: the fixed-bug history in BUGS.md (the regression tests are the durable record), the prototype narrative, and TASKS.md's done-checkbox log. NEXT.md's priority list is replaced by this file; its items 2, 4, 5 and 6 were verified real and appear above as findings 4, the chase cap, play-the-film, and the silent catches respectively.
