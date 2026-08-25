//! Turning releases into films, choosing between copies.

use crate::indexer::SearchResult;
use crate::release::{matches, names_another_language, Preference, Tag};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Film {
    pub title: String,
    pub year: Option<String>,
    pub imdb: Option<String>,
    pub cover_url: Option<String>,
    pub about: String,
    /// Best first; the first is downloaded.
    pub releases: Vec<SearchResult>,
}

impl Film {
    pub fn best(&self) -> Option<&SearchResult> {
        self.releases.first()
    }
}

const GIGABYTE: f64 = 1_073_741_824.0;
/// When the indexer states no runtime.
const TYPICAL_FILM_HOURS: f64 = 1.75;
const TOO_SMALL_TO_BE_THE_FILM_GB: f64 = 0.5;

/// What a good copy looks like.
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
    if let Preference::Language(code) = preference {
        if release.tags.contains(&Tag::Dub(code)) {
            score += 150.0;
        }
    }

    score += if title.contains("2160p") || title.contains("4k") {
        -60.0
    } else if title.contains("1080p") {
        90.0
    } else if title.contains("720p") {
        70.0
    } else if title.contains("480p") || title.contains("576p") || title.contains("360p") {
        -90.0
    } else {
        0.0
    };

    score += if title.contains("remux") {
        -150.0
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

    if title.contains("xvid") || title.contains("divx") || title.contains("dvdrip") {
        score -= 250.0;
    }
    if title.contains("cam") || title.contains("ts.") || title.contains("telesync") {
        score -= 400.0;
    }

    let gigabytes = release.size_bytes as f64 / GIGABYTE;
    let hours = runtime_minutes
        .map(|minutes| minutes / 60.0)
        .unwrap_or(TYPICAL_FILM_HOURS);
    score += density_score(gigabytes / hours.max(0.5));
    if gigabytes < TOO_SMALL_TO_BE_THE_FILM_GB {
        score -= 200.0;
    }

    score += popularity_score(release.grabs);
    score -= rot_risk(release);
    score -= sideshow(&title);

    if let Preference::Language(code) = preference {
        if names_another_language(&release.tags, code) && !release.tags.contains(&Tag::Dub(code)) {
            score -= 150.0;
        }
    }

    score
}

/// A watchable film against a living-room screen: harshest on the too-compressed, hardest on
/// remux territory, best around two gigabytes an hour.
fn density_score(gigabytes_per_hour: f64) -> f64 {
    match gigabytes_per_hour {
        rate if rate < 0.35 => -260.0,
        rate if rate < 0.6 => 60.0,
        rate if rate <= 1.8 => 220.0,
        rate if rate <= 3.0 => 60.0,
        rate if rate <= 6.0 => -40.0,
        _ => -200.0,
    }
}

/// A release nobody has taken is a release nobody has checked.
pub(crate) fn popularity_score(grabs: u64) -> f64 {
    (f64::from(grabs.max(1) as u32).log10() * 40.0).min(140.0)
}

/// Words meaning this is not the film.
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

/// How likely this copy is half gone.
pub fn rot_risk(release: &SearchResult) -> f64 {
    let mut risk = match release.age_days {
        Some(age) if age > 3650.0 => 120.0,
        Some(age) if age > 2200.0 => 40.0,
        _ => 0.0,
    };
    let complaints = release.thumbs_down.saturating_sub(release.thumbs_up);
    risk += f64::from(complaints.min(8)) * 15.0;
    risk
}

fn runtime_of(film: &Film) -> Option<f64> {
    film.about
        .split(" · ")
        .find_map(|part| part.strip_suffix(" min"))
        .and_then(|minutes| minutes.trim().parse().ok())
}

pub fn group(results: Vec<SearchResult>, preference: Preference) -> Vec<Film> {
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
            quality_score(&plain, Preference::Language("es"), Some(100.0))
                > quality_score(&german, Preference::Language("es"), Some(100.0))
        );
        assert_eq!(
            quality_score(&german, Preference::Any, Some(100.0)),
            quality_score(&plain, Preference::Any, Some(100.0))
        );
    }

    #[test]
    fn any_spanish_audio_matches_and_the_named_dub_ranks_first() {
        let castellano = release(
            "Film.2016.CASTELLANO.1080p.BluRay.x264-Es",
            2.0,
            500,
            Some("1"),
        );
        let latino = release("Film.2016.LATINO.1080p.BluRay.x264-La", 2.0, 500, Some("1"));
        let plain = release("Film.2016.1080p.BluRay.x264-Plain", 2.0, 500, Some("1"));
        let score = |release| quality_score(release, Preference::Language("es"), Some(100.0));
        assert!(score(&castellano) > score(&latino));
        assert!(
            score(&latino) > score(&plain),
            "Spanish audio of any variety beats a copy that promises none"
        );
    }

    #[test]
    fn another_households_language_earns_the_same_courtesy() {
        let french = release(
            "Film.2016.TRUEFRENCH.1080p.BluRay.x264-Fr",
            2.0,
            500,
            Some("1"),
        );
        let plain = release("Film.2016.1080p.BluRay.x264-Plain", 2.0, 500, Some("1"));
        assert!(
            quality_score(&french, Preference::Language("fr"), Some(100.0))
                > quality_score(&plain, Preference::Language("fr"), Some(100.0))
        );
        assert!(
            quality_score(&plain, Preference::Language("es"), Some(100.0))
                > quality_score(&french, Preference::Language("es"), Some(100.0)),
            "and a French dub sinks for a Spanish household"
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
            Preference::Language("es"),
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
