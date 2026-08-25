//! What makes two downloads the same film.

use crate::films::Film;
use crate::series::Season;

fn plainly(text: &str) -> String {
    text.to_lowercase()
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
        .join(" ")
}

/// The indexer's id, or the normalised name.
pub fn film_key(film: &Film) -> String {
    match film
        .imdb
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(imdb) => format!("imdb:{}", imdb.trim_start_matches('0')),
        None => format!(
            "film:{}:{}",
            plainly(&film.title),
            film.year.as_deref().unwrap_or("")
        ),
    }
}

/// The show plus its season numbers.
pub fn season_key(season: &Season) -> String {
    format!("season:{}:{}", plainly(&season.show), season.first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn film(title: &str, year: Option<&str>, imdb: Option<&str>) -> Film {
        Film {
            title: title.into(),
            year: year.map(str::to_string),
            imdb: imdb.map(str::to_string),
            cover_url: None,
            about: String::new(),
            releases: Vec::new(),
        }
    }

    #[test]
    fn the_same_film_under_two_names_is_one_film() {
        assert_eq!(
            film_key(&film("Das Boot", Some("1981"), Some("0082096"))),
            film_key(&film(
                "Das Boot: The Director's Cut",
                Some("1997"),
                Some("82096")
            )),
        );
    }

    #[test]
    fn without_an_indexer_id_the_title_and_year_stand_in() {
        assert_eq!(
            film_key(&film("The Red Turtle", Some("2016"), None)),
            film_key(&film("the red turtle!", Some("2016"), None)),
        );
        assert_ne!(
            film_key(&film("The Red Turtle", Some("2016"), None)),
            film_key(&film("The Red Turtle", Some("2020"), None)),
        );
    }

    #[test]
    fn a_season_is_the_show_and_its_number() {
        let season = |show: &str, first: u32| Season {
            show: show.into(),
            first,
            last: first,
            releases: Vec::new(),
        };
        assert_eq!(
            season_key(&season("Game of Thrones", 1)),
            season_key(&season("game.of.thrones", 1)),
        );
        assert_ne!(
            season_key(&season("Game of Thrones", 1)),
            season_key(&season("Game of Thrones", 2)),
        );
    }

    #[test]
    fn a_film_is_never_confused_with_a_season() {
        assert_ne!(
            film_key(&film("Fargo", Some("1996"), None)),
            season_key(&Season {
                show: "Fargo".into(),
                first: 1,
                last: 1,
                releases: Vec::new(),
            }),
        );
    }
}
