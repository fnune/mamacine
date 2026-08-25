//! TVMaze: keyless IMDb-to-tvdb id bridge.

use crate::error::{Error, Result};
use crate::http::{expect_success, HttpClient, Request};
use crate::indexer::{encode_component, ShowIds};
use crate::series::Episode;
use serde_json::Value;

pub struct TvMaze<H> {
    http: H,
    api_base: String,
}

impl<H: HttpClient> TvMaze<H> {
    pub fn new(http: H) -> Self {
        TvMaze {
            http,
            api_base: "https://api.tvmaze.com".into(),
        }
    }

    /// Only tests point it elsewhere.
    pub fn at(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    /// A 404 is an answer, not a failure.
    pub fn ids_for(&self, imdb: &str) -> Result<ShowIds> {
        let known = ShowIds {
            imdb: Some(imdb.to_string()),
            ..ShowIds::default()
        };
        let url = format!(
            "{}/lookup/shows?imdb={}",
            self.api_base,
            encode_component(imdb)
        );
        let request = Request::get(url).header("User-Agent", "MamaCine/1.0");
        let response = self.http.send(request)?;
        if response.status == 404 {
            return Ok(known);
        }
        let response = expect_success("the show database", response)?;
        let answer: Value =
            serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
                what: "the show database".into(),
                detail: failure.to_string(),
            })?;
        Ok(parse_ids(&answer, imdb))
    }

    /// The one show a name means, with ids.
    pub fn show_named(&self, name: &str) -> Result<ShowIds> {
        let url = format!(
            "{}/singlesearch/shows?q={}",
            self.api_base,
            encode_component(name)
        );
        let request = Request::get(url).header("User-Agent", "MamaCine/1.0");
        let response = self.http.send(request)?;
        if response.status == 404 {
            return Ok(ShowIds::default());
        }
        let response = expect_success("the show database", response)?;
        let answer: Value =
            serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
                what: "the show database".into(),
                detail: failure.to_string(),
            })?;
        let imdb = answer
            .get("externals")
            .and_then(|externals| externals.get("imdb"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        Ok(parse_ids(&answer, imdb))
    }

    /// One question answers the whole show.
    pub fn episodes(&self, tvmaze: &str, first: u32, last: u32) -> Result<Vec<Episode>> {
        let url = format!(
            "{}/shows/{}/episodes",
            self.api_base,
            encode_component(tvmaze)
        );
        let request = Request::get(url).header("User-Agent", "MamaCine/1.0");
        let response = expect_success("the show database", self.http.send(request)?)?;
        let answer: Value =
            serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
                what: "the show database".into(),
                detail: failure.to_string(),
            })?;
        Ok(parse_episodes(&answer)
            .into_iter()
            .filter(|episode| (first..=last).contains(&episode.season))
            .collect())
    }
}

pub fn parse_episodes(answer: &Value) -> Vec<Episode> {
    let Some(items) = answer.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let number = |field: &str| item.get(field).and_then(Value::as_i64);
            Some(Episode {
                season: u32::try_from(number("season")?).ok()?,
                number: u32::try_from(number("number")?).ok()?,
                title: item
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string),
                overview: item
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(in_plain_words)
                    .filter(|summary| !summary.is_empty()),
            })
        })
        .collect()
}

fn in_plain_words(html: &str) -> String {
    let mut words = String::with_capacity(html.len());
    let mut inside_tag = false;
    for letter in html.chars() {
        match letter {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => words.push(letter),
            _ => {}
        }
    }
    words
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

pub fn parse_ids(answer: &Value, imdb: &str) -> ShowIds {
    let number = |value: Option<&Value>| {
        value
            .and_then(Value::as_i64)
            .filter(|id| *id > 0)
            .map(|id| id.to_string())
    };
    ShowIds {
        tmdb: None,
        tvdb: number(
            answer
                .get("externals")
                .and_then(|externals| externals.get("thetvdb")),
        ),
        tvmaze: number(answer.get("id")),
        imdb: (!imdb.is_empty()).then(|| imdb.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::http::Response;

    fn service(answers: Vec<Response>) -> TvMaze<FakeHttp> {
        TvMaze::new(FakeHttp::answering(answers)).at("https://tvmaze.test".into())
    }

    fn found() -> String {
        serde_json::json!({
            "id": 82,
            "name": "Game of Thrones",
            "externals": {"tvrage": 24493, "thetvdb": 121361, "imdb": "tt0944947"}
        })
        .to_string()
    }

    #[test]
    fn a_name_off_her_own_card_becomes_the_show_it_is() {
        let service = service(vec![FakeHttp::status(
            200,
            &serde_json::json!({
                "id": 2228,
                "name": "Gomorra: La Serie",
                "externals": {"thetvdb": 281342, "imdb": "tt2049116"}
            })
            .to_string(),
        )]);
        let show = service.show_named("Gomorrah").expect("the show");
        assert_eq!(show.tvmaze.as_deref(), Some("2228"));
        assert_eq!(show.tvdb.as_deref(), Some("281342"));
        assert_eq!(show.imdb.as_deref(), Some("tt2049116"));
        assert!(service
            .http
            .last_url()
            .contains("/singlesearch/shows?q=Gomorrah"));
    }

    #[test]
    fn a_name_it_does_not_recognise_is_an_answer_and_not_a_failure() {
        let service = service(vec![FakeHttp::status(404, "")]);
        let show = service
            .show_named("una serie que no existe")
            .expect("an answer");
        assert!(!show.any(), "and nothing is invented to fill it");
    }

    #[test]
    fn a_show_arrives_with_the_ids_an_indexer_takes() {
        let service = service(vec![FakeHttp::status(200, &found())]);
        let ids = service.ids_for("tt0944947").expect("ids");
        assert_eq!(ids.tvdb.as_deref(), Some("121361"));
        assert_eq!(ids.tvmaze.as_deref(), Some("82"));
        assert_eq!(ids.imdb.as_deref(), Some("tt0944947"));
        assert!(service.http.last_url().contains("imdb=tt0944947"));
    }

    #[test]
    fn a_show_it_has_never_heard_of_is_not_a_failure() {
        let service = service(vec![FakeHttp::status(404, "")]);
        let ids = service.ids_for("tt9999999").expect("no ids, no error");
        assert_eq!(ids.tvdb, None);
        assert_eq!(ids.tvmaze, None);
        assert_eq!(ids.imdb.as_deref(), Some("tt9999999"));
    }

    #[test]
    fn the_episodes_of_the_seasons_asked_about_come_back_named() {
        let all = serde_json::json!([
            {"season": 1, "number": 1, "name": "Il clan dei Savastano",
             "summary": "<p>Ciro &amp; Genny.</p>"},
            {"season": 1, "number": 2, "name": "Ti fidi di me?"},
            {"season": 2, "number": 1, "name": "Un giorno di scuola"}
        ])
        .to_string();
        let service = service(vec![FakeHttp::status(200, &all)]);
        let episodes = service.episodes("2228", 1, 1).expect("episodes");
        assert_eq!(episodes.len(), 2, "only the season she is looking at");
        assert_eq!(episodes[0].number, 1);
        assert_eq!(episodes[0].title.as_deref(), Some("Il clan dei Savastano"));
        assert_eq!(
            episodes[0].overview.as_deref(),
            Some("Ciro & Genny."),
            "the summary arrives as words, not as a fragment of a web page"
        );
        assert_eq!(episodes[1].overview, None, "and is never invented");
        assert!(service.http.last_url().contains("/shows/2228/episodes"));
    }

    #[test]
    fn a_pack_of_several_seasons_lists_all_of_them() {
        let all = serde_json::json!([
            {"season": 1, "number": 1, "name": "Uno"},
            {"season": 2, "number": 1, "name": "Dos"},
            {"season": 3, "number": 1, "name": "Tres"}
        ])
        .to_string();
        let service = service(vec![FakeHttp::status(200, &all)]);
        let episodes = service.episodes("2228", 1, 2).expect("episodes");
        assert_eq!(episodes.len(), 2);
    }

    #[test]
    fn an_episode_the_database_has_not_named_is_not_given_a_name() {
        let episodes = parse_episodes(&serde_json::json!([{"season": 1, "number": 4, "name": ""}]));
        assert_eq!(episodes[0].title, None);
    }

    #[test]
    fn a_show_without_a_tvdb_id_still_offers_its_own() {
        let answer = serde_json::json!({"id": 7, "externals": {"thetvdb": null}});
        let ids = parse_ids(&answer, "tt1");
        assert_eq!(ids.tvdb, None);
        assert_eq!(ids.tvmaze.as_deref(), Some("7"));
    }
}
