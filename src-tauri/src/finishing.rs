//! The work that happens after nzbget says a film has arrived: look inside it, put the subtitles
//! where a player will find them, and fetch Spanish ones when there are none she can read.

use crate::library::Library;
use mamacine_core::error::Result;
use mamacine_core::matroska;
use mamacine_core::media::{
    plan_subtitle_moves, subtitle_beside, MediaInfo, SUBTITLE_SUFFIXES, VIDEO_SUFFIXES,
};
use mamacine_core::nzbget::{Downloader, HistoryItem};
use mamacine_core::opensubtitles::{SubtitleQuery, SubtitleSource};
use mamacine_core::subtitles::{
    check_timing, frame_rate_factor, movie_hash, rank, rescale, Ranked, Timing,
};
use std::path::{Path, PathBuf};

/// Enough alternatives that a badly timed one is not the end of the evening.
const ALTERNATIVES: usize = 3;

/// What came of looking. Kept apart so "there are none" is never said about a refusal.
#[derive(Default)]
pub struct Outcome {
    pub saved: usize,
    pub mistimed: usize,
    pub refused: Vec<String>,
    /// The service will not hand out more today. Every episode after this one gets the same
    /// answer, so the rest of the season stops asking.
    pub spent: bool,
}

impl Outcome {
    pub fn describe(&self) -> String {
        if self.saved > 0 {
            return match self.saved {
                1 => "Subtítulos en español añadidos".to_string(),
                saved => format!("Subtítulos en español añadidos a {saved} episodios"),
            };
        }
        if self.spent {
            return ALLOWANCE_GONE.to_string();
        }
        if !self.refused.is_empty() {
            return "Hay subtítulos, pero ahora mismo no se han podido descargar".to_string();
        }
        if self.mistimed > 0 {
            return "Los subtítulos que hay no son de esta copia".to_string();
        }
        "No hay subtítulos en español para esta copia".to_string()
    }
}

const ALLOWANCE_GONE: &str = "El servicio de subtítulos no deja descargar más por hoy";

/// The allowance answering, rather than anything about this film: 429 is what the service says
/// once the day's downloads are gone, and the client says the same thing without asking again.
fn allowance_gone(failure: &mamacine_core::error::Error) -> bool {
    use mamacine_core::error::Error;
    match failure {
        Error::Refused { status: 429, .. } => true,
        Error::Setup(said) => said.contains("allowance"),
        _ => false,
    }
}

/// What one episode ended up with. The screen is written from these rather than from a count of
/// what this attempt happened to fetch: an episode that already had subtitles last week still has
/// them, and saying otherwise is what made "2 de 12" mean nothing.
pub struct Look {
    /// Which episode this is, from its name. A film, and a file nobody can number, has none.
    pub episode: Option<u32>,
    pub subtitles: Subtitles,
    pub spent: bool,
}

pub enum Subtitles {
    /// It already had them: nothing was asked of anybody.
    Already,
    /// Fetched just now.
    Fetched,
    /// Still none, and why, in her words.
    Missing(String),
}

impl Look {
    fn missing(&self) -> Option<&str> {
        match &self.subtitles {
            Subtitles::Missing(said) => Some(said),
            _ => None,
        }
    }

    fn fetched(&self) -> bool {
        matches!(self.subtitles, Subtitles::Fetched)
    }
}

pub struct Finisher {
    pub downloader: Box<dyn Downloader>,
    pub subtitles: Box<dyn SubtitleSource>,
    pub library: std::sync::Arc<Library>,
    pub log: std::sync::Arc<crate::log::Log>,
    pub language: String,
    /// For the copy a swap leaves behind. Behind the same trait the shelf's Borrar uses, so a
    /// test never touches a real bin.
    pub remover: std::sync::Arc<dyn crate::orchestrator::Remover>,
    /// She will not stare at a progress bar for an hour: a finished film calls her back.
    pub notify: crate::orchestrator::Notify,
}

impl Finisher {
    /// Runs over anything finished that has not been settled yet. One unreadable file must not take
    /// the thread down with it: before, a panic here meant nothing was ever finished again.
    pub fn sweep(&self) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.sweep_once()));
        if outcome.is_err() {
            self.log
                .line("finishing a download panicked; carrying on with the rest");
        }
    }

    fn sweep_once(&self) {
        let Ok(history) = self.downloader.history() else {
            return;
        };
        for item in history {
            if !item.succeeded {
                continue;
            }
            if self.library.get(item.id).map(|entry| entry.settled) == Some(true) {
                continue;
            }
            self.settle(&item);
        }
    }

    /// The copy she swapped out, now that the one she swapped it for is really here.
    ///
    /// Only now, and only if the new one landed: asked to change a copy because the one she had
    /// spoke Italian, she must never be left with neither. The old folder goes to the papelera
    /// rather than being deleted, so a swap she regrets is still recoverable by anyone.
    fn retire_the_copy_it_replaces(&self, id: i64) {
        let Some(replaced) = self.library.get(id).and_then(|entry| entry.replaces) else {
            return;
        };
        self.library.update(id, |entry| entry.replaces = None);
        let Some(folder) = self
            .library
            .get(replaced)
            .and_then(|entry| entry.folder)
            .filter(|folder| folder.exists())
        else {
            return;
        };
        // The copy that just landed is never what gets thrown away. Both copies of a film used
        // to be filed under the film's title, so the new one arrived in the folder of the one it
        // was replacing and this binned the pair of them: she asked to change the language of a
        // film and the film went to the papelera. Nothing else that is hers goes either.
        let landed_in = self.library.get(id).and_then(|entry| entry.folder);
        let shared = landed_in.as_deref() == Some(folder.as_path())
            || self.library.all().into_iter().any(|(other, entry)| {
                other != replaced
                    && entry.settled
                    && entry.folder.as_deref() == Some(folder.as_path())
            });
        if shared {
            self.log.line(&format!(
                "not binning {}: the copy that replaced {replaced} is in it too",
                folder.display()
            ));
            return;
        }
        match self.remover.remove(&folder) {
            Ok(()) => {
                self.log.line(&format!(
                    "swapped out {replaced}: {} binned",
                    folder.display()
                ));
                self.library.update(replaced, |entry| {
                    entry.folder = None;
                    entry.file = None;
                    entry.settled = false;
                });
                self.library.note(
                    replaced,
                    "He cambiado esta copia por otra. La anterior está en la papelera.",
                    "replaced by a copy she chose",
                );
            }
            // she has the new copy either way; a bin that refuses is not worth a screen
            Err(failure) => self.log.line(&format!(
                "could not bin {} after the swap: {failure}",
                folder.display()
            )),
        }
    }

    fn settle(&self, item: &HistoryItem) {
        let Some(directory) = item.directory.as_ref().map(Path::new) else {
            return;
        };
        let Some(largest) = largest_video(directory) else {
            return;
        };
        let series = self
            .library
            .get(item.id)
            .map(|entry| entry.series)
            .unwrap_or(false);

        let looks = self.look_after(item, directory, series);

        let info = self.inspect(&largest);
        self.library.update(item.id, |entry| {
            entry.info = info.clone();
            entry.settled = true;
            entry.remaining.clear(); // it arrived; the copies behind it are no longer a plan
            entry.subtitle_note = summarise(&looks, series);
            // where it landed, so the shelf is a list of files rather than of history rows
            entry.folder = Some(directory.to_path_buf());
            entry.file = Some(largest.clone());
            if entry.title.is_empty() {
                entry.title = item.name.clone();
            }
        });
        self.retire_the_copy_it_replaces(item.id);
        self.library.note(
            item.id,
            if series {
                "Ya está lista. Ábrela para ver los episodios."
            } else {
                "Ya está lista para ver."
            },
            &format!("{} · {}", item.status, directory.display()),
        );
        let title = self.library.get(item.id).unwrap_or_default().title;
        (self.notify)(
            if title.is_empty() { &item.name } else { &title },
            if series {
                "Ya está lista. Abre Mamá Cine para ver los episodios."
            } else {
                "Ya está lista para ver en Mamá Cine."
            },
        );
    }

    /// The pack's own subtitles into place, and then, for every episode still without any she can
    /// read, one fetched. What each episode ends up with is what every sentence is written from.
    ///
    /// A season is a folder of episodes, and every one of them is an evening: before, only the
    /// largest file was looked at, so nine episodes out of ten arrived with nothing she could read.
    fn look_after(&self, item: &HistoryItem, directory: &Path, series: bool) -> Vec<Look> {
        let videos = if series {
            all_videos(directory)
        } else {
            largest_video(directory).into_iter().collect()
        };
        let loose = loose_subtitles(directory);
        let mut looks: Vec<Look> = Vec::new();
        for video in &videos {
            for planned in plan_subtitle_moves(video, &belonging_to(video, &loose, series)) {
                if std::fs::rename(&planned.from, &planned.to).is_ok() {
                    self.library.update(item.id, |entry| {
                        entry.subtitle_files.push(planned.language.clone());
                    });
                }
            }
            // once the day's allowance is gone it is gone for every episode behind this one, and
            // asking anyway is a refusal apiece: forty-nine of them, the evening this was found
            let spent = looks.iter().any(|look| look.spent);
            looks.push(self.look_at(item, video, series, spent));
        }
        looks
    }

    /// One file: what it already has, and what was fetched for it when it had nothing.
    fn look_at(&self, item: &HistoryItem, video: &Path, series: bool, spent: bool) -> Look {
        let episode = video
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(mamacine_core::series::episode_of)
            .map(|(_, episode)| episode);
        let info = self.inspect(video);
        let beside = loose_subtitles(video.parent().unwrap_or(Path::new(".")));
        // what is already there is the only record of what was fetched before: asking again cost
        // a download from somebody else's allowance and left a second copy of the same dialogue
        if info.has_spanish() || subtitle_beside(video, &beside, &self.language) {
            return Look {
                episode,
                subtitles: Subtitles::Already,
                spent: false,
            };
        }
        if spent {
            return Look {
                episode,
                subtitles: Subtitles::Missing(ALLOWANCE_GONE.to_string()),
                spent: true,
            };
        }
        // one per episode rather than three: a season is ten times the requests, and OpenSubtitles
        // is somebody else's allowance
        let wanted = if series { 1 } else { ALTERNATIVES };
        match self.fetch_subtitles(video, &info, item, wanted) {
            Ok(outcome) if outcome.saved > 0 => Look {
                episode,
                subtitles: Subtitles::Fetched,
                spent: false,
            },
            Ok(outcome) => Look {
                episode,
                spent: outcome.spent,
                subtitles: Subtitles::Missing(outcome.describe()),
            },
            Err(failure) => {
                self.log
                    .line(&format!("subtitles for {}: {failure}", video.display()));
                let spent = allowance_gone(&failure);
                Look {
                    episode,
                    spent,
                    subtitles: Subtitles::Missing(if spent {
                        ALLOWANCE_GONE.to_string()
                    } else {
                        "No se han podido buscar subtítulos ahora mismo".to_string()
                    }),
                }
            }
        }
    }

    /// Looks again for subtitles she asked about, for a film that settled without them. The note
    /// is replaced by what this attempt found, so the button always answers.
    pub fn refetch_subtitles(&self, id: i64) -> std::result::Result<String, String> {
        let entry = self
            .library
            .get(id)
            .ok_or_else(|| "Esa película ya no está en este ordenador.".to_string())?;
        let folder = entry
            .folder
            .clone()
            .filter(|folder| folder.exists())
            .ok_or_else(|| "Esa película ya no está en este ordenador.".to_string())?;
        if all_videos(&folder).is_empty() {
            return Err("No se encuentra el archivo de la película.".to_string());
        }

        let item = mamacine_core::nzbget::HistoryItem {
            id,
            name: entry.title.clone(),
            succeeded: true,
            status: String::new(),
            directory: Some(folder.display().to_string()),
            size_mb: 0,
            total_articles: 0,
            failed_articles: 0,
            health_percent: 0.0,
        };
        let looks = self.look_after(&item, &folder, entry.series);
        let note = summarise(&looks, entry.series);
        let said = changed(&looks, entry.series);
        self.library
            .update(id, |entry| entry.subtitle_note = note.clone());
        self.library.note(id, &said, "subtitles refetched by hand");
        Ok(said)
    }

    fn inspect(&self, video: &Path) -> MediaInfo {
        inspect(video)
    }

    fn fetch_subtitles(
        &self,
        video: &Path,
        info: &MediaInfo,
        item: &HistoryItem,
        wanted: usize,
    ) -> Result<Outcome> {
        let entry = self.library.get(item.id).unwrap_or_default();
        let query = SubtitleQuery {
            language: self.language.clone(),
            imdb_id: entry.imdb.clone(),
            movie_hash: hash_of(video),
            file_name: video
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.replace(['.', '_'], " ")),
        };

        let found = self.subtitles.find(&query)?;
        let ranked = rank(
            found,
            video
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(""),
            mamacine_core::subtitles::MediaInfo {
                fps: info.fps,
                duration_seconds: info.duration_seconds,
            },
        );

        // an exact match is already timed to this file, so alternatives would only waste allowance
        let wanted = if ranked.first().map(|best| best.candidate.hash_match) == Some(true) {
            1
        } else {
            wanted
        };
        self.save_subtitles(video, info, &ranked, wanted)
    }

    fn save_subtitles(
        &self,
        video: &Path,
        info: &MediaInfo,
        ranked: &[Ranked],
        wanted: usize,
    ) -> Result<Outcome> {
        let mut outcome = Outcome::default();
        let mut seen: Vec<(Option<i64>, String)> = Vec::new();

        for candidate in ranked {
            if outcome.saved >= wanted {
                break;
            }
            // three copies of one timing is no fallback at all
            let signature = (
                candidate.candidate.uploader,
                candidate.candidate.release.clone(),
            );
            if seen.contains(&signature) {
                continue;
            }
            seen.push(signature);

            // a refusal here is not the same as nothing existing, and must never be reported as one
            let mut content = match self.subtitles.download(candidate.candidate.file_id) {
                Ok(content) => content,
                Err(failure) => {
                    self.log
                        .line(&format!("subtitle download refused: {failure}"));
                    outcome.refused.push(failure.to_string());
                    // the allowance refuses the second candidate exactly as it refused the first
                    if allowance_gone(&failure) {
                        outcome.spent = true;
                        break;
                    }
                    continue;
                }
            };
            if let Some(factor) = frame_rate_factor(candidate.candidate.fps, info.fps) {
                content = rescale(&content, factor);
            }
            if !matches!(
                check_timing(&content, info.duration_seconds),
                Timing::Plausible
            ) {
                outcome.mistimed += 1; // timed for another cut, and unwatchable against this one
                continue;
            }

            let target = next_free(video, &self.language);
            match std::fs::write(&target, &content) {
                Ok(()) => outcome.saved += 1,
                Err(failure) => {
                    self.log
                        .line(&format!("could not write {}: {failure}", target.display()));
                    outcome.refused.push(failure.to_string());
                }
            }
        }
        Ok(outcome)
    }
}

fn next_free(video: &Path, language: &str) -> PathBuf {
    let stem = video
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("film");
    let folder = video.parent().unwrap_or(Path::new("."));
    let mut attempt = 1;
    loop {
        let candidate = folder.join(match attempt {
            1 => format!("{stem}.{language}.srt"),
            other => format!("{stem}.{language}.{other}.srt"),
        });
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

fn files_in(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

fn has_suffix(path: &Path, suffixes: &[&str]) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| suffixes.contains(&value.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Read from the file's own header rather than by running a program, so nothing has to ship one.
/// Matroska writes what we need before any of the video; an MP4 may keep it at either end, so
/// both ends are read.
pub fn inspect(video: &Path) -> MediaInfo {
    use std::io::{Read, Seek, SeekFrom};
    const HEADER: usize = 4 * 1024 * 1024;

    let Ok(mut file) = std::fs::File::open(video) else {
        return MediaInfo::default();
    };
    let mut front = vec![0u8; HEADER];
    let read = match file.read(&mut front) {
        Ok(read) => read,
        Err(_) => return MediaInfo::default(),
    };
    front.truncate(read);

    let iso_bmff = video
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| ["mp4", "m4v", "mov"].contains(&value.to_lowercase().as_str()))
        .unwrap_or(false);
    if !iso_bmff {
        return matroska::read_header(&front);
    }

    let size = file.metadata().map(|data| data.len()).unwrap_or(0) as usize;
    let tail = if size > front.len() {
        let from = size.saturating_sub(HEADER).max(front.len());
        let mut tail = vec![0u8; size - from];
        let filled = file
            .seek(SeekFrom::Start(from as u64))
            .and_then(|_| file.read(&mut tail))
            .unwrap_or(0);
        tail.truncate(filled);
        tail
    } else {
        Vec::new()
    };
    mamacine_core::mp4::read_header(&front, &tail)
}

/// Every episode in the folder, in the order they are named, which is the order they are watched.
pub fn all_videos(directory: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = files_in(directory)
        .into_iter()
        .filter(|path| has_suffix(path, &VIDEO_SUFFIXES))
        .collect();
    found.sort();
    found
}

/// Which of the pack's subtitles belong beside this episode. A film takes all of them; a season
/// takes only the ones marked with its own episode number, so that ten subtitle files are not all
/// renamed onto whichever episode happens to be the largest.
fn belonging_to(video: &Path, subtitles: &[PathBuf], series: bool) -> Vec<PathBuf> {
    use mamacine_core::series::episode_of;
    if !series {
        return subtitles.to_vec();
    }
    let Some(wanted) = video
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(episode_of)
    else {
        return Vec::new(); // an unnamed episode: better nothing than somebody else's dialogue
    };
    subtitles
        .iter()
        .filter(|subtitle| {
            subtitle
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(episode_of)
                == Some(wanted)
        })
        .cloned()
        .collect()
}

/// What the shelf says about a folder of episodes. "Sin subtítulos en español en 2 de 12
/// episodios" named no episode and offered nothing to do about it, and the number was not even
/// true: it counted the downloads this attempt failed to make, not the episodes without subtitles.
fn summarise(looks: &[Look], series: bool) -> String {
    let missing: Vec<&Look> = looks
        .iter()
        .filter(|look| look.missing().is_some())
        .collect();
    if missing.is_empty() {
        return match looks.len() {
            0 => String::new(),
            1 => "Subtítulos en español listos".to_string(),
            _ => "Subtítulos en español en todos los episodios".to_string(),
        };
    }
    if !series || looks.len() == 1 {
        return missing[0].missing().unwrap_or_default().to_string();
    }
    match numbered(&missing) {
        // naming ten of them is a paragraph; the list of episodes says which
        Some(episodes) if episodes.len() <= 3 => {
            format!("Faltan los subtítulos {}", of_episodes(&episodes))
        }
        _ if missing.len() == looks.len() => {
            format!("Faltan los subtítulos de los {} episodios", looks.len())
        }
        _ => format!(
            "Faltan los subtítulos de {} episodios de {}",
            missing.len(),
            looks.len()
        ),
    }
}

/// What the button answers: what this attempt changed, which is the one thing the screen does not
/// already say. Saying the same sentence back was what made it feel like nothing had happened.
fn changed(looks: &[Look], series: bool) -> String {
    let fetched: Vec<&Look> = looks.iter().filter(|look| look.fetched()).collect();
    let missing: Vec<&Look> = looks
        .iter()
        .filter(|look| look.missing().is_some())
        .collect();
    let spent = looks.iter().any(|look| look.spent);

    if !series || looks.len() == 1 {
        return match (fetched.is_empty(), missing.first()) {
            (false, _) => "Ya están los subtítulos en español.".to_string(),
            (true, Some(look)) => format!("{}.", look.missing().unwrap_or_default()),
            (true, None) => "Ya estaban los subtítulos en español.".to_string(),
        };
    }
    let phrase = |looks: &[&Look]| match numbered(looks) {
        Some(episodes) if episodes.len() <= 3 => of_episodes(&episodes),
        _ => format!("de {} episodios", looks.len()),
    };
    match (fetched.is_empty(), missing.is_empty()) {
        (true, true) => "Ya estaban todos los subtítulos.".to_string(),
        (false, true) => "Ya están todos los subtítulos.".to_string(),
        (false, false) => format!(
            "Ya están los subtítulos {}. Todavía faltan los {}.",
            phrase(&fetched),
            phrase(&missing)
        ),
        (true, false) if spent => {
            format!("{ALLOWANCE_GONE}. Mañana se puede volver a intentar.")
        }
        (true, false) => format!(
            "No hay subtítulos en español para {}. Puede que aparezcan más adelante.",
            match numbered(&missing) {
                Some(episodes) if episodes.len() <= 3 => the_episodes(&episodes),
                _ => format!("{} episodios", missing.len()),
            }
        ),
    }
}

/// Their episode numbers, or nothing at all if any of them cannot be numbered: half a list is
/// worse than a count, because the episodes it leaves out are the ones she would go looking for.
fn numbered(looks: &[&Look]) -> Option<Vec<u32>> {
    looks.iter().map(|look| look.episode).collect()
}

/// "del episodio 4", "de los episodios 4 y 7".
fn of_episodes(episodes: &[u32]) -> String {
    match episodes {
        [only] => format!("del episodio {only}"),
        _ => format!("de los episodios {}", list(episodes)),
    }
}

/// "el episodio 4", "los episodios 4 y 7".
fn the_episodes(episodes: &[u32]) -> String {
    match episodes {
        [only] => format!("el episodio {only}"),
        _ => format!("los episodios {}", list(episodes)),
    }
}

/// "4", "4 y 7", "4, 7 y 9".
fn list(episodes: &[u32]) -> String {
    let said: Vec<String> = episodes.iter().map(u32::to_string).collect();
    match said.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} y {last}", rest.join(", ")),
        None => String::new(),
    }
}

pub fn largest_video(directory: &Path) -> Option<PathBuf> {
    files_in(directory)
        .into_iter()
        .filter(|path| has_suffix(path, &VIDEO_SUFFIXES))
        .max_by_key(|path| path.metadata().map(|data| data.len()).unwrap_or(0))
}

/// Whether this episode has subtitles she can read: one beside it, or one inside it. The one
/// beside it is checked first, because reading a header costs a seek and a file named after the
/// episode is the answer nine times out of ten.
///
/// The screen that lists the episodes is where "which one" is worth answering, and the answer has
/// to be what is on the disk now rather than what one attempt happened to find once.
pub fn has_subtitles(video: &Path, beside: &[PathBuf], language: &str) -> bool {
    subtitle_beside(video, beside, language) || inspect(video).has_spanish()
}

pub fn subtitles_in(directory: &Path) -> Vec<PathBuf> {
    loose_subtitles(directory)
}

fn loose_subtitles(directory: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = files_in(directory)
        .into_iter()
        .filter(|path| has_suffix(path, &SUBTITLE_SUFFIXES))
        .collect();
    found.sort();
    found
}

fn hash_of(video: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    const CHUNK: usize = 65536;

    let mut file = std::fs::File::open(video).ok()?;
    let size = file.metadata().ok()?.len();
    if size < (CHUNK * 2) as u64 {
        return None;
    }
    let mut head = vec![0u8; CHUNK];
    file.read_exact(&mut head).ok()?;
    file.seek(SeekFrom::End(-(CHUNK as i64))).ok()?;
    let mut tail = vec![0u8; CHUNK];
    file.read_exact(&mut tail).ok()?;
    movie_hash(size, &head, &tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[derive(Default)]
    struct Silent;

    impl Downloader for Silent {
        fn append(&self, _: &str, _: &[u8]) -> Result<i64> {
            Ok(0)
        }
        fn queue(&self) -> Result<Vec<mamacine_core::nzbget::QueueItem>> {
            Ok(Vec::new())
        }
        fn history(&self) -> Result<Vec<HistoryItem>> {
            Ok(Vec::new())
        }
        fn download_rate(&self) -> Result<u64> {
            Ok(0)
        }
        fn cancel(&self, _: i64) -> Result<()> {
            Ok(())
        }
        fn forget(&self, _: i64) -> Result<()> {
            Ok(())
        }
        fn check_server(
            &self,
            _: &mamacine_core::settings::NewsServer,
        ) -> mamacine_core::nzbget::ServerCheck {
            mamacine_core::nzbget::ServerCheck::Unknown
        }
    }

    impl SubtitleSource for Silent {
        fn find(&self, _: &SubtitleQuery) -> Result<Vec<mamacine_core::subtitles::Candidate>> {
            Ok(Vec::new())
        }
        fn download(&self, _: i64) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
        fn downloads_remaining(&self) -> Option<i64> {
            None
        }
    }

    #[derive(Default)]
    struct Binned(std::sync::Mutex<Vec<PathBuf>>);

    impl crate::orchestrator::Remover for Binned {
        fn remove(&self, folder: &Path) -> std::result::Result<(), String> {
            self.0.lock().expect("not poisoned").push(folder.into());
            std::fs::remove_dir_all(folder).map_err(|failure| failure.to_string())
        }
    }

    // She asked for a copy in another language because the one she had spoke Italian. The old
    // folder goes, but only once the new one is really here: a swap that fails must leave her
    // the film she already had, not neither.
    #[test]
    fn the_copy_a_swap_leaves_behind_goes_to_the_bin_only_once_the_new_one_has_landed() {
        let directory = std::env::temp_dir().join("mama-cine-swap-test");
        let _ = std::fs::remove_dir_all(&directory);
        let old = directory.join("La.Virgen.Roja.2024.ITA");
        std::fs::create_dir_all(&old).expect("the copy she had");

        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(&directory, std::sync::Arc::clone(&log)));
        library.update(1, |entry| {
            entry.title = "La virgen roja".into();
            entry.settled = true;
            entry.folder = Some(old.clone());
        });
        library.update(2, |entry| {
            entry.title = "La virgen roja".into();
            entry.replaces = Some(1);
        });

        let bin = std::sync::Arc::new(Binned::default());
        let finisher = Finisher {
            downloader: Box::new(Silent),
            subtitles: Box::new(Silent),
            library: std::sync::Arc::clone(&library),
            log,
            language: "es".into(),
            remover: std::sync::Arc::clone(&bin)
                as std::sync::Arc<dyn crate::orchestrator::Remover>,
            notify: Box::new(|_, _| {}),
        };

        finisher.retire_the_copy_it_replaces(2);

        assert_eq!(
            bin.0.lock().expect("not poisoned").as_slice(),
            std::slice::from_ref(&old)
        );
        assert!(!old.exists());
        assert!(library.get(1).expect("the old record").folder.is_none());
        assert!(!library.get(1).expect("the old record").settled);
        assert_eq!(
            library.get(2).expect("the new record").replaces,
            None,
            "the swap is done, so a later sweep does not bin anything again"
        );

        finisher.retire_the_copy_it_replaces(2);
        assert_eq!(bin.0.lock().expect("not poisoned").len(), 1);
    }

    // She asked to change the language of a film and the film went to the papelera: both copies
    // were filed under the film's title, so the replacement landed in the folder of the copy it
    // was replacing, and binning that folder took the one she had just waited for.
    #[test]
    fn a_swap_never_bins_the_folder_the_new_copy_landed_in() {
        let directory = std::env::temp_dir().join("mama-cine-swap-collide");
        let _ = std::fs::remove_dir_all(&directory);
        let shared = directory.join("The red virgin");
        std::fs::create_dir_all(&shared).expect("the one folder both landed in");

        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(&directory, std::sync::Arc::clone(&log)));
        library.update(1, |entry| {
            entry.settled = true;
            entry.folder = Some(shared.clone());
        });
        library.update(2, |entry| {
            entry.settled = true;
            entry.folder = Some(shared.clone());
            entry.replaces = Some(1);
        });

        let bin = std::sync::Arc::new(Binned::default());
        let finisher = Finisher {
            downloader: Box::new(Silent),
            subtitles: Box::new(Silent),
            library: std::sync::Arc::clone(&library),
            log,
            language: "es".into(),
            remover: std::sync::Arc::clone(&bin)
                as std::sync::Arc<dyn crate::orchestrator::Remover>,
            notify: Box::new(|_, _| {}),
        };
        finisher.retire_the_copy_it_replaces(2);

        assert!(bin.0.lock().expect("not poisoned").is_empty());
        assert!(shared.exists(), "the film she waited for is still there");
    }

    // Nothing is thrown away for a download that was never a swap.
    #[test]
    fn an_ordinary_download_bins_nothing() {
        let directory = std::env::temp_dir().join("mama-cine-swap-none");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(&directory, std::sync::Arc::clone(&log)));
        library.update(9, |entry| entry.title = "El Sur".into());

        let bin = std::sync::Arc::new(Binned::default());
        let finisher = Finisher {
            downloader: Box::new(Silent),
            subtitles: Box::new(Silent),
            library,
            log,
            language: "es".into(),
            remover: std::sync::Arc::clone(&bin)
                as std::sync::Arc<dyn crate::orchestrator::Remover>,
            notify: Box::new(|_, _| {}),
        };
        finisher.retire_the_copy_it_replaces(9);
        assert!(bin.0.lock().expect("not poisoned").is_empty());
    }

    // Before, a season pack's ten subtitle files were all renamed onto whichever episode happened
    // to be the largest file, and the other nine episodes arrived with nothing to read.
    #[test]
    fn a_seasons_subtitles_go_beside_the_episode_they_belong_to() {
        let subtitles = paths(&[
            "/x/Subs/Show.S01E01.spa.srt",
            "/x/Subs/Show.S01E02.spa.srt",
            "/x/Subs/Show.S01E03.spa.srt",
        ]);
        let beside = belonging_to(Path::new("/x/Show.S01E02.1080p.mkv"), &subtitles, true);
        assert_eq!(beside, paths(&["/x/Subs/Show.S01E02.spa.srt"]));
    }

    #[test]
    fn an_episode_nobody_can_place_is_left_alone_rather_than_given_the_wrong_dialogue() {
        let subtitles = paths(&["/x/Subs/Show.S01E01.spa.srt"]);
        assert!(belonging_to(Path::new("/x/bonus-feature.mkv"), &subtitles, true).is_empty());
    }

    #[test]
    fn a_film_still_takes_every_subtitle_that_came_with_it() {
        let subtitles = paths(&["/x/Subs/2_Spanish.srt", "/x/Subs/3_English.srt"]);
        assert_eq!(
            belonging_to(Path::new("/x/Das.Boot.1981.mkv"), &subtitles, false),
            subtitles,
        );
    }

    fn look(episode: u32, subtitles: Subtitles) -> Look {
        Look {
            episode: Some(episode),
            subtitles,
            spent: false,
        }
    }

    #[test]
    fn a_season_says_which_episodes_are_missing_rather_than_how_many() {
        let all_fine = vec![
            look(1, Subtitles::Already),
            look(2, Subtitles::Fetched),
            look(3, Subtitles::Already),
        ];
        assert_eq!(
            summarise(&all_fine, true),
            "Subtítulos en español en todos los episodios"
        );

        let one_missing = vec![
            look(1, Subtitles::Already),
            look(2, Subtitles::Missing("No hay subtítulos".into())),
            look(3, Subtitles::Already),
        ];
        assert_eq!(
            summarise(&one_missing, true),
            "Faltan los subtítulos del episodio 2"
        );

        let two_missing = vec![
            look(1, Subtitles::Missing("No hay subtítulos".into())),
            look(2, Subtitles::Already),
            look(3, Subtitles::Missing("No hay subtítulos".into())),
        ];
        assert_eq!(
            summarise(&two_missing, true),
            "Faltan los subtítulos de los episodios 1 y 3"
        );
    }

    // Naming ten of them is a paragraph, and half a list is worse than a count: the episodes it
    // leaves out are the ones she would go looking for.
    #[test]
    fn a_season_missing_more_than_a_handful_is_counted_instead() {
        let missing = || Subtitles::Missing("No hay subtítulos".into());
        let many: Vec<Look> = (1..=5).map(|number| look(number, missing())).collect();
        assert_eq!(
            summarise(&many, true),
            "Faltan los subtítulos de los 5 episodios"
        );

        let unnamed = vec![
            look(1, missing()),
            Look {
                episode: None,
                subtitles: missing(),
                spent: false,
            },
        ];
        assert_eq!(
            summarise(&unnamed, true),
            "Faltan los subtítulos de los 2 episodios"
        );
    }

    #[test]
    fn a_film_says_what_happened_to_its_own_subtitles() {
        let refused = vec![Look {
            episode: None,
            subtitles: Subtitles::Missing(
                "Hay subtítulos, pero ahora mismo no se han podido descargar".into(),
            ),
            spent: false,
        }];
        assert_eq!(
            summarise(&refused, false),
            "Hay subtítulos, pero ahora mismo no se han podido descargar"
        );
        let fine = vec![Look {
            episode: None,
            subtitles: Subtitles::Already,
            spent: false,
        }];
        assert_eq!(summarise(&fine, false), "Subtítulos en español listos");
    }

    // The button used to answer with the sentence already on the card, whatever it had just done.
    #[test]
    fn the_button_answers_with_what_it_changed() {
        let missing = || Subtitles::Missing("No hay subtítulos".into());
        assert_eq!(
            changed(
                &[look(1, Subtitles::Already), look(2, Subtitles::Already)],
                true
            ),
            "Ya estaban todos los subtítulos."
        );
        assert_eq!(
            changed(
                &[look(1, Subtitles::Already), look(2, Subtitles::Fetched)],
                true
            ),
            "Ya están todos los subtítulos."
        );
        assert_eq!(
            changed(
                &[
                    look(1, Subtitles::Fetched),
                    look(2, missing()),
                    look(3, Subtitles::Already)
                ],
                true
            ),
            "Ya están los subtítulos del episodio 1. Todavía faltan los del episodio 2."
        );
        assert_eq!(
            changed(&[look(1, Subtitles::Already), look(2, missing())], true),
            "No hay subtítulos en español para el episodio 2. Puede que aparezcan más adelante."
        );
    }

    // Forty-nine refusals in one evening, one per episode and three times over. What she needs to
    // know is that it is the service saying no for today, not this season being unsubtitled.
    #[test]
    fn a_spent_allowance_is_said_as_itself_and_not_as_an_absence() {
        let spent = Look {
            episode: Some(2),
            subtitles: Subtitles::Missing(ALLOWANCE_GONE.to_string()),
            spent: true,
        };
        assert_eq!(
            changed(&[look(1, Subtitles::Already), spent], true),
            "El servicio de subtítulos no deja descargar más por hoy. \
             Mañana se puede volver a intentar."
        );
    }
}
