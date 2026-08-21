//! What a finished file actually contains, and where its subtitles belong.
//!
//! The parsing and the decisions are here and pure; running ffprobe and moving files is the
//! application's job, because those are the parts a test should never do.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub fps: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub audio_languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
}

pub const SPANISH_CODES: [&str; 5] = ["spa", "es", "esp", "cast", "spanish"];
const UNKNOWN: &str = "und";

impl MediaInfo {
    /// Untagged tracks say nothing about the language, so they are never counted as evidence.
    pub fn known_languages(&self) -> Vec<&str> {
        self.audio_languages
            .iter()
            .chain(self.subtitle_languages.iter())
            .map(String::as_str)
            .filter(|code| *code != UNKNOWN)
            .collect()
    }

    pub fn has_spanish(&self) -> bool {
        self.known_languages()
            .iter()
            .any(|code| SPANISH_CODES.contains(code))
    }

    /// The honest third answer: the file simply does not say.
    pub fn language_is_unknown(&self) -> bool {
        self.known_languages().is_empty()
    }
}

pub const VIDEO_SUFFIXES: [&str; 5] = ["mkv", "mp4", "avi", "m4v", "mov"];
pub const SUBTITLE_SUFFIXES: [&str; 4] = ["srt", "ass", "ssa", "sub"];

const LANGUAGE_WORDS: [(&str, &[&str]); 4] = [
    (
        "es",
        &[
            "spanish",
            "castellano",
            "espanol",
            "español",
            "spa",
            "esp",
            "es",
            "cast",
        ],
    ),
    ("en", &["english", "eng", "en", "ingles", "inglés", "sdh"]),
    (
        "fr",
        &["french", "francais", "français", "fra", "fre", "fr"],
    ),
    ("de", &["german", "deutsch", "ger", "deu", "de", "aleman"]),
];

pub fn subtitle_language(file_name: &str) -> Option<&'static str> {
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    let words: Vec<String> = stem
        .split(|c: char| !c.is_alphanumeric())
        .map(|word| word.to_lowercase())
        .collect();
    LANGUAGE_WORDS
        .iter()
        .find(|(_, names)| words.iter().any(|word| names.contains(&word.as_str())))
        .map(|(code, _)| *code)
}

/// Whether a subtitle in this language already sits beside the film, named after it.
///
/// The only record of what was fetched last week is the file itself: nothing else survives a
/// restart, and asking again cost a download from somebody else's daily allowance and left a
/// second copy of the same dialogue in her player's menu.
pub fn subtitle_beside(video: &Path, files: &[PathBuf], language: &str) -> bool {
    let Some(stem) = video.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    files.iter().any(|file| {
        let Some(name) = file.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        file.parent() == video.parent()
            && has_suffix(name)
            && name.starts_with(stem)
            && subtitle_language(name) == Some(language)
    })
}

fn has_suffix(name: &str) -> bool {
    name.rsplit_once('.')
        .map(|(_, extension)| SUBTITLE_SUFFIXES.contains(&extension.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
    pub language: String,
}

/// Players only load subtitles that sit beside the film and share its name.
pub fn plan_subtitle_moves(video: &Path, subtitles: &[PathBuf]) -> Vec<Move> {
    let Some(stem) = video.file_stem().and_then(|stem| stem.to_str()) else {
        return Vec::new();
    };
    let folder = video.parent().unwrap_or(Path::new("."));

    let mut planned: Vec<Move> = Vec::new();
    for subtitle in subtitles {
        let Some(name) = subtitle.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if subtitle.parent() == Some(folder) && name.starts_with(stem) {
            continue; // already where a player will find it
        }
        let extension = subtitle
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("srt")
            .to_lowercase();
        let language = subtitle_language(name).unwrap_or("und").to_string();

        let mut attempt = 1;
        let destination = loop {
            let candidate = folder.join(match attempt {
                1 => format!("{stem}.{language}.{extension}"),
                other => format!("{stem}.{language}.{other}.{extension}"),
            });
            let taken = planned.iter().any(|planned| planned.to == candidate);
            if !taken && !candidate.exists() {
                break candidate;
            }
            attempt += 1;
        };
        planned.push(Move {
            from: subtitle.clone(),
            to: destination,
            language,
        });
    }
    planned
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spoken(languages: &[&str]) -> MediaInfo {
        MediaInfo {
            audio_languages: languages.iter().map(|code| code.to_string()).collect(),
            ..MediaInfo::default()
        }
    }

    #[test]
    fn an_untagged_track_is_unknown_rather_than_foreign() {
        // the Chilean film whose audio is Spanish but carries no tag
        let info = spoken(&["und"]);
        assert!(info.language_is_unknown());
        assert!(!info.has_spanish(), "unknown is not a claim either way");
        assert!(info.known_languages().is_empty());
    }

    #[test]
    fn a_spanish_track_is_recognised_however_it_is_spelled() {
        for code in ["spa", "es", "esp"] {
            assert!(spoken(&[code]).has_spanish(), "{code}");
        }
    }

    #[test]
    fn a_film_with_other_languages_is_understood_to_have_none_she_reads() {
        let info = MediaInfo {
            audio_languages: vec!["ger".into(), "eng".into()],
            subtitle_languages: vec!["fre".into()],
            ..MediaInfo::default()
        };
        assert!(!info.has_spanish());
        assert!(
            !info.language_is_unknown(),
            "this file did say, and what it said was not Spanish"
        );
    }

    #[test]
    fn subtitles_beside_the_film_count_as_much_as_tracks_inside_it() {
        let info = MediaInfo {
            audio_languages: vec!["ger".into()],
            subtitle_languages: vec!["spa".into()],
            ..MediaInfo::default()
        };
        assert!(info.has_spanish());
    }

    // Every "Buscar subtítulos" downloaded all twelve episodes again, because the app looked at
    // the tracks inside the file and at the pack's own subtitles, and never at the file it had
    // written itself. Four copies of the same dialogue, and the day's allowance spent on them.
    #[test]
    fn a_subtitle_already_beside_the_film_is_one_that_need_not_be_fetched_again() {
        let video = PathBuf::from("/x/Gomorrah.S01E01.Bluray.x265-iVy.mkv");
        let beside = |names: &[&str]| {
            names
                .iter()
                .map(|name| PathBuf::from(format!("/x/{name}")))
                .collect::<Vec<_>>()
        };
        assert!(subtitle_beside(
            &video,
            &beside(&["Gomorrah.S01E01.Bluray.x265-iVy.es.srt"]),
            "es"
        ));
        assert!(subtitle_beside(
            &video,
            &beside(&["Gomorrah.S01E01.Bluray.x265-iVy.spa.srt"]),
            "es"
        ));
        assert!(!subtitle_beside(
            &video,
            &beside(&["Gomorrah.S01E01.Bluray.x265-iVy.en.srt"]),
            "es"
        ));
        assert!(
            !subtitle_beside(
                &video,
                &beside(&["Gomorrah.S01E02.Bluray.x265-iVy.es.srt"]),
                "es"
            ),
            "another episode's dialogue is not this episode's subtitle"
        );
        assert!(
            !subtitle_beside(
                &video,
                &beside(&["Gomorrah.S01E01.Bluray.x265-iVy.mkv"]),
                "es"
            ),
            "the film is not its own subtitle"
        );
        assert!(
            !subtitle_beside(
                &video,
                &[PathBuf::from(
                    "/x/Subs/Gomorrah.S01E01.Bluray.x265-iVy.es.srt"
                )],
                "es"
            ),
            "a player loads what sits beside the film, not what is filed under it"
        );
    }

    #[test]
    fn names_subtitles_after_the_film_so_players_load_them() {
        let video = PathBuf::from("/films/Das Boot/Das.Boot.1981.mkv");
        let moves = plan_subtitle_moves(
            &video,
            &[
                PathBuf::from("/films/Das Boot/Subs/2_Spanish.srt"),
                PathBuf::from("/films/Das Boot/Subs/3_English.srt"),
            ],
        );
        assert_eq!(
            moves[0].to,
            PathBuf::from("/films/Das Boot/Das.Boot.1981.es.srt")
        );
        assert_eq!(
            moves[1].to,
            PathBuf::from("/films/Das Boot/Das.Boot.1981.en.srt")
        );
    }

    #[test]
    fn recognises_a_language_from_a_bare_code_as_well_as_a_name() {
        assert_eq!(subtitle_language("2_Spanish.srt"), Some("es"));
        assert_eq!(subtitle_language("es.srt"), Some("es"));
        assert_eq!(subtitle_language("film.castellano.srt"), Some("es"));
        assert_eq!(subtitle_language("film.eng.srt"), Some("en"));
        assert_eq!(subtitle_language("readme.srt"), None);
    }

    #[test]
    fn an_unrecognised_language_is_kept_rather_than_dropped() {
        let moves = plan_subtitle_moves(
            &PathBuf::from("/films/x/film.mkv"),
            &[PathBuf::from("/films/x/Subs/weird_name.srt")],
        );
        assert_eq!(moves[0].to, PathBuf::from("/films/x/film.und.srt"));
    }

    #[test]
    fn two_subtitles_in_one_language_do_not_collide() {
        let moves = plan_subtitle_moves(
            &PathBuf::from("/films/x/film.mkv"),
            &[
                PathBuf::from("/films/x/Subs/spanish.srt"),
                PathBuf::from("/films/x/Subs/es.srt"),
            ],
        );
        assert_eq!(moves[0].to, PathBuf::from("/films/x/film.es.srt"));
        assert_eq!(moves[1].to, PathBuf::from("/films/x/film.es.2.srt"));
    }

    #[test]
    fn subtitles_already_beside_the_film_are_left_alone() {
        let moves = plan_subtitle_moves(
            &PathBuf::from("/films/x/film.mkv"),
            &[PathBuf::from("/films/x/film.es.srt")],
        );
        assert!(moves.is_empty());
    }
}
