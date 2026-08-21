//! Choosing and repairing subtitles. Sync is what actually fails, so timing signals outrank taste.

use regex::bytes::Regex as ByteRegex;
use regex::Regex;
use std::sync::OnceLock;

/// One result from the subtitle service, reduced to what a decision needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Candidate {
    pub file_id: i64,
    pub release: String,
    /// Uploaded against a file with the same hash, so it cannot drift.
    pub hash_match: bool,
    pub fps: Option<f64>,
    pub downloads: u64,
    pub rating: f64,
    pub trusted: bool,
    pub machine_translated: bool,
    /// Translates only the foreign dialogue: nearly blank as a main track.
    pub foreign_parts_only: bool,
    pub uploader: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub candidate: Candidate,
    pub score: f64,
    pub reasons: Vec<&'static str>,
}

/// What the film itself says about how a subtitle must be timed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MediaInfo {
    pub fps: Option<f64>,
    pub duration_seconds: Option<f64>,
}

pub fn rank(candidates: Vec<Candidate>, reference_name: &str, media: MediaInfo) -> Vec<Ranked> {
    let mut ranked: Vec<Ranked> = candidates
        .into_iter()
        .filter(|candidate| !candidate.foreign_parts_only)
        .map(|candidate| score(candidate, reference_name, media))
        .collect();
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    ranked
}

fn score(candidate: Candidate, reference_name: &str, media: MediaInfo) -> Ranked {
    let mut score = 0.0;
    let mut reasons = Vec::new();

    if candidate.hash_match {
        score += 1000.0;
        reasons.push("matches this exact file");
    }
    if let (Some(subtitle_fps), Some(film_fps)) = (candidate.fps, media.fps) {
        if subtitle_fps > 0.0 && (subtitle_fps - film_fps).abs() < 0.05 {
            score += 400.0;
            reasons.push("same frame rate");
        }
    }
    let overlap = name_overlap(reference_name, &candidate.release);
    score += 300.0 * overlap;
    if overlap >= 0.4 {
        reasons.push("same kind of release");
    }
    if candidate.trusted {
        score += 60.0;
        reasons.push("trusted uploader");
    }
    if candidate.machine_translated {
        score -= 400.0;
        reasons.push("machine translated");
    }
    score += (f64::from(candidate.downloads.max(1) as u32).log10() * 40.0).min(120.0);
    score += (candidate.rating * 6.0).min(60.0);

    Ranked {
        candidate,
        score,
        reasons,
    }
}

fn words(text: &str) -> Vec<String> {
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    let split = SPLIT.get_or_init(|| Regex::new(r"[^0-9a-zA-Z]+").expect("pattern compiles"));
    split
        .split(&text.to_lowercase())
        .filter(|word| word.len() > 2)
        .map(str::to_string)
        .collect()
}

pub fn name_overlap(ours: &str, theirs: &str) -> f64 {
    let mine = words(ours);
    if mine.is_empty() {
        return 0.0;
    }
    let theirs = words(theirs);
    let shared = mine.iter().filter(|word| theirs.contains(word)).count();
    shared as f64 / mine.len() as f64
}

/// A subtitle authored at 25 fps runs about 4% fast against a 23.976 fps copy of the same film.
pub fn frame_rate_factor(subtitle_fps: Option<f64>, film_fps: Option<f64>) -> Option<f64> {
    let (subtitle_fps, film_fps) = (subtitle_fps?, film_fps?);
    if subtitle_fps <= 0.0 || film_fps <= 0.0 || (subtitle_fps - film_fps).abs() < 0.05 {
        return None;
    }
    Some(subtitle_fps / film_fps)
}

/// Rewrites timestamps in place on the raw bytes, so the file's own encoding survives untouched.
pub fn rescale(content: &[u8], factor: f64) -> Vec<u8> {
    static TIMESTAMP: OnceLock<ByteRegex> = OnceLock::new();
    let timestamp = TIMESTAMP.get_or_init(|| {
        ByteRegex::new(r"(\d{2}):(\d{2}):(\d{2}),(\d{3})").expect("pattern compiles")
    });

    timestamp
        .replace_all(content, |caps: &regex::bytes::Captures| {
            let number = |index: usize| -> u64 {
                std::str::from_utf8(&caps[index])
                    .ok()
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(0)
            };
            let millis =
                ((number(1) * 3600 + number(2) * 60 + number(3)) * 1000 + number(4)) as f64;
            let scaled = (millis * factor).round().max(0.0) as u64;
            let seconds = scaled / 1000;
            format!(
                "{:02}:{:02}:{:02},{:03}",
                seconds / 3600,
                seconds % 3600 / 60,
                seconds % 60,
                scaled % 1000
            )
            .into_bytes()
        })
        .into_owned()
}

pub fn last_cue_seconds(content: &[u8]) -> Option<f64> {
    static TIMESTAMP: OnceLock<ByteRegex> = OnceLock::new();
    let timestamp = TIMESTAMP.get_or_init(|| {
        ByteRegex::new(r"(\d{2}):(\d{2}):(\d{2}),(\d{3})").expect("pattern compiles")
    });

    timestamp
        .captures_iter(content)
        .map(|caps| {
            let number = |index: usize| -> f64 {
                std::str::from_utf8(&caps[index])
                    .ok()
                    .and_then(|text| text.parse().ok())
                    .unwrap_or(0.0)
            };
            number(1) * 3600.0 + number(2) * 60.0 + number(3) + number(4) / 1000.0
        })
        .fold(None, |best: Option<f64>, value| {
            Some(best.map_or(value, |best| best.max(value)))
        })
}

/// A cue after the film has ended is always wrong.
const OVERRUN_TOLERANCE: f64 = 0.02;
/// Ending early is normal: credits carry no dialogue.
const UNDERRUN_TOLERANCE: f64 = 0.15;
/// Below this many cues, stopping early says nothing about which cut it was timed for. The Red
/// Turtle has almost no dialogue at all: twenty-eight lines across eighty minutes, the last at
/// sixty-two. Judging that file by how much of the runtime it covers throws away the only Spanish
/// subtitle the film has.
const ENOUGH_CUES_TO_JUDGE: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub enum Timing {
    Plausible,
    NoTimings,
    RunsPastTheEnd { last_cue_minutes: f64 },
    TimedForAnotherCut { last_cue_minutes: f64 },
}

pub fn check_timing(content: &[u8], duration_seconds: Option<f64>) -> Timing {
    let Some(duration) = duration_seconds.filter(|value| *value > 0.0) else {
        return Timing::Plausible;
    };
    let Some(span) = last_cue_seconds(content) else {
        return Timing::NoTimings;
    };

    let drift = (span - duration) / duration;
    if drift > OVERRUN_TOLERANCE {
        return Timing::RunsPastTheEnd {
            last_cue_minutes: span / 60.0,
        };
    }
    // a talkative film timed to the wrong cut still has hundreds of cues; a quiet one has a handful
    let talkative = count_cues(content) >= ENOUGH_CUES_TO_JUDGE;
    if talkative && -drift > UNDERRUN_TOLERANCE {
        return Timing::TimedForAnotherCut {
            last_cue_minutes: span / 60.0,
        };
    }
    Timing::Plausible
}

pub fn count_cues(content: &[u8]) -> usize {
    static TIMESTAMP: OnceLock<ByteRegex> = OnceLock::new();
    let timestamp = TIMESTAMP.get_or_init(|| {
        ByteRegex::new(r"(\d{2}):(\d{2}):(\d{2}),(\d{3})").expect("pattern compiles")
    });
    timestamp.find_iter(content).count() / 2
}

/// The subtitle service's own hash: the file size plus its first and last 64 KiB, summed as u64.
pub fn movie_hash(size: u64, head: &[u8], tail: &[u8]) -> Option<String> {
    const CHUNK: usize = 65536;
    if head.len() < CHUNK || tail.len() < CHUNK {
        return None;
    }
    let sum_chunk = |bytes: &[u8], start: u64| -> u64 {
        bytes.chunks_exact(8).fold(start, |total, word| {
            total.wrapping_add(u64::from_le_bytes(word.try_into().expect("eight bytes")))
        })
    };
    let digest = sum_chunk(tail, sum_chunk(head, size));
    Some(format!("{digest:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(file_id: i64) -> Candidate {
        Candidate {
            file_id,
            ..Candidate::default()
        }
    }

    fn film() -> MediaInfo {
        MediaInfo {
            fps: Some(23.976),
            duration_seconds: Some(12488.5),
        }
    }

    #[test]
    fn an_exact_file_match_beats_a_wildly_popular_one() {
        let matched = Candidate {
            hash_match: true,
            downloads: 3,
            ..candidate(1)
        };
        let popular = Candidate {
            downloads: 500_000,
            ..candidate(2)
        };
        let ranked = rank(vec![popular, matched], "", film());
        assert!(ranked[0].candidate.hash_match);
    }

    #[test]
    fn matching_frame_rate_beats_popularity() {
        let same = Candidate {
            fps: Some(23.976),
            downloads: 10,
            ..candidate(1)
        };
        let wrong = Candidate {
            fps: Some(25.0),
            downloads: 100_000,
            ..candidate(2)
        };
        let ranked = rank(vec![wrong, same], "", film());
        assert_eq!(ranked[0].candidate.file_id, 1);
        assert!(ranked[0].reasons.contains(&"same frame rate"));
    }

    #[test]
    fn a_foreign_parts_only_subtitle_is_never_offered() {
        let ranked = rank(
            vec![Candidate {
                foreign_parts_only: true,
                downloads: 900_000,
                ..candidate(1)
            }],
            "",
            film(),
        );
        assert!(ranked.is_empty());
    }

    #[test]
    fn machine_translations_sink_below_human_ones() {
        let machine = Candidate {
            machine_translated: true,
            downloads: 90_000,
            ..candidate(1)
        };
        let human = Candidate {
            downloads: 20,
            ..candidate(2)
        };
        let ranked = rank(vec![machine, human], "", film());
        assert_eq!(ranked[0].candidate.file_id, 2);
    }

    #[test]
    fn popularity_only_breaks_ties() {
        let popular = score(
            Candidate {
                downloads: 100_000,
                ..candidate(1)
            },
            "",
            film(),
        );
        let obscure = score(
            Candidate {
                downloads: 1,
                ..candidate(2)
            },
            "",
            film(),
        );
        assert!(popular.score - obscure.score < 400.0);
    }

    #[test]
    fn a_pal_subtitle_is_stretched_onto_the_film() {
        let rescaled = rescale(b"1\n00:10:00,000 --> 00:10:02,000\nhola\n", 25.0 / 23.976);
        assert_eq!(
            String::from_utf8_lossy(&rescaled),
            "1\n00:10:25,626 --> 00:10:27,711\nhola\n"
        );
    }

    #[test]
    fn drift_accumulates_across_the_film() {
        let rescaled = rescale(b"1\n03:19:12,000 --> 03:19:14,000\nfin\n", 25.0 / 23.976);
        assert!(String::from_utf8_lossy(&rescaled).contains("03:27:42"));
    }

    #[test]
    fn the_text_and_its_encoding_are_left_alone() {
        let latin1 = [
            b"1\n00:00:01,000 --> 00:00:02,000\n".to_vec(),
            vec![0xBF, 0x44, 0xF3, 0x6E, 0x64, 0x65],
            b"\n".to_vec(),
        ]
        .concat();
        let rescaled = rescale(&latin1, 1.0427);
        assert!(rescaled
            .windows(6)
            .any(|window| window == [0xBF, 0x44, 0xF3, 0x6E, 0x64, 0x65]));
    }

    #[test]
    fn only_converts_when_the_frame_rates_actually_differ() {
        assert_eq!(frame_rate_factor(Some(23.976), Some(23.976)), None);
        assert_eq!(frame_rate_factor(Some(0.0), Some(23.976)), None);
        assert_eq!(frame_rate_factor(None, Some(23.976)), None);
        assert_eq!(frame_rate_factor(Some(25.0), None), None);
        let factor = frame_rate_factor(Some(25.0), Some(23.976)).expect("a conversion");
        assert!((factor - 1.0427).abs() < 0.0001);
    }

    /// The real case: a 25 fps subtitle whose last cue lands at 199.2 minutes on a 208 minute film.
    /// A 4% drift hides inside the tolerance that lets subtitles stop before the credits, so the
    /// duration check cannot catch this on its own. The frame rate the service reports can.
    #[test]
    fn a_converted_subtitle_lands_where_the_film_ends() {
        let film_seconds = 12488.5;
        let pal = b"1\n03:19:09,550 --> 03:19:12,000\nfin\n";
        assert_eq!(
            check_timing(pal, Some(film_seconds)),
            Timing::Plausible,
            "duration alone cannot tell this apart from a film with long credits"
        );

        let before = (last_cue_seconds(pal).unwrap() - film_seconds).abs();
        let converted = rescale(pal, frame_rate_factor(Some(25.0), Some(23.976)).unwrap());
        let after = (last_cue_seconds(&converted).unwrap() - film_seconds).abs();

        assert!(before > 500.0, "it was more than eight minutes out");
        assert!(
            after < 30.0,
            "conversion lands it within half a minute of the end"
        );
        assert_eq!(
            check_timing(&converted, Some(film_seconds)),
            Timing::Plausible
        );
    }

    #[test]
    fn rejects_cues_that_land_after_the_film_has_ended() {
        let overrun = b"1\n02:10:00,000 --> 02:10:02,000\nhola\n";
        assert!(matches!(
            check_timing(overrun, Some(7200.0)),
            Timing::RunsPastTheEnd { .. }
        ));
    }

    /// The Red Turtle: twenty-eight lines, the last at sixty-two minutes of eighty. Nothing about
    /// that file is wrong, and rejecting it leaves her with no subtitles at all.
    #[test]
    fn accepts_a_quiet_film_whose_subtitle_covers_little_of_it() {
        let mut sparse = String::new();
        for cue in 0..14 {
            let minute = cue * 4;
            sparse.push_str(&format!(
                "{}\n00:{minute:02}:00,000 --> 00:{minute:02}:02,000\nalgo\n\n",
                cue + 1
            ));
        }
        assert_eq!(
            check_timing(sparse.as_bytes(), Some(4878.0)),
            Timing::Plausible
        );
    }

    #[test]
    fn still_rejects_a_talkative_film_timed_for_another_cut() {
        let mut dense = String::new();
        for cue in 0..400 {
            let seconds = cue * 20;
            dense.push_str(&format!(
                "{}\n{:02}:{:02}:{:02},000 --> {:02}:{:02}:{:02},500\nlínea\n\n",
                cue + 1,
                seconds / 3600,
                seconds % 3600 / 60,
                seconds % 60,
                seconds / 3600,
                seconds % 3600 / 60,
                seconds % 60,
            ));
        }
        // the last cue lands at 2 hours 13, against a 3 hour 28 film
        assert!(matches!(
            check_timing(dense.as_bytes(), Some(12488.0)),
            Timing::TimedForAnotherCut { .. }
        ));
    }

    #[test]
    fn allows_a_subtitle_that_stops_before_the_credits() {
        let early = b"1\n01:58:00,000 --> 01:58:02,000\nfin\n";
        assert_eq!(check_timing(early, Some(7500.0)), Timing::Plausible);
    }

    #[test]
    fn says_so_when_there_are_no_timings_at_all() {
        assert_eq!(
            check_timing(b"not a subtitle", Some(7000.0)),
            Timing::NoTimings
        );
    }

    #[test]
    fn cannot_complain_without_knowing_the_duration() {
        let anything = b"1\n09:00:00,000 --> 09:00:02,000\nx\n";
        assert_eq!(check_timing(anything, None), Timing::Plausible);
    }

    #[test]
    fn the_hash_needs_both_ends_of_the_file() {
        assert_eq!(movie_hash(1000, &[0; 10], &[0; 10]), None);
        let digest = movie_hash(5_192_014_484, &[0; 65536], &[0; 65536]).expect("a hash");
        assert_eq!(digest.len(), 16);
    }
}
