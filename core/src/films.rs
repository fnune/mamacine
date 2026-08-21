//! Turning a list of releases into a list of films.
//!
//! A search for one film returns dozens of releases of it, all with the same poster and different
//! technical names. Choosing between them is this module's job, so that it is never hers.

use crate::indexer::SearchResult;
use crate::release::{matches, Preference, Tag};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Film {
    pub title: String,
    pub year: Option<String>,
    pub imdb: Option<String>,
    pub cover_url: Option<String>,
    pub about: String,
    /// Best first. The first is what a tap on the poster downloads.
    pub releases: Vec<SearchResult>,
}

impl Film {
    pub fn best(&self) -> Option<&SearchResult> {
        self.releases.first()
    }
}

const GIGABYTE: f64 = 1_073_741_824.0;

/// What a good copy looks like, in the order the signals actually matter.
pub fn quality_score(
    release: &SearchResult,
    preference: Preference,
    runtime_minutes: Option<f64>,
) -> f64 {
    let title = release.title.to_lowercase();
    let mut score = 0.0;

    if matches(&release.tags, preference) {
        score += 500.0;
    }
    if release.tags.contains(&Tag::Spanish) && preference == Preference::Spanish {
        score += 150.0; // named outright, rather than a dual that might carry it
    }

    // Resolution counts for less than size: a 720p copy she can watch tonight beats a 1080p copy
    // that fills her disk, and both look the same across a living room.
    score += if title.contains("2160p") || title.contains("4k") {
        -60.0
    } else if title.contains("1080p") {
        90.0
    } else if title.contains("720p") {
        70.0
    } else if title.contains("480p") || title.contains("576p") || title.contains("360p") {
        -90.0 // small enough to be tempting, and soft enough to spoil the film on a television
    } else {
        0.0
    };

    score += if title.contains("remux") {
        -150.0 // tens of gigabytes for a difference she will not see
    } else if title.contains("bluray") || title.contains("blu-ray") {
        120.0
    } else if title.contains("web-dl") || title.contains("webdl") {
        110.0
    } else if title.contains("webrip") {
        80.0
    } else if title.contains("hdtv") {
        30.0
    } else {
        0.0
    };

    // the encodes that rot: old scene releases whose articles are half gone
    if title.contains("xvid") || title.contains("divx") || title.contains("dvdrip") {
        score -= 250.0;
    }
    if title.contains("cam") || title.contains("ts.") || title.contains("telesync") {
        score -= 400.0;
    }

    // She watches on a normal screen, so the copy to take is the smallest one that still looks like
    // a film. Around two gigabytes for a feature is plenty; four times that buys her nothing and
    // costs her disk, her connection and her patience. Judged per hour, because two gigabytes is
    // generous for a ninety minute film and thin for a three and a half hour one.
    let gigabytes = release.size_bytes as f64 / GIGABYTE;
    let hours = runtime_minutes
        .map(|minutes| minutes / 60.0)
        .unwrap_or(1.75);
    let per_hour = gigabytes / hours.max(0.5);
    score += match per_hour {
        rate if rate < 0.35 => -260.0, // too compressed to be worth the evening
        rate if rate < 0.6 => 60.0,
        rate if rate <= 1.8 => 220.0, // the copy to take
        rate if rate <= 3.0 => 60.0,  // twice the size, for a difference she will not see
        rate if rate <= 6.0 => -40.0,
        _ => -200.0, // remux territory: tens of gigabytes for a difference she will not see
    };
    if gigabytes < 0.5 {
        score -= 200.0; // whatever this is, it is not the film
    }

    // a release nobody has taken is a release nobody has checked
    score += (f64::from(release.grabs.max(1) as u32).log10() * 40.0).min(140.0);
    score -= rot_risk(release);
    score -= sideshow(&title);

    // a dub into a language she never asked for serves her worse than the original with
    // subtitles; seen live as a German.DL season pack sitting in the Spanish fall-through
    if preference == Preference::Spanish
        && release.tags.contains(&Tag::OtherLanguage)
        && !release.tags.contains(&Tag::Spanish)
    {
        score -= 150.0;
    }

    score
}

/// Words that mean this is not actually the film: seen live as "Coco Sing-Along" carrying Coco's
/// own id, one dead copy away from being downloaded in its place.
pub fn sideshow(title: &str) -> f64 {
    const SIDESHOWS: [&str; 12] = [
        "sing along",
        "singalong",
        "karaoke",
        "making of",
        "behind the scenes",
        "premiere special",
        "episode review",
        "recap",
        "trailer",
        "sample",
        "commentary",
        "soundtrack",
    ];
    let plain: String = title
        .to_lowercase()
        .chars()
        .map(|letter| {
            if letter.is_alphanumeric() {
                letter
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    SIDESHOWS
        .iter()
        .filter(|marker| plain.contains(*marker))
        .count() as f64
        * 400.0
}

/// How likely this copy is to be half gone.
///
/// The indexer publishes no completeness figure, so this is inferred. Age is the mechanism:
/// providers drop articles as a post ages, and a ten year old encode is missing parts of itself.
/// Votes are the only direct evidence there is, and they are evidence of exactly one thing, since
/// nobody downvotes a release that worked. There are rarely more than a dozen of them, so they are
/// worth less than they look: enough to break a tie, not enough to overrule a copy thousands of
/// people have taken.
pub fn rot_risk(release: &SearchResult) -> f64 {
    let mut risk = match release.age_days {
        Some(age) if age > 3650.0 => 120.0, // a decade on, articles start going missing
        Some(age) if age > 2200.0 => 40.0,
        _ => 0.0,
    };
    let complaints = release.thumbs_down.saturating_sub(release.thumbs_up);
    risk += f64::from(complaints.min(8)) * 15.0;
    risk
}

/// The indexer reports a film's length as "149 min" among its other facts.
fn runtime_of(film: &Film) -> Option<f64> {
    film.about
        .split(" · ")
        .find_map(|part| part.strip_suffix(" min"))
        .and_then(|minutes| minutes.trim().parse().ok())
}

pub fn group(results: Vec<SearchResult>, preference: Preference) -> Vec<Film> {
    // keyed once, when the group is created: deriving it again from the display title would
    // compare a film's name against a release's name and never match
    let mut grouped: Vec<(String, Film)> = Vec::new();

    for release in results {
        let key = film_key(&release);
        match grouped.iter_mut().find(|(existing, _)| existing == &key) {
            Some((_, film)) => {
                if film.cover_url.is_none() {
                    film.cover_url = release.cover_url.clone();
                }
                if film.about.is_empty() {
                    film.about = release.about.clone();
                }
                film.releases.push(release);
            }
            None => grouped.push((
                key,
                Film {
                    title: display_title(&release),
                    year: year_of(&release),
                    imdb: release.imdb.clone(),
                    cover_url: release.cover_url.clone(),
                    about: release.about.clone(),
                    releases: vec![release],
                },
            )),
        }
    }

    let mut films: Vec<Film> = grouped.into_iter().map(|(_, film)| film).collect();
    for film in &mut films {
        let runtime = runtime_of(film);
        film.releases.sort_by(|left, right| {
            quality_score(right, preference, runtime)
                .partial_cmp(&quality_score(left, preference, runtime))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    films.retain(|film| !film.releases.is_empty());
    films
}

fn film_key(release: &SearchResult) -> String {
    match &release.imdb {
        Some(id) => format!("imdb:{id}"),
        // without an id, fall back to the words before the year
        None => format!("name:{}", normalised_name(&release.title)),
    }
}

fn normalised_name(title: &str) -> String {
    let mut words = Vec::new();
    for word in title.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        if is_year(word) {
            break;
        }
        words.push(word.to_lowercase());
    }
    if words.is_empty() {
        title.to_lowercase()
    } else {
        words.join(" ")
    }
}

fn is_year(word: &str) -> bool {
    word.len() == 4
        && word.chars().all(|c| c.is_ascii_digit())
        && matches!(word.parse::<u16>(), Ok(1888..=2100))
}

fn display_title(release: &SearchResult) -> String {
    let from_metadata = release.about.split(" · ").next().unwrap_or("").trim();
    if !from_metadata.is_empty() && !from_metadata.contains('.') {
        return from_metadata.to_string();
    }
    let name = normalised_name(&release.title);
    name.split(' ')
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn year_of(release: &SearchResult) -> Option<String> {
    release
        .about
        .split(" · ")
        .find(|part| is_year(part))
        .map(str::to_string)
        .or_else(|| {
            release
                .title
                .split(|c: char| !c.is_alphanumeric())
                .find(|word| is_year(word))
                .map(str::to_string)
        })
}

#[cfg(test)]
mod tests {

    // Measured against the real listing for Das Boot: a 5.5 GB 1080p copy was being chosen over a
    // 1.7 GB 720p one. Twice the size buys nothing across her living room, and costs her an evening
    // of downloading.
    #[test]
    fn a_copy_twice_the_size_does_not_win_on_resolution_alone() {
        let big = release(
            "Das Boot 1981 1080p BluRay H.265-iVy",
            5.5,
            1305,
            Some("0082096"),
        );
        let right = release(
            "Das Boot 1981 720p BluRay x264-Ozlem",
            1.7,
            528,
            Some("0082096"),
        );
        assert!(
            quality_score(&right, Preference::Original, Some(149.0))
                > quality_score(&big, Preference::Original, Some(149.0))
        );
    }

    #[test]
    fn small_is_not_the_same_as_good_enough_to_watch() {
        let soft = release("Das Boot 1981 BluRay 480p H264", 1.6, 1107, Some("0082096"));
        let right = release(
            "Das Boot 1981 720p BluRay x264-Ozlem",
            1.7,
            528,
            Some("0082096"),
        );
        assert!(
            quality_score(&right, Preference::Original, Some(149.0))
                > quality_score(&soft, Preference::Original, Some(149.0)),
            "the smallest copy is only the one to take while it still looks like a film",
        );
    }

    fn voted(title: &str, up: u32, down: u32, age: f64) -> SearchResult {
        SearchResult {
            thumbs_up: up,
            thumbs_down: down,
            age_days: Some(age),
            ..release(title, 2.2, 500, Some("0082096"))
        }
    }

    // Nobody downvotes a release that worked. It is the only evidence of completeness the indexer
    // publishes: the Game of Thrones pack that arrived 6% short had eight downvotes and no upvotes.
    #[test]
    fn a_copy_people_complained_about_ranks_below_one_they_did_not() {
        let complained = voted("Das Boot 1981 1080p BluRay x264-A", 0, 8, 400.0);
        let quiet = voted("Das Boot 1981 1080p BluRay x264-B", 0, 0, 400.0);
        assert!(
            quality_score(&quiet, Preference::Any, Some(149.0))
                > quality_score(&complained, Preference::Any, Some(149.0))
        );
    }

    #[test]
    fn upvotes_answer_downvotes_rather_than_being_counted_twice() {
        let argued = voted("Das Boot 1981 1080p BluRay x264-A", 6, 6, 400.0);
        let quiet = voted("Das Boot 1981 1080p BluRay x264-B", 0, 0, 400.0);
        assert_eq!(
            quality_score(&argued, Preference::Any, Some(149.0)),
            quality_score(&quiet, Preference::Any, Some(149.0)),
        );
    }

    // Otherwise a handful of votes would overrule the thousands of people it worked for.
    #[test]
    fn a_few_complaints_never_outweigh_a_copy_everybody_takes() {
        let popular = SearchResult {
            grabs: 6184,
            thumbs_down: 8,
            ..voted("Das Boot 1981 1080p BluRay x264-A", 0, 8, 400.0)
        };
        let obscure = SearchResult {
            grabs: 2,
            ..voted("Das Boot 1981 1080p BluRay x264-B", 0, 0, 400.0)
        };
        assert!(
            quality_score(&popular, Preference::Any, Some(149.0))
                > quality_score(&obscure, Preference::Any, Some(149.0))
        );
    }

    #[test]
    fn an_older_post_is_treated_as_the_riskier_one() {
        let old = voted("Das Boot 1981 1080p BluRay x264-A", 0, 0, 3700.0);
        let recent = voted("Das Boot 1981 1080p BluRay x264-B", 0, 0, 300.0);
        assert!(rot_risk(&old) > rot_risk(&recent));
        assert_eq!(rot_risk(&recent), 0.0);
    }
    use super::*;
    use crate::release::tags;

    fn release(title: &str, size_gb: f64, grabs: u64, imdb: Option<&str>) -> SearchResult {
        // an indexer only sends metadata when it recognised the film, which is when it has an id
        let about = match imdb {
            Some(_) => "Das Boot · 1981 · ★8.4 · Drama, War · 149 min".to_string(),
            None => String::new(),
        };
        SearchResult {
            tags: tags(title),
            title: title.to_string(),
            nzb_url: format!("https://indexer.test/{title}"),
            size_bytes: (size_gb * GIGABYTE) as u64,
            age_days: Some(400.0),
            grabs,
            cover_url: Some("https://indexer.test/cover.jpg".into()),
            imdb: imdb.map(str::to_string),
            about,
            thumbs_up: 0,
            thumbs_down: 0,
        }
    }

    #[test]
    fn one_card_per_film_however_many_releases_it_has() {
        let films = group(
            vec![
                release(
                    "Das.Boot.1981.1080p.BluRay.x264-A",
                    8.0,
                    500,
                    Some("0082096"),
                ),
                release("Das.Boot.1981.720p.WEB-DL-B", 3.0, 200, Some("0082096")),
                release("Persepolis.2007.1080p.BluRay-C", 6.0, 90, Some("0808417")),
            ],
            Preference::Any,
        );
        assert_eq!(films.len(), 2);
        assert_eq!(films[0].releases.len(), 2);
        assert_eq!(films[0].imdb.as_deref(), Some("0082096"));
    }

    #[test]
    fn takes_the_smaller_copy_when_both_are_proper_films() {
        let films = group(
            vec![
                release(
                    "Film.2016.1080p.BluRay.x264-BIG",
                    12.0,
                    900,
                    Some("0082096"),
                ),
                release(
                    "Film.2016.1080p.BluRay.x265-SMALL",
                    2.1,
                    120,
                    Some("0082096"),
                ),
            ],
            Preference::Any,
        );
        assert!(films[0].best().expect("a pick").title.contains("SMALL"));
    }

    #[test]
    fn refuses_a_copy_too_compressed_to_be_worth_it() {
        let films = group(
            vec![
                release("Film.2016.480p.WEB-TINY", 0.55, 4000, Some("0082096")),
                release("Film.2016.720p.WEB-DL-FINE", 1.9, 30, Some("0082096")),
            ],
            Preference::Any,
        );
        assert!(films[0].best().expect("a pick").title.contains("FINE"));
    }

    /// Two gigabytes is generous for ninety minutes and thin for three and a half hours.
    #[test]
    fn judges_size_against_how_long_the_film_runs() {
        let short = quality_score(
            &release("Film.1080p.BluRay-A", 2.0, 100, Some("1")),
            Preference::Any,
            Some(95.0),
        );
        let long = quality_score(
            &release("Film.1080p.BluRay-A", 2.0, 100, Some("1")),
            Preference::Any,
            Some(208.0),
        );
        assert!(
            short > long,
            "the same file is a better copy of a shorter film"
        );
    }

    #[test]
    fn prefers_a_normal_sized_bluray_over_a_giant_remux() {
        let films = group(
            vec![
                release(
                    "Das.Boot.1981.2160p.REMUX.AVC.DTS-HD-X",
                    42.0,
                    30,
                    Some("0082096"),
                ),
                release(
                    "Das.Boot.1981.1080p.BluRay.H.265-iVy",
                    5.5,
                    300,
                    Some("0082096"),
                ),
            ],
            Preference::Any,
        );
        assert!(films[0].best().expect("a pick").title.contains("H.265-iVy"));
    }

    #[test]
    fn avoids_the_old_encodes_that_rot() {
        let films = group(
            vec![
                release(
                    "Das.Boot.1981.German.AC3.HDRip.XViD-FuN",
                    2.5,
                    900,
                    Some("0082096"),
                ),
                release(
                    "Das.Boot.1981.1080p.BluRay.x264-Good",
                    8.0,
                    120,
                    Some("0082096"),
                ),
            ],
            Preference::Any,
        );
        assert!(
            films[0].best().expect("a pick").title.contains("x264-Good"),
            "a popular XviD is still a bad bet"
        );
    }

    // Seen live: the indexer filed "Coco Sing-Along" under Coco's own id, so it sat in the group
    // as an ordinary copy, one dead fall-through away from being the film she got.
    #[test]
    fn a_sing_along_never_outranks_the_film_itself() {
        let films = group(
            vec![
                release(
                    "Coco.Sing-Along.2022.MULTI.1080p.DSNP.WEB-DL",
                    2.0,
                    5000,
                    Some("2380307"),
                ),
                release(
                    "Coco.2017.1080p.BluRay.x265-Plain",
                    3.1,
                    40,
                    Some("2380307"),
                ),
            ],
            Preference::Any,
        );
        assert!(films[0].best().expect("a pick").title.contains("BluRay"));
    }

    // Seen live: a German.DL season pack sat third in the Spanish fall-through, ahead of plain
    // English copies whose missing Spanish at least has a subtitles remedy.
    #[test]
    fn a_dub_into_a_third_language_sinks_when_she_wants_spanish() {
        let german = release(
            "Film.2016.German.DL.1080p.BluRay.x264-GTVG",
            2.0,
            500,
            Some("1"),
        );
        let plain = release("Film.2016.1080p.BluRay.x264-Plain", 2.0, 500, Some("1"));
        assert!(
            quality_score(&plain, Preference::Spanish, Some(100.0))
                > quality_score(&german, Preference::Spanish, Some(100.0))
        );
        // the original of a French film is not punished under Original or Any
        assert_eq!(
            quality_score(&german, Preference::Any, Some(100.0)),
            quality_score(&plain, Preference::Any, Some(100.0))
        );
    }

    #[test]
    fn honours_the_language_she_asked_for() {
        let films = group(
            vec![
                release(
                    "Das.Boot.1981.1080p.BluRay.x264-English",
                    8.0,
                    5000,
                    Some("0082096"),
                ),
                release(
                    "Das.Boot.1981.SPANISH.1080p.BluRay.x264-Es",
                    8.0,
                    40,
                    Some("0082096"),
                ),
            ],
            Preference::Spanish,
        );
        assert!(films[0].best().expect("a pick").title.contains("SPANISH"));
    }

    #[test]
    fn refuses_a_file_too_small_to_be_the_film() {
        let films = group(
            vec![
                release("Das.Boot.1981.SAMPLE.1080p", 0.05, 10, Some("0082096")),
                release("Das.Boot.1981.1080p.BluRay-Full", 7.0, 10, Some("0082096")),
            ],
            Preference::Any,
        );
        assert!(films[0].best().expect("a pick").title.contains("Full"));
    }

    #[test]
    fn falls_back_to_the_release_name_for_a_title() {
        let films = group(
            vec![release(
                "Some.Obscure.Film.2015.1080p.WEB-DL-A",
                4.0,
                5,
                None,
            )],
            Preference::Any,
        );
        assert_eq!(films[0].title, "Some Obscure Film");
        assert_eq!(films[0].year.as_deref(), Some("2015"));
    }

    #[test]
    fn groups_by_name_when_the_indexer_gives_no_id() {
        let films = group(
            vec![
                release("Some.Obscure.Film.2015.1080p.WEB-DL-A", 4.0, 5, None),
                release("Some.Obscure.Film.2015.720p.HDTV-B", 2.0, 3, None),
            ],
            Preference::Any,
        );
        assert_eq!(films.len(), 1);
        assert_eq!(films[0].releases.len(), 2);
    }

    #[test]
    fn shows_the_films_name_rather_than_the_release_name() {
        let films = group(
            vec![release(
                "Das.Boot.1981.1080p.BluRay.x264-iVy",
                8.0,
                100,
                Some("0082096"),
            )],
            Preference::Any,
        );
        assert_eq!(films[0].title, "Das Boot");
        assert_eq!(films[0].year.as_deref(), Some("1981"));
    }
}
