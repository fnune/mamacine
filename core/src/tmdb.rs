//! The Movie Database, used for suggestions when a key is configured.
//!
//! It is the one source that serves titles in her language and states `original_title` outright,
//! so it is the only provider allowed to put an original in parentheses: a guessed original is a
//! wrong name waiting to happen. Without a key the app falls back to the keyless IMDb lookup.

use crate::error::{Error, Result};
use crate::http::{expect_success, HttpClient, Request};
use crate::indexer::{encode_component, ShowIds};
use crate::lookup::{shown_title, Picked, Suggestion};
use crate::search::fold;
use crate::series::Episode;
use serde_json::Value;

const SUGGESTION_LIMIT: usize = 8;

pub struct Tmdb<H> {
    key: String,
    /// The language the interface speaks, e.g. "es-ES". Titles come back localized to it.
    language: String,
    http: H,
    api_base: String,
}

impl<H: HttpClient> Tmdb<H> {
    pub fn new(key: String, language: String, http: H) -> Self {
        Tmdb {
            key,
            language,
            http,
            api_base: "https://api.themoviedb.org/3".into(),
        }
    }

    /// Only tests point it elsewhere.
    pub fn at(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    fn json(&self, path_and_query: &str) -> Result<Value> {
        let url = if is_read_token(&self.key) {
            format!("{}{path_and_query}", self.api_base)
        } else {
            let join = if path_and_query.contains('?') {
                '&'
            } else {
                '?'
            };
            format!(
                "{}{path_and_query}{join}api_key={}",
                self.api_base,
                encode_component(&self.key)
            )
        };
        let mut request = Request::get(url).header("User-Agent", "MamaCine/1.0");
        if is_read_token(&self.key) {
            request = request.header("Authorization", format!("Bearer {}", self.key.trim()));
        }
        let response = expect_success("the film database", self.http.send(request)?)?;
        serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
            what: "the film database".into(),
            detail: failure.to_string(),
        })
    }

    pub fn suggest(&self, text: &str) -> Result<Vec<Suggestion>> {
        let query = text.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let answer = self.json(&format!(
            "/search/multi?query={}&language={}&include_adult=false",
            encode_component(query),
            encode_component(&self.language)
        ))?;
        Ok(parse_suggestions(&answer))
    }

    /// A picked suggestion becomes the query the indexer can answer. Releases are filed under a
    /// film's IMDb id and a show's international name, neither of which the search result carries,
    /// so picking costs one more lookup.
    pub fn resolve(&self, suggestion: &Suggestion) -> Result<Picked> {
        let title = shown_title(&suggestion.title, suggestion.original.as_deref());
        if suggestion.series {
            // the indexer files a show under its tvdb id and its packs under an international
            // name; both come back in one answer, and neither is in the suggestion
            let answer = self.json(&format!(
                "/tv/{}?language=en-US&append_to_response=external_ids",
                suggestion.id
            ))?;
            let international = answer
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&suggestion.title);
            return Ok(Picked {
                query: international.to_string(),
                series: true,
                title,
                show: parse_show_ids(&answer),
            });
        }
        let answer = self.json(&format!("/movie/{}/external_ids", suggestion.id))?;
        let query = match answer
            .get("imdb_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            Some(imdb) => imdb.to_string(),
            // no id registered: the international name is the next most searchable thing
            None => self
                .json(&format!("/movie/{}?language=en-US", suggestion.id))?
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&suggestion.title)
                .to_string(),
        };
        Ok(Picked {
            query,
            series: false,
            title,
            show: ShowIds::default(),
        })
    }

    /// The episodes of a season, named in her language. A run of several seasons is answered with
    /// how many episodes each of them has, from the show's own entry, rather than with one request
    /// per season: the number is what a five-season pack has to say, and fifty names are not a
    /// screen she can read.
    pub fn episodes(&self, tmdb: &str, first: u32, last: u32) -> Result<Vec<Episode>> {
        if first != last {
            let answer = self.json(&format!("/tv/{}", encode_component(tmdb)))?;
            return Ok(parse_season_lengths(&answer, first, last));
        }
        let answer = self.json(&format!(
            "/tv/{}/season/{first}?language={}",
            encode_component(tmdb),
            encode_component(&self.language)
        ))?;
        Ok(parse_episodes(&answer))
    }

    /// The show a name means, for a season she already owns whose search is long gone. Two
    /// questions, because a search result states no ids: which show, then which ids it has.
    pub fn show_named(&self, name: &str) -> Result<ShowIds> {
        let answer = self.json(&format!(
            "/search/tv?query={}&language={}",
            encode_component(name),
            encode_component(&self.language)
        ))?;
        let Some(id) = best_show(&answer, name) else {
            return Ok(ShowIds::default());
        };
        let full = self.json(&format!(
            "/tv/{id}?language=en-US&append_to_response=external_ids"
        ))?;
        Ok(parse_show_ids(&full))
    }

    /// What the film is about, in her language, found under the film's IMDb id. `None` when the
    /// database has nothing to say, which is an answer and not an error.
    pub fn synopsis(&self, imdb_id: &str) -> Result<Option<String>> {
        let answer = self.json(&format!(
            "/find/{}?external_source=imdb_id&language={}",
            encode_component(imdb_id),
            encode_component(&self.language)
        ))?;
        Ok(parse_synopsis(&answer))
    }

    /// Validates the key from the settings screen, on the endpoint that costs nothing.
    pub fn check(&self) -> Result<()> {
        self.json("/configuration").map(|_| ())
    }

    pub fn poster(&self, url: &str) -> Result<(String, Vec<u8>)> {
        // fetched here rather than by the page, so only TMDB's own image host is ever contacted
        if !url.starts_with("https://image.tmdb.org/") {
            return Err(Error::Setup(
                "posters must come from TMDB's image host".into(),
            ));
        }
        let request = Request::get(url.to_string()).header("User-Agent", "MamaCine/1.0");
        let response = expect_success("the poster host", self.http.send(request)?)?;
        Ok((response.content_type.clone(), response.body))
    }
}

/// TMDB offers two credentials on the same page: the v3 key, which travels in the query, and the
/// v4 read access token, a JWT that is only accepted as a bearer header and answers 401 in the
/// query. She pastes whichever one she found, so what she pasted decides how it is sent.
pub fn is_read_token(key: &str) -> bool {
    let key = key.trim();
    key.starts_with("eyJ") && key.contains('.')
}

pub fn parse_synopsis(answer: &Value) -> Option<String> {
    ["movie_results", "tv_results"].iter().find_map(|kind| {
        answer
            .get(kind)
            .and_then(Value::as_array)
            .and_then(|results| results.first())
            .and_then(|found| found.get("overview"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|overview| !overview.is_empty())
            .map(str::to_string)
    })
}

pub fn parse_episodes(answer: &Value) -> Vec<Episode> {
    let Some(items) = answer.get("episodes").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let number = |field: &str| item.get(field).and_then(Value::as_i64);
            Some(Episode {
                season: u32::try_from(number("season_number")?).ok()?,
                number: u32::try_from(number("episode_number")?).ok()?,
                title: item
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string),
                overview: item
                    .get("overview")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|overview| !overview.is_empty())
                    .map(str::to_string),
            })
        })
        .collect()
}

/// How many episodes each season has, from the show's own entry: enough to say how much television
/// a pack of several seasons is, and it states no name it was not told.
pub fn parse_season_lengths(answer: &Value, first: u32, last: u32) -> Vec<Episode> {
    let Some(seasons) = answer.get("seasons").and_then(Value::as_array) else {
        return Vec::new();
    };
    seasons
        .iter()
        .filter_map(|season| {
            let number = |field: &str| season.get(field).and_then(Value::as_i64);
            let index = u32::try_from(number("season_number")?).ok()?;
            let count = u32::try_from(number("episode_count")?).ok()?;
            (first..=last).contains(&index).then_some((index, count))
        })
        .flat_map(|(season, count)| {
            (1..=count).map(move |number| Episode {
                season,
                number,
                ..Episode::default()
            })
        })
        .collect()
}

/// The ids a show is filed under at the indexer, out of `external_ids`.
/// Which of the shows the database offers is the one on her card.
///
/// Its own order is not the answer: searching "gomorrah" puts a collection nobody has heard of
/// (popularity 0.5) above Gomorra itself (popularity 25), because the collection's name matches
/// the letters more exactly. So the name is a filter and not a ranking: a show whose name or
/// original name does not contain every word she has is a different show and is dropped, and of
/// those that survive, the most watched one is what a name on its own means.
pub fn best_show(answer: &Value, name: &str) -> Option<String> {
    let results = answer.get("results").and_then(Value::as_array)?;
    results
        .iter()
        .filter_map(|item| {
            let named = |field: &str| item.get(field).and_then(Value::as_str).unwrap_or_default();
            let looks_like = crate::search::relevance(name, named("name"))
                .max(crate::search::relevance(name, named("original_name")));
            let id = item
                .get("id")
                .and_then(Value::as_i64)
                .filter(|id| *id > 0)?;
            let watched = item
                .get("popularity")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            (looks_like >= 1.0).then_some((watched, id.to_string()))
        })
        .max_by(|one, other| one.0.total_cmp(&other.0))
        .map(|(_, id)| id)
}

pub fn parse_show_ids(answer: &Value) -> ShowIds {
    let ids = answer.get("external_ids").unwrap_or(answer);
    ShowIds {
        tmdb: answer
            .get("id")
            .and_then(Value::as_i64)
            .filter(|id| *id > 0)
            .map(|id| id.to_string()),
        tvdb: ids
            .get("tvdb_id")
            .and_then(Value::as_i64)
            .filter(|id| *id > 0)
            .map(|id| id.to_string()),
        tvmaze: None,
        imdb: ids
            .get("imdb_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
    }
}

pub fn parse_suggestions(answer: &Value) -> Vec<Suggestion> {
    let Some(items) = answer.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let series = match item.get("media_type").and_then(Value::as_str)? {
                "movie" => false,
                "tv" => true,
                _ => return None, // people are not downloadable
            };
            let text = |field: &str| {
                item.get(field)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            };
            let title = if series { text("name") } else { text("title") }?;
            let original = if series {
                text("original_name")
            } else {
                text("original_title")
            }
            // the parentheses appear only when the original is genuinely another name
            .filter(|original| fold(original).to_lowercase() != fold(&title).to_lowercase());
            // an unreleased title has nothing on usenet to satisfy it
            let year = if series {
                text("first_air_date")
            } else {
                text("release_date")
            }
            .and_then(|date| date.split('-').next().map(str::to_string))?;
            Some(Suggestion {
                id: item.get("id").and_then(Value::as_i64)?.to_string(),
                title,
                original,
                year: Some(year),
                series,
                poster_url: text("poster_path")
                    .map(|path| format!("https://image.tmdb.org/t/p/w185{path}")),
            })
        })
        .take(SUGGESTION_LIMIT)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::http::Response;

    fn service(answers: Vec<Response>) -> Tmdb<FakeHttp> {
        Tmdb::new(
            "la-clave".into(),
            "es-ES".into(),
            FakeHttp::answering(answers),
        )
        .at("https://tmdb.test/3".into())
    }

    fn answer() -> Value {
        serde_json::json!({"results": [
            {"media_type": "movie", "id": 432787, "title": "El hoyo",
             "original_title": "El hoyo", "release_date": "2019-11-08",
             "poster_path": "/hoyo.jpg"},
            {"media_type": "tv", "id": 71446, "name": "La casa de papel",
             "original_name": "La casa de papel", "first_air_date": "2017-05-02",
             "poster_path": "/lcdp.jpg"},
            {"media_type": "movie", "id": 496243, "title": "Parásitos",
             "original_title": "기생충", "release_date": "2019-05-30"},
            {"media_type": "person", "id": 1, "name": "Pedro Almodóvar"},
            {"media_type": "movie", "id": 9, "title": "Sin estrenar",
             "original_title": "Unreleased", "release_date": ""}
        ]})
    }

    #[test]
    fn titles_arrive_in_her_language_with_the_original_only_when_it_differs() {
        let found = parse_suggestions(&answer());
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].title, "El hoyo");
        assert_eq!(found[0].original, None, "El hoyo is its own original");
        assert_eq!(found[2].title, "Parásitos");
        assert_eq!(found[2].original.as_deref(), Some("기생충"));
        assert!(found[1].series);
        assert_eq!(
            found[0].poster_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w185/hoyo.jpg")
        );
    }

    #[test]
    fn people_and_unreleased_titles_are_not_offered() {
        let found = parse_suggestions(&answer());
        assert!(found
            .iter()
            .all(|suggestion| suggestion.title != "Sin estrenar"));
        assert!(found
            .iter()
            .all(|suggestion| !suggestion.title.contains("Almodóvar")));
    }

    #[test]
    fn the_search_asks_in_her_language_and_carries_the_key() {
        let service = service(vec![FakeHttp::status(200, &answer().to_string())]);
        service.suggest("el hoyo").expect("suggestions");
        let url = service.http.last_url();
        assert!(
            url.starts_with("https://tmdb.test/3/search/multi?"),
            "{url}"
        );
        assert!(url.contains("query=el+hoyo"), "{url}");
        assert!(url.contains("language=es-ES"), "{url}");
        assert!(url.contains("api_key=la-clave"), "{url}");
        assert!(url.contains("include_adult=false"), "{url}");
    }

    // The page she copies from offers two credentials, and the long one is refused outright as a
    // query parameter: sending it the wrong way is a key that looks configured and works nowhere.
    #[test]
    fn a_read_access_token_travels_as_a_bearer_header_and_never_in_the_query() {
        let token = "eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJtYW1hY2luZSJ9.firma";
        let service = Tmdb::new(
            token.into(),
            "es-ES".into(),
            FakeHttp::answering(vec![FakeHttp::status(200, &answer().to_string())]),
        )
        .at("https://tmdb.test/3".into());
        service.suggest("el hoyo").expect("suggestions");
        let asked = service.http.requests().pop().expect("a request");
        assert!(!asked.url.contains("api_key"), "{}", asked.url);
        assert!(asked.url.contains("language=es-ES"), "{}", asked.url);
        assert_eq!(
            asked.headers.get("Authorization").map(String::as_str),
            Some(format!("Bearer {token}").as_str())
        );
    }

    #[test]
    fn the_short_key_still_travels_in_the_query_and_carries_no_header() {
        let service = service(vec![FakeHttp::status(200, &answer().to_string())]);
        service.suggest("el hoyo").expect("suggestions");
        let asked = service.http.requests().pop().expect("a request");
        assert!(asked.url.contains("api_key=la-clave"), "{}", asked.url);
        assert_eq!(asked.headers.get("Authorization"), None);
    }

    #[test]
    fn only_a_jwt_is_read_as_a_read_access_token() {
        assert!(is_read_token(" eyJhbGciOiJIUzI1NiJ9.eyJhIjoxfQ.firma "));
        assert!(!is_read_token("la-clave"));
        assert!(
            !is_read_token("eyJsinpuntos"),
            "a key is not a token for looking like one"
        );
        assert!(!is_read_token(""));
    }

    // Real answer, trimmed: TMDB's own order puts a collection with a popularity of 0.5 above the
    // show with a popularity of 25, because its name matches the letters more exactly. Naming her
    // episodes from that one would put another programme's names on her season.
    #[test]
    fn the_show_a_name_means_is_the_watched_one_and_not_the_closest_spelling() {
        let answer = serde_json::json!({"results": [
            {"id": 314677, "name": "Gomorrah Collection",
             "original_name": "Gomorrah Collection", "popularity": 0.4764},
            {"id": 61068, "name": "Gomorra", "original_name": "Gomorra - La serie",
             "popularity": 25.4641},
            {"id": 293550, "name": "Gomorra: El origen", "original_name": "Gomorra - Le Origini",
             "popularity": 8.4936}
        ]});
        assert_eq!(best_show(&answer, "gomorrah").as_deref(), Some("61068"));
    }

    // The card says what the release said; the database answers in her language. The original
    // name is what the two have in common, and a show that matches neither is not hers.
    #[test]
    fn a_show_named_in_her_language_is_found_by_the_original_name_beside_it() {
        let answer = serde_json::json!({"results": [
            {"id": 1399, "name": "Juego de tronos", "original_name": "Game of Thrones",
             "popularity": 171.258},
            {"id": 326761, "name": "The Official Game of Thrones Podcast: House of the Dragon",
             "original_name": "The Official Game of Thrones Podcast: House of the Dragon",
             "popularity": 2.98}
        ]});
        assert_eq!(
            best_show(&answer, "game of thrones").as_deref(),
            Some("1399")
        );
        assert_eq!(
            best_show(&answer, "los serrano"),
            None,
            "a name none of them carries is not answered with the most popular one"
        );
    }

    #[test]
    fn identifying_a_show_asks_what_it_is_and_then_what_ids_it_has() {
        let service = service(vec![
            FakeHttp::status(
                200,
                r#"{"results": [{"id": 61068, "name": "Gomorra",
                    "original_name": "Gomorra - La serie", "popularity": 25.5}]}"#,
            ),
            FakeHttp::status(
                200,
                r#"{"id": 61068, "external_ids": {"imdb_id": "tt2049116", "tvdb_id": 281342}}"#,
            ),
        ]);
        let show = service.show_named("gomorrah").expect("the show");
        assert_eq!(show.tmdb.as_deref(), Some("61068"));
        assert_eq!(show.tvdb.as_deref(), Some("281342"));
        assert_eq!(show.imdb.as_deref(), Some("tt2049116"));
        let asked = service.http.requests();
        assert!(
            asked[0].url.contains("/search/tv?query=gomorrah"),
            "{}",
            asked[0].url
        );
    }

    fn film(id: &str, title: &str, original: Option<&str>) -> Suggestion {
        Suggestion {
            id: id.into(),
            title: title.into(),
            original: original.map(str::to_string),
            year: Some("2019".into()),
            series: false,
            poster_url: None,
        }
    }

    // The indexer files releases under the IMDb id; TMDB's search result does not carry it, so
    // picking costs one more lookup, and the picked name keeps the original in parentheses.
    #[test]
    fn a_picked_film_is_searched_by_its_imdb_id_and_named_with_both_titles() {
        let service = service(vec![FakeHttp::status(
            200,
            r#"{"id": 432787, "imdb_id": "tt8228288"}"#,
        )]);
        let picked = service
            .resolve(&film("432787", "El hoyo", Some("The Platform")))
            .expect("resolved");
        assert_eq!(picked.query, "tt8228288");
        assert_eq!(picked.title, "El hoyo (The Platform)");
        assert!(service
            .http
            .last_url()
            .contains("/movie/432787/external_ids"));
    }

    #[test]
    fn a_film_without_an_imdb_id_falls_back_to_its_international_name() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"id": 432787, "imdb_id": null}"#),
            FakeHttp::status(200, r#"{"id": 432787, "title": "The Platform"}"#),
        ]);
        let picked = service
            .resolve(&film("432787", "El hoyo", Some("The Platform")))
            .expect("resolved");
        assert_eq!(picked.query, "The Platform");
    }

    // The indexer files the show under its tvdb id, and the packs under an international name for
    // the indexers that have no id for it: "La casa de papel" alone finds neither.
    #[test]
    fn a_picked_series_carries_the_ids_and_the_name_an_indexer_answers_to() {
        let service = service(vec![FakeHttp::status(
            200,
            r#"{"id": 71446, "name": "Money Heist",
                "external_ids": {"imdb_id": "tt6468322", "tvdb_id": 327417}}"#,
        )]);
        let picked = service
            .resolve(&Suggestion {
                series: true,
                ..film("71446", "La casa de papel", None)
            })
            .expect("resolved");
        assert_eq!(picked.query, "Money Heist");
        assert_eq!(picked.title, "La casa de papel");
        assert_eq!(picked.show.tvdb.as_deref(), Some("327417"));
        assert_eq!(picked.show.imdb.as_deref(), Some("tt6468322"));
        let url = service.http.last_url();
        assert!(url.contains("/tv/71446?language=en-US"), "{url}");
        assert!(
            url.contains("append_to_response=external_ids"),
            "the ids and the name are one question, not two: {url}"
        );
    }

    #[test]
    fn a_show_the_database_has_no_ids_for_is_left_with_its_name() {
        let ids = parse_show_ids(&serde_json::json!({"external_ids": {"tvdb_id": null}}));
        assert_eq!(ids.tvdb, None);
        assert!(!ids.any());
    }

    #[test]
    fn the_episodes_of_one_season_are_asked_for_in_her_language() {
        let service = service(vec![FakeHttp::status(
            200,
            r#"{"episodes": [
                {"season_number": 1, "episode_number": 1, "name": "El clan Savastano",
                 "overview": "Ciro y Genny."},
                {"season_number": 1, "episode_number": 2, "name": "¿Te fías de mí?",
                 "overview": "  "}]}"#,
        )]);
        let episodes = service.episodes("71446", 1, 1).expect("episodes");
        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[1].title.as_deref(), Some("¿Te fías de mí?"));
        assert_eq!(
            episodes[0].overview.as_deref(),
            Some("Ciro y Genny."),
            "what the episode is about comes with its name"
        );
        assert_eq!(episodes[1].overview, None, "and blank words are no words");
        let url = service.http.last_url();
        assert!(url.contains("/tv/71446/season/1?"), "{url}");
        assert!(url.contains("language=es-ES"), "{url}");
    }

    // Fifty names are not a screen she can read, and five requests are not a screen worth paying
    // for: a pack of several seasons is answered with how much television it is.
    #[test]
    fn a_pack_of_several_seasons_counts_them_in_one_question() {
        let service = service(vec![FakeHttp::status(
            200,
            r#"{"id": 71446, "seasons": [
                {"season_number": 0, "episode_count": 3},
                {"season_number": 1, "episode_count": 13},
                {"season_number": 2, "episode_count": 9},
                {"season_number": 3, "episode_count": 8}]}"#,
        )]);
        let episodes = service.episodes("71446", 1, 2).expect("episodes");
        assert_eq!(episodes.len(), 22, "the specials are not part of the pack");
        assert!(episodes.iter().all(|episode| episode.title.is_none()));
        assert_eq!(service.http.requests().len(), 1);
    }

    #[test]
    fn a_synopsis_is_asked_for_by_imdb_id_in_her_language() {
        let service = service(vec![FakeHttp::status(
            200,
            r#"{"movie_results": [{"id": 432787, "overview": "Una prisión vertical."}],
                "tv_results": []}"#,
        )]);
        let words = service.synopsis("tt8228288").expect("a synopsis");
        assert_eq!(words.as_deref(), Some("Una prisión vertical."));
        let url = service.http.last_url();
        assert!(url.contains("/find/tt8228288?"), "{url}");
        assert!(url.contains("external_source=imdb_id"), "{url}");
        assert!(url.contains("language=es-ES"), "{url}");
    }

    #[test]
    fn a_film_the_database_cannot_describe_is_none_rather_than_empty_words() {
        let blank = serde_json::json!({"movie_results": [{"overview": "  "}], "tv_results": []});
        assert_eq!(parse_synopsis(&blank), None);
        let missing = serde_json::json!({"movie_results": [], "tv_results": []});
        assert_eq!(parse_synopsis(&missing), None);
        let series = serde_json::json!({"movie_results": [],
            "tv_results": [{"overview": "Una casa, un atraco."}]});
        assert_eq!(
            parse_synopsis(&series).as_deref(),
            Some("Una casa, un atraco.")
        );
    }

    #[test]
    fn checking_the_key_uses_the_endpoint_that_costs_nothing() {
        let service = service(vec![FakeHttp::status(200, r#"{"images": {}}"#)]);
        service.check().expect("a working key");
        assert!(service.http.last_url().contains("/configuration"));
    }

    #[test]
    fn posters_may_only_come_from_tmdbs_image_host() {
        let service = service(vec![]);
        let refused = service.poster("https://somewhere-else.test/evil.jpg");
        assert!(matches!(refused, Err(Error::Setup(_))));
        assert!(service.http.requests().is_empty(), "it must not even ask");
    }
}
