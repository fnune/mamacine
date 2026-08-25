use crate::library::Library;
use crate::text::Lang;
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

const ALTERNATIVES: usize = 3;

#[derive(Default)]
pub struct Outcome {
    pub saved: usize,
    pub mistimed: usize,
    pub refused: Vec<String>,
    pub spent: bool,
}

impl Outcome {
    pub fn describe(&self, lang: Lang) -> String {
        if self.saved > 0 {
            return lang.subtitles_added(self.saved);
        }
        if self.spent {
            return lang.allowance_gone().to_string();
        }
        if !self.refused.is_empty() {
            return lang.subtitles_refused().to_string();
        }
        if self.mistimed > 0 {
            return lang.subtitles_mistimed().to_string();
        }
        lang.no_subtitles_for_this_copy().to_string()
    }
}

fn allowance_gone(failure: &mamacine_core::error::Error) -> bool {
    use mamacine_core::error::Error;
    match failure {
        Error::Refused { status: 406, .. } | Error::Refused { status: 429, .. } => true,
        Error::Setup(said) => said.contains("allowance"),
        _ => false,
    }
}

pub struct Look {
    pub episode: Option<u32>,
    pub subtitles: Subtitles,
    pub spent: bool,
}

pub enum Subtitles {
    Already,
    Fetched,
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
    pub subtitles: std::sync::Arc<dyn SubtitleSource>,
    pub library: std::sync::Arc<Library>,
    pub log: std::sync::Arc<crate::log::Log>,
    pub language: String,
    pub lang: Lang,
    pub remover: std::sync::Arc<dyn crate::orchestrator::Remover>,
    pub notify: crate::orchestrator::Notify,
}

impl Finisher {
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
            if self
                .library
                .get(item.id)
                .map(|entry| entry.settled || entry.retired)
                == Some(true)
            {
                continue;
            }
            self.settle(&item);
        }
    }

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
        let landed_in = self.library.get(id).and_then(|entry| entry.folder);
        let still_hosts_a_film = landed_in.as_deref() == Some(folder.as_path())
            || self.library.all().into_iter().any(|(other, entry)| {
                other != replaced
                    && entry.settled
                    && entry.folder.as_deref() == Some(folder.as_path())
            });
        if still_hosts_a_film {
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
                    entry.retired = true;
                    entry.file = None;
                });
                self.library.note(
                    replaced,
                    self.lang.copy_swapped(),
                    "replaced by a copy she chose",
                );
            }
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
            entry.remaining.clear();
            entry.subtitle_note = summarise(&looks, series, self.lang);
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
                self.lang.ready_series_note()
            } else {
                self.lang.ready_film_note()
            },
            &format!("{} · {}", item.status, directory.display()),
        );
        let title = self.library.get(item.id).unwrap_or_default().title;
        (self.notify)(
            if title.is_empty() { &item.name } else { &title },
            if series {
                self.lang.ready_series_notification()
            } else {
                self.lang.ready_film_notification()
            },
        );
    }

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
            let spent = looks.iter().any(|look| look.spent);
            looks.push(self.look_at(item, video, series, spent));
        }
        looks
    }

    fn look_at(&self, item: &HistoryItem, video: &Path, series: bool, spent: bool) -> Look {
        let episode = video
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(mamacine_core::series::episode_of)
            .map(|(_, episode)| episode);
        let info = self.inspect(video);
        let beside = loose_subtitles(video.parent().unwrap_or(Path::new(".")));
        if info.has_language(&self.language) || subtitle_beside(video, &beside, &self.language) {
            return Look {
                episode,
                subtitles: Subtitles::Already,
                spent: false,
            };
        }
        if spent {
            return Look {
                episode,
                subtitles: Subtitles::Missing(self.lang.allowance_gone().to_string()),
                spent: true,
            };
        }
        let one_per_episode = 1;
        let wanted = if series {
            one_per_episode
        } else {
            ALTERNATIVES
        };
        match self.fetch_subtitles(video, &info, item, wanted) {
            Ok(outcome) if outcome.saved > 0 => Look {
                episode,
                subtitles: Subtitles::Fetched,
                spent: false,
            },
            Ok(outcome) => Look {
                episode,
                spent: outcome.spent,
                subtitles: Subtitles::Missing(outcome.describe(self.lang)),
            },
            Err(failure) => {
                self.log
                    .line(&format!("subtitles for {}: {failure}", video.display()));
                let spent = allowance_gone(&failure);
                Look {
                    episode,
                    spent,
                    subtitles: Subtitles::Missing(if spent {
                        self.lang.allowance_gone().to_string()
                    } else {
                        self.lang.could_not_search_subtitles().to_string()
                    }),
                }
            }
        }
    }

    pub fn refetch_subtitles(&self, id: i64) -> std::result::Result<String, String> {
        let entry = self
            .library
            .get(id)
            .ok_or_else(|| self.lang.film_gone_from_computer().to_string())?;
        let folder = entry
            .folder
            .clone()
            .filter(|folder| folder.exists())
            .ok_or_else(|| self.lang.film_gone_from_computer().to_string())?;
        if all_videos(&folder).is_empty() {
            return Err(self.lang.film_file_missing().to_string());
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
        let note = summarise(&looks, entry.series, self.lang);
        let said = changed(&looks, entry.series, self.lang);
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

        let already_timed_to_this_file =
            ranked.first().map(|best| best.candidate.hash_match) == Some(true);
        let wanted = if already_timed_to_this_file {
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
        let mut takes_of_one_timing: Vec<(Option<i64>, String)> = Vec::new();

        for candidate in ranked {
            if outcome.saved >= wanted {
                break;
            }
            let same_take = (
                candidate.candidate.uploader,
                candidate.candidate.release.clone(),
            );
            if takes_of_one_timing.contains(&same_take) {
                continue;
            }
            takes_of_one_timing.push(same_take);

            let mut content = match self.subtitles.download(candidate.candidate.file_id) {
                Ok(content) => content,
                Err(failure) => {
                    self.log
                        .line(&format!("subtitle download refused: {failure}"));
                    outcome.refused.push(failure.to_string());
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
                outcome.mistimed += 1;
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

pub fn all_videos(directory: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = files_in(directory)
        .into_iter()
        .filter(|path| has_suffix(path, &VIDEO_SUFFIXES))
        .collect();
    found.sort();
    found
}

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
        return Vec::new();
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

fn summarise(looks: &[Look], series: bool, lang: Lang) -> String {
    let missing: Vec<&Look> = looks
        .iter()
        .filter(|look| look.missing().is_some())
        .collect();
    if missing.is_empty() {
        return match looks.len() {
            0 => String::new(),
            1 => lang.subtitles_ready().to_string(),
            _ => lang.subtitles_on_every_episode().to_string(),
        };
    }
    if !series || looks.len() == 1 {
        return missing[0].missing().unwrap_or_default().to_string();
    }
    match numbered(&missing) {
        Some(episodes) if episodes.len() <= 3 => lang.subtitles_missing_for(&episodes),
        _ if missing.len() == looks.len() => lang.subtitles_missing_for_all(looks.len()),
        _ => lang.subtitles_missing_count(missing.len(), looks.len()),
    }
}

fn changed(looks: &[Look], series: bool, lang: Lang) -> String {
    let fetched: Vec<&Look> = looks.iter().filter(|look| look.fetched()).collect();
    let missing: Vec<&Look> = looks
        .iter()
        .filter(|look| look.missing().is_some())
        .collect();
    let spent = looks.iter().any(|look| look.spent);

    if !series || looks.len() == 1 {
        return match (fetched.is_empty(), missing.first()) {
            (false, _) => lang.subtitles_already_there().to_string(),
            (true, Some(look)) => format!("{}.", look.missing().unwrap_or_default()),
            (true, None) => lang.subtitles_were_already_there().to_string(),
        };
    }
    let phrase = |looks: &[&Look]| match numbered(looks) {
        Some(episodes) if episodes.len() <= 3 => lang.of_episodes(&episodes),
        _ => lang.of_count_episodes(looks.len()),
    };
    match (fetched.is_empty(), missing.is_empty()) {
        (true, true) => lang.all_subtitles_already_there().to_string(),
        (false, true) => lang.all_subtitles_there_now().to_string(),
        (false, false) => lang.subtitles_fetched_and_missing(&phrase(&fetched), &phrase(&missing)),
        (true, false) if spent => lang.allowance_gone_try_tomorrow(),
        (true, false) => lang.no_subtitles_found_for(&match numbered(&missing) {
            Some(episodes) if episodes.len() <= 3 => lang.the_episodes(&episodes),
            _ => lang.count_episodes(missing.len()),
        }),
    }
}

fn numbered(looks: &[&Look]) -> Option<Vec<u32>> {
    looks.iter().map(|look| look.episode).collect()
}

pub fn largest_video(directory: &Path) -> Option<PathBuf> {
    files_in(directory)
        .into_iter()
        .filter(|path| has_suffix(path, &VIDEO_SUFFIXES))
        .max_by_key(|path| path.metadata().map(|data| data.len()).unwrap_or(0))
}

pub fn has_subtitles(video: &Path, beside: &[PathBuf], language: &str) -> bool {
    subtitle_beside(video, beside, language) || inspect(video).has_language(language)
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

    use mamacine_core::subtitles::Candidate;

    struct Answering {
        candidates: Vec<Candidate>,
        allowance: Option<usize>,
        finds: std::sync::Mutex<usize>,
        asked: std::sync::Mutex<Vec<i64>>,
    }

    impl Answering {
        fn holding(candidates: Vec<Candidate>) -> Self {
            Answering {
                candidates,
                allowance: None,
                finds: std::sync::Mutex::new(0),
                asked: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn finds(&self) -> usize {
            *self.finds.lock().expect("not poisoned")
        }

        fn asked(&self) -> Vec<i64> {
            self.asked.lock().expect("not poisoned").clone()
        }
    }

    impl SubtitleSource for Answering {
        fn find(&self, _: &SubtitleQuery) -> Result<Vec<Candidate>> {
            *self.finds.lock().expect("not poisoned") += 1;
            Ok(self.candidates.clone())
        }

        fn download(&self, file_id: i64) -> Result<Vec<u8>> {
            let mut asked = self.asked.lock().expect("not poisoned");
            asked.push(file_id);
            if self.allowance.map(|limit| asked.len() > limit) == Some(true) {
                return Err(mamacine_core::error::Error::Refused {
                    what: "opensubtitles".into(),
                    status: 406,
                    message: "quota reached".into(),
                });
            }
            Ok(format!("1\n00:00:01,000 --> 00:00:02,000\nHola {file_id}\n").into_bytes())
        }

        fn downloads_remaining(&self) -> Option<i64> {
            None
        }
    }

    fn candidate(file_id: i64, uploader: i64) -> Candidate {
        Candidate {
            file_id,
            release: format!("Release.{uploader}"),
            hash_match: false,
            fps: None,
            downloads: 100,
            rating: 0.0,
            trusted: false,
            machine_translated: false,
            foreign_parts_only: false,
            uploader: Some(uploader),
        }
    }

    fn arrived(id: i64) -> HistoryItem {
        HistoryItem {
            id,
            name: "arrived".into(),
            succeeded: true,
            status: "SUCCESS/ALL".into(),
            directory: None,
            size_mb: 1,
            total_articles: 1,
            failed_articles: 0,
            health_percent: 100.0,
        }
    }

    fn world(
        name: &str,
        subtitles: std::sync::Arc<dyn SubtitleSource>,
    ) -> (PathBuf, std::sync::Arc<Library>, Finisher) {
        let directory = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(
            &directory,
            std::sync::Arc::clone(&log),
            Lang::Es,
        ));
        let finisher = Finisher {
            downloader: Box::new(Silent),
            subtitles,
            library: std::sync::Arc::clone(&library),
            log,
            language: "es".into(),
            lang: Lang::Es,
            remover: std::sync::Arc::new(Binned::default()),
            notify: Box::new(|_, _| {}),
        };
        (directory, library, finisher)
    }

    #[derive(Default)]
    struct Binned(std::sync::Mutex<Vec<PathBuf>>);

    impl crate::orchestrator::Remover for Binned {
        fn remove(&self, folder: &Path) -> std::result::Result<(), String> {
            self.0.lock().expect("not poisoned").push(folder.into());
            std::fs::remove_dir_all(folder).map_err(|failure| failure.to_string())
        }
    }

    #[test]
    fn the_copy_a_swap_leaves_behind_goes_to_the_bin_only_once_the_new_one_has_landed() {
        let directory = std::env::temp_dir().join("mama-cine-swap-test");
        let _ = std::fs::remove_dir_all(&directory);
        let old = directory.join("La.Virgen.Roja.2024.ITA");
        std::fs::create_dir_all(&old).expect("the copy she had");

        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(
            &directory,
            std::sync::Arc::clone(&log),
            Lang::Es,
        ));
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
            subtitles: std::sync::Arc::new(Silent),
            library: std::sync::Arc::clone(&library),
            log,
            language: "es".into(),
            lang: Lang::Es,
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
        assert!(library.get(1).expect("the old record").retired);
        assert!(!library.get(1).expect("the old record").present());
        assert_eq!(
            library.get(2).expect("the new record").replaces,
            None,
            "the swap is done, so a later sweep does not bin anything again"
        );

        finisher.retire_the_copy_it_replaces(2);
        assert_eq!(bin.0.lock().expect("not poisoned").len(), 1);
    }

    #[test]
    fn a_swap_never_bins_the_folder_the_new_copy_landed_in() {
        let directory = std::env::temp_dir().join("mama-cine-swap-collide");
        let _ = std::fs::remove_dir_all(&directory);
        let shared = directory.join("The red virgin");
        std::fs::create_dir_all(&shared).expect("the one folder both landed in");

        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(
            &directory,
            std::sync::Arc::clone(&log),
            Lang::Es,
        ));
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
            subtitles: std::sync::Arc::new(Silent),
            library: std::sync::Arc::clone(&library),
            log,
            language: "es".into(),
            lang: Lang::Es,
            remover: std::sync::Arc::clone(&bin)
                as std::sync::Arc<dyn crate::orchestrator::Remover>,
            notify: Box::new(|_, _| {}),
        };
        finisher.retire_the_copy_it_replaces(2);

        assert!(bin.0.lock().expect("not poisoned").is_empty());
        assert!(shared.exists(), "the film she waited for is still there");
    }

    #[test]
    fn a_record_she_has_finished_with_is_never_settled_again() {
        let directory = std::env::temp_dir().join("mama-cine-resettle");
        let _ = std::fs::remove_dir_all(&directory);
        let landed = directory.join("The red virgin");
        std::fs::create_dir_all(&landed).expect("a folder");
        std::fs::write(landed.join("film.mkv"), b"not really a film").expect("a film");

        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(
            &directory,
            std::sync::Arc::clone(&log),
            Lang::Es,
        ));
        library.update(1, |entry| {
            entry.title = "The red virgin".into();
            entry.retired = true;
        });

        struct Remembers;
        impl Downloader for Remembers {
            fn append(&self, _: &str, _: &[u8]) -> Result<i64> {
                Ok(0)
            }
            fn queue(&self) -> Result<Vec<mamacine_core::nzbget::QueueItem>> {
                Ok(Vec::new())
            }
            fn history(&self) -> Result<Vec<HistoryItem>> {
                Ok(vec![HistoryItem {
                    id: 1,
                    name: "The red virgin".into(),
                    succeeded: true,
                    status: "SUCCESS/ALL".into(),
                    directory: None,
                    size_mb: 1,
                    total_articles: 1,
                    failed_articles: 0,
                    health_percent: 100.0,
                }])
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

        let finisher = Finisher {
            downloader: Box::new(Remembers),
            subtitles: std::sync::Arc::new(Silent),
            library: std::sync::Arc::clone(&library),
            log,
            language: "es".into(),
            lang: Lang::Es,
            remover: std::sync::Arc::new(Binned::default()),
            notify: Box::new(|_, _| {}),
        };
        finisher.sweep();

        let entry = library.get(1).expect("the record");
        assert!(entry.retired, "it is still hers no longer");
        assert!(
            !entry.settled,
            "and the sweep did not put it back on her shelf"
        );
        assert!(!entry.present());
    }

    #[test]
    fn an_ordinary_download_bins_nothing() {
        let directory = std::env::temp_dir().join("mama-cine-swap-none");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let log = std::sync::Arc::new(crate::log::Log::open(&directory));
        let library = std::sync::Arc::new(Library::open(
            &directory,
            std::sync::Arc::clone(&log),
            Lang::Es,
        ));
        library.update(9, |entry| entry.title = "El Sur".into());

        let bin = std::sync::Arc::new(Binned::default());
        let finisher = Finisher {
            downloader: Box::new(Silent),
            subtitles: std::sync::Arc::new(Silent),
            library,
            log,
            language: "es".into(),
            lang: Lang::Es,
            remover: std::sync::Arc::clone(&bin)
                as std::sync::Arc<dyn crate::orchestrator::Remover>,
            notify: Box::new(|_, _| {}),
        };
        finisher.retire_the_copy_it_replaces(9);
        assert!(bin.0.lock().expect("not poisoned").is_empty());
    }

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
            summarise(&all_fine, true, Lang::Es),
            "Subtítulos en español en todos los episodios"
        );

        let one_missing = vec![
            look(1, Subtitles::Already),
            look(2, Subtitles::Missing("No hay subtítulos".into())),
            look(3, Subtitles::Already),
        ];
        assert_eq!(
            summarise(&one_missing, true, Lang::Es),
            "Faltan los subtítulos del episodio 2"
        );

        let two_missing = vec![
            look(1, Subtitles::Missing("No hay subtítulos".into())),
            look(2, Subtitles::Already),
            look(3, Subtitles::Missing("No hay subtítulos".into())),
        ];
        assert_eq!(
            summarise(&two_missing, true, Lang::Es),
            "Faltan los subtítulos de los episodios 1 y 3"
        );
    }

    #[test]
    fn a_season_missing_more_than_a_handful_is_counted_instead() {
        let missing = || Subtitles::Missing("No hay subtítulos".into());
        let many: Vec<Look> = (1..=5).map(|number| look(number, missing())).collect();
        assert_eq!(
            summarise(&many, true, Lang::Es),
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
            summarise(&unnamed, true, Lang::Es),
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
            summarise(&refused, false, Lang::Es),
            "Hay subtítulos, pero ahora mismo no se han podido descargar"
        );
        let fine = vec![Look {
            episode: None,
            subtitles: Subtitles::Already,
            spent: false,
        }];
        assert_eq!(
            summarise(&fine, false, Lang::Es),
            "Subtítulos en español listos"
        );
    }

    #[test]
    fn the_button_answers_with_what_it_changed() {
        let missing = || Subtitles::Missing("No hay subtítulos".into());
        assert_eq!(
            changed(
                &[look(1, Subtitles::Already), look(2, Subtitles::Already)],
                true,
                Lang::Es
            ),
            "Ya estaban todos los subtítulos."
        );
        assert_eq!(
            changed(
                &[look(1, Subtitles::Already), look(2, Subtitles::Fetched)],
                true,
                Lang::Es
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
                true,
                Lang::Es
            ),
            "Ya están los subtítulos del episodio 1. Todavía faltan los del episodio 2."
        );
        assert_eq!(
            changed(
                &[look(1, Subtitles::Already), look(2, missing())],
                true,
                Lang::Es
            ),
            "No hay subtítulos en español para el episodio 2. Puede que aparezcan más adelante."
        );
    }

    #[test]
    fn a_film_without_spanish_gets_a_subtitle_fetched_and_placed_beside_it() {
        let source = std::sync::Arc::new(Answering::holding(vec![candidate(7, 1)]));
        let (directory, _, finisher) =
            world("mama-cine-fetch-film", std::sync::Arc::clone(&source) as _);
        let film = directory.join("Das.Boot.1981.mkv");
        std::fs::write(&film, b"not really a film").expect("a film");

        let looks = finisher.look_after(&arrived(1), &directory, false);
        assert!(matches!(looks[0].subtitles, Subtitles::Fetched));
        let beside = std::fs::read_to_string(directory.join("Das.Boot.1981.es.srt"))
            .expect("the subtitle beside the film");
        assert!(beside.contains("Hola 7"), "{beside}");

        let again = finisher.look_after(&arrived(1), &directory, false);
        assert!(matches!(again[0].subtitles, Subtitles::Already));
        assert_eq!(source.finds(), 1);
    }

    #[test]
    fn a_film_keeps_alternatives_and_each_lands_in_its_own_file() {
        let source = std::sync::Arc::new(Answering::holding(vec![
            candidate(1, 1),
            candidate(2, 2),
            candidate(3, 3),
            candidate(4, 4),
        ]));
        let (directory, _, finisher) = world(
            "mama-cine-fetch-alternatives",
            std::sync::Arc::clone(&source) as _,
        );
        std::fs::write(directory.join("El.Sur.1983.mkv"), b"not really a film").expect("a film");

        let looks = finisher.look_after(&arrived(1), &directory, false);
        assert!(matches!(looks[0].subtitles, Subtitles::Fetched));
        assert_eq!(
            source.asked().len(),
            ALTERNATIVES,
            "the fourth is left for somebody else"
        );
        for name in [
            "El.Sur.1983.es.srt",
            "El.Sur.1983.es.2.srt",
            "El.Sur.1983.es.3.srt",
        ] {
            assert!(directory.join(name).exists(), "{name} should exist");
        }
    }

    #[test]
    fn an_exact_hash_match_spends_one_download_rather_than_three() {
        let mut exact = candidate(9, 1);
        exact.hash_match = true;
        let source = std::sync::Arc::new(Answering::holding(vec![
            exact,
            candidate(2, 2),
            candidate(3, 3),
        ]));
        let (directory, _, finisher) =
            world("mama-cine-fetch-exact", std::sync::Arc::clone(&source) as _);
        std::fs::write(directory.join("Volver.2006.mkv"), b"not really a film").expect("a film");

        finisher.look_after(&arrived(1), &directory, false);
        assert_eq!(source.asked(), vec![9]);
    }

    #[test]
    fn the_same_upload_twice_is_skipped_rather_than_saved_twice() {
        let source = std::sync::Arc::new(Answering::holding(vec![
            candidate(1, 1),
            candidate(1, 1),
            candidate(3, 3),
        ]));
        let (directory, _, finisher) = world(
            "mama-cine-fetch-duplicate",
            std::sync::Arc::clone(&source) as _,
        );
        std::fs::write(directory.join("Tasio.1984.mkv"), b"not really a film").expect("a film");

        finisher.look_after(&arrived(1), &directory, false);
        assert_eq!(source.asked(), vec![1, 3]);
    }

    #[test]
    fn a_season_stops_asking_once_the_allowance_runs_out_mid_way() {
        let mut source = Answering::holding(vec![candidate(5, 1)]);
        source.allowance = Some(1);
        let source = std::sync::Arc::new(source);
        let (directory, _, finisher) = world(
            "mama-cine-fetch-season",
            std::sync::Arc::clone(&source) as _,
        );
        for episode in ["Show.S01E01.mkv", "Show.S01E02.mkv", "Show.S01E03.mkv"] {
            std::fs::write(directory.join(episode), b"not really an episode").expect("an episode");
        }

        let looks = finisher.look_after(&arrived(1), &directory, true);
        assert!(matches!(looks[0].subtitles, Subtitles::Fetched));
        assert_eq!(looks[1].missing(), Some(Lang::Es.allowance_gone()));
        assert_eq!(looks[2].missing(), Some(Lang::Es.allowance_gone()));
        assert!(directory.join("Show.S01E01.es.srt").exists());
        assert!(!directory.join("Show.S01E03.es.srt").exists());
        assert_eq!(
            source.finds(),
            2,
            "the third episode is never even searched for"
        );
        assert_eq!(
            source.asked().len(),
            2,
            "one download and the refusal, nothing more"
        );
    }

    #[test]
    fn the_button_fetches_for_a_film_that_settled_without_subtitles() {
        let source = std::sync::Arc::new(Answering::holding(vec![candidate(7, 1)]));
        let (directory, library, finisher) =
            world("mama-cine-refetch", std::sync::Arc::clone(&source) as _);
        std::fs::write(
            directory.join("Cria.Cuervos.1976.mkv"),
            b"not really a film",
        )
        .expect("a film");
        library.update(3, |entry| {
            entry.title = "Cría cuervos".into();
            entry.settled = true;
            entry.folder = Some(directory.clone());
            entry.subtitle_note = "No hay subtítulos en español para esta copia".into();
        });

        let said = finisher.refetch_subtitles(3).expect("an answer");
        assert_eq!(said, "Ya están los subtítulos en español.");
        assert!(directory.join("Cria.Cuervos.1976.es.srt").exists());
        assert_eq!(
            library.get(3).expect("the record").subtitle_note,
            "Subtítulos en español listos"
        );
    }

    #[test]
    fn a_spent_allowance_is_said_as_itself_and_not_as_an_absence() {
        let spent = Look {
            episode: Some(2),
            subtitles: Subtitles::Missing(Lang::Es.allowance_gone().to_string()),
            spent: true,
        };
        assert_eq!(
            changed(&[look(1, Subtitles::Already), spent], true, Lang::Es),
            "El servicio de subtítulos no deja descargar más por hoy. \
             Mañana se puede volver a intentar."
        );
    }
}
