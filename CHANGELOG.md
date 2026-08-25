# Changelog

What changed for the person using the app, newest entry first. Versions follow [semver](https://semver.org/); the shape follows [Keep a Changelog](https://keepachangelog.com/), loosely and in prose. A release's notes are its entry here.

## v0.4.0 · 2026-08-25

- The AppImage settles in on first launch: it copies itself somewhere safe from download-folder cleanups, and the menu entry points at that copy. One notification says the downloaded file is no longer needed.
- The Linux build now travels with its own downloader, the way the Windows one always did: a double-clicked AppImage works on a computer with nothing installed.
- Opening the log, a folder or a film from the AppImage no longer fails: programs the app starts are freed of the AppImage's own surroundings first.

## v0.3.0 · 2026-08-25

- On Linux, the AppImage puts itself in the desktop's menu on first launch: a proper entry with the app's own icon, kept pointing at wherever the file lives.

## v0.2.0 · 2026-08-25

The first release.

- Ajustes says which version of Mamá Cine is running, so an update has somewhere to be seen.
- A search box, a shelf of posters, and one button that plays the film: the app finds a copy over usenet, downloads it through its own private nzbget, fetches subtitles when the copy has none the person can read, and quietly tries the next copy when one arrives broken.
- Whole seasons of television as single downloads, with an episode list, per-episode subtitles, and episode names from the show's own database.
- Copies are judged before a byte is spent: which release is worth the bandwidth, whether its repair data is real, and whether it still exists on the news server.
- The interface speaks Spanish and English, follows the computer's language, and can be set in Ajustes. The languages of the subtitles, the film records and the dub-ranking follow the computer too, and can be pinned in the settings file.
- The app looks at GitHub Releases once a day: the AppImage replaces itself in place and starts new on the next launch; on Windows the app downloads the installer, verifies it, and runs it when asked to.
- A Windows installer and a Linux AppImage, both built reproducibly in pinned containers.
