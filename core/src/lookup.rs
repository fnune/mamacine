//! Looking a film up as she types, so a misspelling is a suggestion rather than a dead end.

use crate::error::{Error, Result};
use crate::http::{expect_success, HttpClient, Request};
use crate::indexer::ShowIds;
use serde_json::Value;

/// One title offered while she types. `id` belongs to whichever provider offered it; the window
/// never interprets it, it only hands the suggestion back by position.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Suggestion {
    pub id: String,
    /// In her language when the provider knows it, else however the provider names it.
    pub title: String,
    /// The original title, only when the provider states it outright and it actually differs.
    /// Guessing at originals would put wrong names in parentheses, so absent means unknown.
    pub original: Option<String>,
    pub year: Option<String>,
    pub series: bool,
    pub poster_url: Option<String>,
}

/// A suggestion turned into something searchable: the query the indexer understands, and the
/// name everything downstream (cards, the shelf, notifications) should call the thing.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Picked {
    pub query: String,
    pub series: bool,
    pub title: String,
    /// A show as the indexer files it, which is what finds its packs whatever they were named.
    /// Empty for a film, whose IMDb id is already the query.
    #[serde(skip)]
    pub show: ShowIds,
}

/// The localized name first, the original beside it, exactly when both are known.
pub fn shown_title(title: &str, original: Option<&str>) -> String {
    match original {
        Some(original) => format!("{title} ({original})"),
        None => title.to_string(),
    }
}

pub struct Lookup<H> {
    http: H,
}

const SUGGESTION_LIMIT: usize = 8;

impl<H: HttpClient> Lookup<H> {
    pub fn new(http: H) -> Self {
        Lookup { http }
    }

    pub fn suggest(&self, text: &str) -> Result<Vec<Suggestion>> {
        let query = text.trim().to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let first = query
            .chars()
            .find(char::is_ascii_alphanumeric)
            .unwrap_or('x');
        let url = format!(
            "https://v2.sg.media-imdb.com/suggestion/{first}/{}.json",
            encode_path_segment(&query)
        );
        // the default agent of a scripting library gets a blanket 403 from indexers
        let request = Request::get(url).header("User-Agent", "MamaCine/1.0");
        let response = expect_success("the title lookup", self.http.send(request)?)?;
        let answer: Value =
            serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
                what: "the title lookup".into(),
                detail: failure.to_string(),
            })?;
        Ok(parse_suggestions(&answer))
    }

    pub fn poster(&self, url: &str) -> Result<(String, Vec<u8>)> {
        // fetched here rather than by the page, so only IMDb's own image host is ever contacted
        if !is_imdb_image_url(url) {
            return Err(Error::Setup(
                "posters must come from IMDb's image host".into(),
            ));
        }
        let request = Request::get(url.to_string()).header("User-Agent", "MamaCine/1.0");
        let response = expect_success("the poster host", self.http.send(request)?)?;
        Ok((response.content_type.clone(), response.body))
    }
}

/// IMDb's endpoint names one primary title and never says which title is the original, so the
/// picked name is passed through untranslated and no parentheses are ever claimed.
pub fn resolve(suggestion: &Suggestion) -> Picked {
    Picked {
        query: if suggestion.series {
            suggestion.title.clone()
        } else {
            format!("tt{}", suggestion.id)
        },
        series: suggestion.series,
        title: suggestion.title.clone(),
        show: ShowIds {
            imdb: suggestion.series.then(|| format!("tt{}", suggestion.id)),
            ..ShowIds::default()
        },
    }
}

pub fn parse_suggestions(answer: &Value) -> Vec<Suggestion> {
    let Some(items) = answer.get("d").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_str)?;
            let imdb = id.strip_prefix("tt")?.to_string();
            let qid = item.get("qid").and_then(Value::as_str)?;
            let series = matches!(qid, "tvSeries" | "tvMiniSeries");
            if !series && !matches!(qid, "movie" | "feature" | "tvMovie" | "short") {
                return None;
            }
            // no year means announced and unreleased ("Aegon's Conquest"): nothing on usenet
            // can satisfy it, so offering it only leads to an empty answer
            let year = item.get("y").and_then(Value::as_i64)?;
            Some(Suggestion {
                id: imdb,
                title: item.get("l").and_then(Value::as_str)?.to_string(),
                original: None,
                year: Some(year.to_string()),
                series,
                poster_url: item
                    .get("i")
                    .and_then(|image| image.get("imageUrl"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .take(SUGGESTION_LIMIT)
        .collect()
}

fn is_imdb_image_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("").to_lowercase();
    host == "media-amazon.com" || host.ends_with(".media-amazon.com")
}

/// IMDb's endpoint takes the query as a path segment, where a `+` is a literal plus,
/// so a space must be `%20` and `indexer::encode_component` would corrupt it.
fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::http::Response;

    fn answer() -> Value {
        serde_json::json!({"d": [
            {"id": "tt0082096", "l": "Das Boot", "y": 1981, "q": "feature", "qid": "movie",
             "i": {"imageUrl": "https://m.media-amazon.com/images/M/boot.jpg",
                   "width": 679, "height": 1000}},
            {"id": "tt0106004", "l": "Das Boot", "y": 2018, "q": "TV series", "qid": "tvSeries"},
            {"id": "nm0000000", "l": "Jürgen Prochnow", "qid": "name"},
            {"id": "tt9999991", "l": "Das Boot: The Game", "y": 1990, "qid": "videoGame"}
        ]})
    }

    fn lookup(answers: Vec<Response>) -> Lookup<FakeHttp> {
        Lookup::new(FakeHttp::answering(answers))
    }

    fn suggestions_body() -> Response {
        FakeHttp::status(200, &answer().to_string())
    }

    #[test]
    fn a_realistic_answer_becomes_suggestions_a_row_can_show() {
        let found = parse_suggestions(&answer());
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "0082096");
        assert_eq!(found[0].title, "Das Boot");
        assert_eq!(found[0].year.as_deref(), Some("1981"));
        assert!(!found[0].series);
        assert_eq!(
            found[0].poster_url.as_deref(),
            Some("https://m.media-amazon.com/images/M/boot.jpg")
        );
        assert!(found[1].series);
        assert_eq!(found[1].poster_url, None);
    }

    #[test]
    fn a_picked_film_searches_by_id_and_a_series_by_its_name() {
        let film = Suggestion {
            id: "0082096".into(),
            title: "Das Boot".into(),
            original: None,
            year: Some("1981".into()),
            series: false,
            poster_url: None,
        };
        assert_eq!(resolve(&film).query, "tt0082096");
        assert_eq!(resolve(&film).title, "Das Boot");

        let series = Suggestion {
            series: true,
            title: "Money Heist".into(),
            ..film.clone()
        };
        assert_eq!(resolve(&series).query, "Money Heist");
        assert!(resolve(&series).series);
        assert_eq!(
            resolve(&series).show.imdb.as_deref(),
            Some("tt0082096"),
            "the id is what the show search is turned into, and the name is the fallback"
        );
        assert!(!resolve(&film).show.any(), "a film is already an id");
    }

    // IMDb never says which title is the original, so this provider must never invent one.
    #[test]
    fn no_original_title_is_ever_claimed_without_a_source_that_states_it() {
        for suggestion in parse_suggestions(&answer()) {
            assert_eq!(suggestion.original, None);
        }
        assert_eq!(
            shown_title("El hoyo", Some("The Platform")),
            "El hoyo (The Platform)"
        );
        assert_eq!(shown_title("Coco", None), "Coco");
    }

    #[test]
    fn an_announced_title_without_a_year_is_not_offered() {
        // seen live: "Aegon's Conquest" and "Snow", announced spin-offs nothing can download
        let found = parse_suggestions(&serde_json::json!({"d": [
            {"id": "tt31178266", "l": "Aegon's Conquest", "qid": "movie"},
            {"id": "tt0082096", "l": "Das Boot", "y": 1981, "qid": "movie"}
        ]}));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Das Boot");
    }

    #[test]
    fn people_and_video_games_are_not_suggested() {
        let found = parse_suggestions(&answer());
        assert!(found
            .iter()
            .all(|suggestion| suggestion.title == "Das Boot"));
    }

    #[test]
    fn the_query_is_lowercased_and_spaces_become_percent_twenty() {
        let lookup = lookup(vec![suggestions_body()]);
        lookup.suggest("  Das Boot ").expect("suggestions");
        assert_eq!(
            lookup.http.last_url(),
            "https://v2.sg.media-imdb.com/suggestion/d/das%20boot.json"
        );
    }

    #[test]
    fn a_query_without_a_letter_or_digit_still_has_a_path_segment() {
        let lookup = lookup(vec![suggestions_body()]);
        lookup.suggest("¡¿?!").expect("suggestions");
        let url = lookup.http.last_url();
        assert!(url.contains("/suggestion/x/"), "{url}");
    }

    #[test]
    fn typing_nothing_asks_nobody() {
        let lookup = lookup(vec![]);
        let found = lookup.suggest("   ").expect("no suggestions");
        assert!(found.is_empty());
        assert!(lookup.http.requests().is_empty(), "it must not even ask");
    }

    #[test]
    fn at_most_eight_suggestions_are_kept_in_the_order_given() {
        let many: Vec<Value> = (0..12)
            .map(|n| {
                serde_json::json!({"id": format!("tt000000{n}"), "l": format!("Film {n}"),
                                   "y": 2000 + n, "qid": "movie"})
            })
            .collect();
        let found = parse_suggestions(&serde_json::json!({ "d": many }));
        assert_eq!(found.len(), 8);
        assert_eq!(found[0].title, "Film 0");
        assert_eq!(found[7].title, "Film 7");
    }

    #[test]
    fn the_request_identifies_itself_honestly() {
        let lookup = lookup(vec![suggestions_body()]);
        lookup.suggest("das boot").expect("suggestions");
        let request = lookup.http.requests().pop().expect("a request");
        assert_eq!(
            request.headers.get("User-Agent").map(String::as_str),
            Some("MamaCine/1.0")
        );
    }

    #[test]
    fn an_unreadable_answer_is_reported_as_such() {
        let lookup = lookup(vec![FakeHttp::status(200, "not json")]);
        let failed = lookup.suggest("das boot");
        assert!(matches!(failed, Err(Error::Unreadable { .. })));
    }

    #[test]
    fn posters_may_only_come_from_imdbs_image_host() {
        let lookup = lookup(vec![]);
        for url in [
            "https://somewhere-else.test/boot.jpg",
            "http://m.media-amazon.com/boot.jpg",
            "https://media-amazon.com.evil.test/boot.jpg",
        ] {
            let refused = lookup.poster(url);
            assert!(matches!(refused, Err(Error::Setup(_))), "{url}");
        }
        assert!(lookup.http.requests().is_empty(), "it must not even ask");
    }

    #[test]
    fn a_fetched_poster_carries_its_content_type() {
        let lookup = lookup(vec![Response {
            status: 200,
            content_type: "image/jpeg".into(),
            body: b"jpeg bytes".to_vec(),
        }]);
        let (content_type, bytes) = lookup
            .poster("https://m.media-amazon.com/images/M/boot.jpg")
            .expect("a poster");
        assert_eq!(content_type, "image/jpeg");
        assert_eq!(bytes, b"jpeg bytes");
    }
}
