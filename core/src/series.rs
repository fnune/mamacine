//! Television, whole season packs only.

use crate::indexer::SearchResult;
use crate::release::{matches, names_another_language, Preference, Tag};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One episode, never offered here.
    Episode,
    /// A whole season, as one download.
    Season(u32),
    /// Several seasons at once.
    Seasons(u32, u32),
}

/// An episode; a name nobody stated stays absent.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Episode {
    pub season: u32,
    pub number: u32,
    pub title: Option<String>,
    /// What happens in it, when known.
    pub overview: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Season {
    pub show: String,
    pub first: u32,
    pub last: u32,
    /// Best first, as `films` orders copies.
    pub releases: Vec<SearchResult>,
}

impl Season {
    pub fn best(&self) -> Option<&SearchResult> {
        self.releases.first()
    }
}

fn pattern(cell: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(source).expect("pattern compiles"))
}

/// What a release is, from its name.
pub fn shape_of(title: &str) -> Option<Shape> {
    static EPISODE: OnceLock<Regex> = OnceLock::new();
    static RANGE: OnceLock<Regex> = OnceLock::new();
    static SEASON: OnceLock<Regex> = OnceLock::new();
    static WORDED: OnceLock<Regex> = OnceLock::new();

    let title = title.to_lowercase();

    if pattern(
        &EPISODE,
        r"(?i)\b(s\d{1,2}[\s._-]?ep?\d{1,3}|\d{1,2}x\d{2})\b",
    )
    .is_match(&title)
    {
        return Some(Shape::Episode);
    }
    if let Some(found) =
        pattern(&RANGE, r"(?i)\bs(\d{1,2})[\s._-]*[-–][\s._-]*s?(\d{1,2})\b").captures(&title)
    {
        let first: u32 = found[1].parse().ok()?;
        let last: u32 = found[2].parse().ok()?;
        return Some(Shape::Seasons(first.min(last), first.max(last)));
    }
    if let Some(found) = pattern(&SEASON, r"(?i)\bs(\d{1,2})\b").captures(&title) {
        return found[1].parse().ok().map(Shape::Season);
    }
    if let Some(found) =
        pattern(&WORDED, r"(?i)\b(?:season|temporada)[\s._-]*(\d{1,2})\b").captures(&title)
    {
        return found[1].parse().ok().map(Shape::Season);
    }
    None
}

/// Season and episode; bare `103` is ambiguous.
pub fn episode_of(name: &str) -> Option<(u32, u32)> {
    static MARKED: OnceLock<Regex> = OnceLock::new();
    static CROSSED: OnceLock<Regex> = OnceLock::new();

    let name = name.to_lowercase();
    let found = pattern(&MARKED, r"(?i)\bs(\d{1,2})[\s._-]?e(\d{1,3})\b")
        .captures(&name)
        .or_else(|| pattern(&CROSSED, r"(?i)\b(\d{1,2})x(\d{2})\b").captures(&name))?;
    Some((found[1].parse().ok()?, found[2].parse().ok()?))
}

/// Whatever comes before the season marker.
pub fn show_of(title: &str) -> String {
    static MARKER: OnceLock<Regex> = OnceLock::new();
    let marker = pattern(
        &MARKER,
        r"(?i)[\s._-](s\d{1,2}\b|season[\s._-]*\d{1,2}\b|temporada[\s._-]*\d{1,2}\b|\d{1,2}x\d{2}\b)",
    );
    let cut = marker
        .find(title)
        .map(|found| &title[..found.start()])
        .unwrap_or(title);
    let mut words: Vec<String> = cut
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect();
    while words.len() > 1 {
        let last = words.last().map(String::as_str).unwrap_or("");
        if matches!(last, "complete" | "completa" | "full" | "collection") || is_year(last) {
            words.pop();
        } else {
            break;
        }
    }
    words.join(" ")
}

fn is_year(word: &str) -> bool {
    word.len() == 4
        && word.chars().all(|c| c.is_ascii_digit())
        && matches!(word.parse::<u16>(), Ok(1900..=2100))
}

const GIGABYTE: f64 = 1_073_741_824.0;

/// The smallest pack still looking like television.
pub fn pack_score(release: &SearchResult, preference: Preference, episodes: f64) -> f64 {
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
        -120.0
    } else if title.contains("1080p") {
        90.0
    } else if title.contains("720p") {
        80.0
    } else {
        0.0
    };

    let gigabytes_per_episode = (release.size_bytes as f64 / GIGABYTE) / episodes.max(1.0);
    score += episode_density_score(gigabytes_per_episode);

    score += crate::films::popularity_score(release.grabs);
    score -= crate::films::rot_risk(release);
    score -= crate::films::sideshow(&title);
    if let Preference::Language(code) = preference {
        if names_another_language(&release.tags, code) && !release.tags.contains(&Tag::Dub(code)) {
            score -= 150.0;
        }
    }
    score
}

/// Best around three quarters of a gigabyte an episode.
fn episode_density_score(gigabytes_per_episode: f64) -> f64 {
    match gigabytes_per_episode {
        rate if rate < 0.15 => -260.0,
        rate if rate < 0.3 => 40.0,
        rate if rate <= 1.2 => 220.0,
        rate if rate <= 2.5 => 110.0,
        rate if rate <= 5.0 => -40.0,
        _ => -200.0,
    }
}

/// One card per season, however releases named it.
pub fn group_seasons(
    results: Vec<SearchResult>,
    preference: Preference,
    named: Option<&str>,
) -> Vec<Season> {
    let mut grouped: Vec<(String, Season)> = Vec::new();

    for release in results {
        let Some(shape) = shape_of(&release.title) else {
            continue;
        };
        let (first, last) = match shape {
            Shape::Episode => continue,
            Shape::Season(0) => continue,
            Shape::Season(number) => (number, number),
            Shape::Seasons(from, to) => (from, to),
        };

        let show = match named {
            Some(name) => name.to_string(),
            None => show_of(&release.title),
        };
        let key = format!("{show}|{first}|{last}");
        match grouped.iter_mut().find(|(existing, _)| existing == &key) {
            Some((_, season)) => season.releases.push(release),
            None => grouped.push((
                key,
                Season {
                    show: match named {
                        Some(name) => name.to_string(),
                        None => titled(&show),
                    },
                    first,
                    last,
                    releases: vec![release],
                },
            )),
        }
    }

    let mut seasons: Vec<Season> = grouped.into_iter().map(|(_, season)| season).collect();
    for season in &mut seasons {
        let episodes = 10.0 * f64::from(season.last - season.first + 1);
        season.releases.sort_by(|left, right| {
            pack_score(right, preference, episodes)
                .partial_cmp(&pack_score(left, preference, episodes))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    seasons.sort_by_key(|season| (season.show.clone(), season.first));
    seasons
}

fn titled(show: &str) -> String {
    show.split(' ')
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::tags;

    fn release(title: &str, size_gb: f64, grabs: u64) -> SearchResult {
        SearchResult {
            tags: tags(title),
            title: title.to_string(),
            nzb_url: format!("https://indexer.test/{title}"),
            size_bytes: (size_gb * GIGABYTE) as u64,
            age_days: Some(200.0),
            grabs,
            cover_url: None,
            imdb: None,
            about: String::new(),
            thumbs_up: 0,
            thumbs_down: 0,
        }
    }

    #[test]
    fn an_episode_knows_which_episode_it_is() {
        assert_eq!(episode_of("Show.S01E03.1080p.mkv"), Some((1, 3)));
        assert_eq!(episode_of("Show.s01.e03.1080p.mkv"), Some((1, 3)));
        assert_eq!(episode_of("Show 1x03 1080p.mkv"), Some((1, 3)));
        assert_eq!(episode_of("Show.S02E11.spa.srt"), Some((2, 11)));
    }

    #[test]
    fn a_name_with_no_episode_in_it_is_not_guessed_at() {
        assert_eq!(episode_of("Das.Boot.1981.1080p.BluRay.mkv"), None);
        assert_eq!(episode_of("Show.S01.COMPLETE.1080p"), None);
    }

    #[test]
    fn tells_a_season_pack_from_an_episode() {
        assert_eq!(
            shape_of("Some.Show.S01E04.1080p.WEB-DL"),
            Some(Shape::Episode)
        );
        assert_eq!(shape_of("Some.Show.1x04.1080p"), Some(Shape::Episode));
        assert_eq!(
            shape_of("Some.Show.S02.1080p.WEB-DL"),
            Some(Shape::Season(2))
        );
        assert_eq!(shape_of("Some Show Season 3 1080p"), Some(Shape::Season(3)));
        assert_eq!(
            shape_of("Alguna.Serie.Temporada.1.720p"),
            Some(Shape::Season(1))
        );
        assert_eq!(
            shape_of("Some.Show.S01-S05.COMPLETE.1080p"),
            Some(Shape::Seasons(1, 5))
        );
        assert_eq!(shape_of("Just.A.Film.2016.1080p.BluRay"), None);
    }

    #[test]
    fn an_episode_written_ep_is_still_an_episode() {
        assert_eq!(
            shape_of("Game.of.Thrones.S01.EP01.2010.2160p.UHD.BluRay.x265"),
            Some(Shape::Episode)
        );
        assert_eq!(
            shape_of("Game.of.Thrones.S01.COMPLETE.2160p.UHD.BluRay.x265"),
            Some(Shape::Season(1))
        );
    }

    #[test]
    fn the_extras_that_come_as_season_zero_are_not_an_evening_of_television() {
        let seasons = group_seasons(
            vec![
                release("The.Show.S00.Bluray.1080p.x265", 6.0, 40),
                release("The.Show.S01.Bluray.1080p.x265", 20.0, 40),
            ],
            Preference::Any,
            None,
        );
        assert_eq!(seasons.len(), 1);
        assert_eq!((seasons[0].first, seasons[0].last), (1, 1));
    }

    #[test]
    fn a_show_asked_for_by_id_is_one_card_a_season_under_the_name_she_picked() {
        let seasons = group_seasons(
            vec![
                release("La.casa.de.papel.S01.1080p.NF.WEB-DL", 12.0, 300),
                release("Money.Heist.S01.1080p.NF.WEB-DL.DDP5.1.x264-DoA", 14.0, 900),
                release("Money.Heist.2017.S02.NF.WEBRip.1080p.x265", 10.0, 120),
            ],
            Preference::Any,
            Some("La casa de papel"),
        );
        assert_eq!(seasons.len(), 2, "one card a season, not one a spelling");
        assert_eq!(seasons[0].show, "La casa de papel");
        assert_eq!((seasons[0].first, seasons[0].last), (1, 1));
        assert_eq!(seasons[0].releases.len(), 2);
        assert_eq!(seasons[1].show, "La casa de papel");
    }

    #[test]
    fn without_an_id_the_names_are_all_there_is_to_tell_two_shows_apart() {
        let seasons = group_seasons(
            vec![
                release("Gomorrah.S01.1080p.HMAX.WEB-DL", 12.0, 300),
                release("Gomorrah.The.Origins.S01.1080p.SKY.WEB-DL", 10.0, 100),
            ],
            Preference::Any,
            None,
        );
        assert_eq!(seasons.len(), 2);
    }

    #[test]
    fn offers_seasons_and_never_single_episodes() {
        let seasons = group_seasons(
            vec![
                release("Some.Show.S01E01.1080p.WEB-DL", 1.4, 900),
                release("Some.Show.S01E02.1080p.WEB-DL", 1.4, 800),
                release("Some.Show.S01.1080p.WEB-DL-PACK", 7.5, 300),
            ],
            Preference::Any,
            None,
        );
        assert_eq!(seasons.len(), 1);
        assert_eq!(seasons[0].releases.len(), 1);
        assert!(seasons[0].best().expect("a pack").title.contains("PACK"));
    }

    #[test]
    fn reads_the_show_out_of_the_release_name() {
        let seasons = group_seasons(
            vec![release("The.Sopranos.S02.1080p.BluRay-X", 20.0, 50)],
            Preference::Any,
            None,
        );
        assert_eq!(seasons[0].show, "The Sopranos");
        assert_eq!((seasons[0].first, seasons[0].last), (2, 2));
    }

    #[test]
    fn packaging_words_do_not_invent_a_second_show() {
        let seasons = group_seasons(
            vec![
                release("Game.of.Thrones.S01.1080p.WEB-DL", 9.0, 6000),
                release("Game.of.Thrones.Complete.S01.2160p", 35.0, 700),
            ],
            Preference::Any,
            None,
        );
        assert_eq!(seasons.len(), 1, "one show, one Temporada 1");
        assert_eq!(seasons[0].show, "Game Of Thrones");
        assert_eq!(seasons[0].releases.len(), 2);
    }

    #[test]
    fn a_release_year_does_not_invent_a_second_show() {
        let seasons = group_seasons(
            vec![
                release("Money.Heist.S01.1080p.WEB-DL", 20.0, 56),
                release("Money.Heist.2017.S01.720p.WEB-DL", 12.0, 61),
            ],
            Preference::Any,
            None,
        );
        assert_eq!(seasons.len(), 1);
        assert_eq!(seasons[0].show, "Money Heist");
        assert_eq!(seasons[0].releases.len(), 2);
    }

    #[test]
    fn a_show_actually_named_after_a_year_keeps_its_name() {
        assert_eq!(show_of("1923.S01.1080p.WEB-DL"), "1923");
    }

    #[test]
    fn keeps_seasons_of_one_show_apart() {
        let seasons = group_seasons(
            vec![
                release("The.Show.S01.1080p-A", 7.0, 10),
                release("The.Show.S02.1080p-B", 7.0, 10),
                release("The.Show.S01.720p-C", 4.0, 10),
            ],
            Preference::Any,
            None,
        );
        assert_eq!(seasons.len(), 2);
        assert_eq!(seasons[0].releases.len(), 2);
        assert_eq!(seasons[0].first, 1);
        assert_eq!(seasons[1].first, 2);
    }

    #[test]
    fn takes_the_smaller_pack_when_both_are_proper_copies() {
        let seasons = group_seasons(
            vec![
                release("The.Show.S01.2160p.BluRay-HUGE", 90.0, 400),
                release("The.Show.S01.1080p.WEB-DL-SANE", 8.0, 60),
            ],
            Preference::Any,
            None,
        );
        assert!(seasons[0].best().expect("a pack").title.contains("SANE"));
    }

    #[test]
    fn refuses_a_pack_too_small_to_be_a_season() {
        let seasons = group_seasons(
            vec![
                release("The.Show.S01.SAMPLE-TINY", 0.4, 5000),
                release("The.Show.S01.720p.WEB-FINE", 5.0, 20),
            ],
            Preference::Any,
            None,
        );
        assert!(seasons[0].best().expect("a pack").title.contains("FINE"));
    }

    #[test]
    fn a_pack_of_several_seasons_is_allowed_to_be_larger() {
        let five = release("The.Show.S01-S05.1080p-BOX", 40.0, 100);
        let one = release("The.Show.S01.1080p-ONE", 40.0, 100);
        assert!(
            pack_score(&five, Preference::Any, 50.0) > pack_score(&one, Preference::Any, 10.0),
            "the same size is a better copy when it holds five times as much television"
        );
    }
}
