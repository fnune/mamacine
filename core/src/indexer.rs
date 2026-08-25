//! Searching a newznab indexer.

use crate::clock::{age_days, parse_feed_date, Clock};
use crate::error::{Error, Result};
use crate::http::{expect_success, HttpClient, Request};
use crate::release::{tags, Tag};
use crate::settings::IndexerSettings;
use regex::Regex;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub nzb_url: String,
    pub size_bytes: u64,
    pub age_days: Option<f64>,
    pub grabs: u64,
    /// The only completeness hint indexers publish.
    pub thumbs_up: u32,
    pub thumbs_down: u32,
    pub cover_url: Option<String>,
    pub imdb: Option<String>,
    pub about: String,
    pub tags: Vec<Tag>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Movies,
    Television,
    Audio,
    Books,
    Applications,
    Games,
}

impl Category {
    pub fn newznab_id(self) -> &'static str {
        match self {
            Category::Movies => "2000",
            Category::Television => "5000",
            Category::Audio => "3000",
            Category::Books => "7020",
            Category::Applications => "4000",
            Category::Games => "1000",
        }
    }
}

/// Ids find a show under every release name.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShowIds {
    pub tvdb: Option<String>,
    pub tvmaze: Option<String>,
    /// With the `tt`, as newznab takes it.
    pub imdb: Option<String>,
    pub tmdb: Option<String>,
}

impl ShowIds {
    pub fn any(&self) -> bool {
        self.tvdb.is_some() || self.tvmaze.is_some() || self.imdb.is_some() || self.tmdb.is_some()
    }

    /// The first id this indexer understands.
    fn parameter(&self, supported: &[String]) -> Option<(&'static str, String)> {
        let known = |name: &'static str, value: &Option<String>| {
            value
                .as_ref()
                .filter(|_| supported.iter().any(|param| param == name))
                .map(|id| (name, id.clone()))
        };
        known("tvdbid", &self.tvdb)
            .or_else(|| known("imdbid", &self.imdb))
            .or_else(|| known("tvmazeid", &self.tvmaze))
            .or_else(|| known("tmdbid", &self.tmdb))
    }
}

/// An id names one film exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Title(String),
    Imdb(String),
    /// A show, asked for as season packs.
    Show {
        name: String,
        ids: ShowIds,
    },
}

impl Query {
    pub fn parse(text: &str) -> Option<Query> {
        static IMDB: OnceLock<Regex> = OnceLock::new();
        let pattern = IMDB.get_or_init(|| {
            Regex::new(r"(?i)^(?:imdb[:/ ]*)?(?:tt)?(\d{6,9})$").expect("pattern compiles")
        });
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        Some(match pattern.captures(text) {
            Some(found) => Query::Imdb(found[1].to_string()),
            None => Query::Title(crate::search::fold(text)),
        })
    }
}

pub trait Indexer: Send + Sync {
    fn search(&self, query: &Query, category: Option<Category>) -> Result<Vec<SearchResult>>;
    fn capabilities(&self) -> Result<String>;
    fn fetch_nzb(&self, url: &str) -> Result<Vec<u8>>;
    fn cover(&self, url: &str) -> Result<(String, Vec<u8>)>;
    /// Routes a link back to its indexer.
    fn host(&self) -> Option<String>;
}

/// NZBGeek's maximum; the same price as fewer.
pub const RESULT_LIMIT: u16 = 100;

pub struct Newznab<H, C> {
    settings: IndexerSettings,
    http: H,
    clock: C,
    tv_search: Mutex<Option<Vec<String>>>,
}

impl<H: HttpClient, C: Clock> Newznab<H, C> {
    pub fn new(settings: IndexerSettings, http: H, clock: C) -> Self {
        Newznab {
            settings,
            http,
            clock,
            tv_search: Mutex::new(None),
        }
    }

    fn url(&self, parameters: &[(&str, &str)]) -> String {
        let query: Vec<String> = parameters
            .iter()
            .map(|(name, value)| format!("{name}={}", encode_component(value)))
            .collect();
        format!(
            "{}/api?{}",
            self.settings.base_url.trim_end_matches('/'),
            query.join("&")
        )
    }

    fn ask(
        &self,
        parameters: &[(&str, &str)],
        category: Option<Category>,
    ) -> Result<Vec<SearchResult>> {
        let limit = RESULT_LIMIT.to_string();
        let mut all = vec![
            ("apikey", self.settings.api_key.as_str()),
            ("extended", "1"),
            ("limit", limit.as_str()),
        ];
        all.extend_from_slice(parameters);
        if let Some(category) = category {
            all.push(("cat", category.newznab_id()));
        }
        let body = self.get(self.url(&all), "the indexer")?;
        parse_feed(&String::from_utf8_lossy(&body), self.clock.unix_seconds())
    }

    /// Empty `ep` asks newznab for whole seasons.
    fn season_packs(
        &self,
        name: &str,
        ids: &ShowIds,
        category: Option<Category>,
    ) -> Result<Vec<SearchResult>> {
        let (parameter, value) = match ids
            .any()
            .then(|| ids.parameter(&self.tv_search_params()))
            .flatten()
        {
            Some(by_id) => by_id,
            None => ("q", name.to_string()),
        };
        let asked = [("t", "tvsearch"), (parameter, value.as_str())];
        let packs = self.ask(&[asked[0], asked[1], ("ep", "")], category)?;
        if !packs.is_empty() {
            return Ok(packs);
        }
        self.ask(&asked, category)
    }

    /// Asked once; refusal leaves the name search.
    fn tv_search_params(&self) -> Vec<String> {
        let mut remembered = self.tv_search.lock().expect("not poisoned");
        if let Some(known) = remembered.as_ref() {
            return known.clone();
        }
        let found = self
            .get(
                self.url(&[("t", "caps"), ("apikey", &self.settings.api_key)]),
                "the indexer",
            )
            .map(|body| tv_search_params(&String::from_utf8_lossy(&body)))
            .unwrap_or_default();
        *remembered = Some(found.clone());
        found
    }

    fn get(&self, url: String, what: &str) -> Result<Vec<u8>> {
        let request = Request::get(url).header("User-Agent", "MamaCine/1.0");
        Ok(expect_success(what, self.http.send(request)?)?.body)
    }
}

impl<H: HttpClient, C: Clock> Indexer for Newznab<H, C> {
    fn host(&self) -> Option<String> {
        self.settings.host()
    }

    fn search(&self, query: &Query, category: Option<Category>) -> Result<Vec<SearchResult>> {
        match query {
            Query::Imdb(id) => self.ask(&[("t", "movie"), ("imdbid", id.as_str())], category),
            Query::Title(text) => self.ask(&[("t", "search"), ("q", text.as_str())], category),
            Query::Show { name, ids } => self.season_packs(name, ids, category),
        }
    }

    /// Validates the key without spending a search.
    fn capabilities(&self) -> Result<String> {
        let parameters = [("t", "caps"), ("apikey", self.settings.api_key.as_str())];
        let body = self.get(self.url(&parameters), "the indexer")?;
        let text = String::from_utf8_lossy(&body).into_owned();
        let document = roxmltree::Document::parse(&text).map_err(|failure| Error::Unreadable {
            what: "the indexer".into(),
            detail: failure.to_string(),
        })?;
        if document.root_element().tag_name().name() == "error" {
            return Err(service_error(&document));
        }
        let limit = document
            .descendants()
            .find(|node| node.has_tag_name("limits"))
            .and_then(|node| node.attribute("max"))
            .unwrap_or("?");
        Ok(format!("indexer ready, up to {limit} results a query"))
    }

    fn fetch_nzb(&self, url: &str) -> Result<Vec<u8>> {
        self.get(url.to_string(), "the indexer")
    }

    fn cover(&self, url: &str) -> Result<(String, Vec<u8>)> {
        let allowed = match (host_of(url), host_of(&self.settings.base_url)) {
            (Some(cover_host), Some(api_host)) => same_site(&cover_host, &api_host),
            _ => false,
        };
        if !allowed {
            return Err(Error::Setup(
                "cover art must come from the configured indexer".into(),
            ));
        }
        let request = Request::get(url.to_string()).header("User-Agent", "MamaCine/1.0");
        let response = expect_success("the indexer", self.http.send(request)?)?;
        Ok((response.content_type.clone(), response.body))
    }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    Some(rest.split('/').next()?.to_lowercase())
}

fn same_site(left: &str, right: &str) -> bool {
    let tail = |host: &str| {
        host.rsplit('.')
            .take(2)
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    tail(left) == tail(right)
}

pub fn encode_component(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The advertised `supportedParams`, split.
pub fn tv_search_params(xml: &str) -> Vec<String> {
    let Ok(document) = roxmltree::Document::parse(xml) else {
        return Vec::new();
    };
    document
        .descendants()
        .find(|node| node.has_tag_name("tv-search"))
        .and_then(|node| node.attribute("supportedParams"))
        .map(|params| {
            params
                .split(',')
                .map(str::trim)
                .filter(|param| !param.is_empty())
                .map(str::to_lowercase)
                .collect()
        })
        .unwrap_or_default()
}

fn service_error(document: &roxmltree::Document) -> Error {
    let root = document.root_element();
    Error::Refused {
        what: "the indexer".into(),
        status: root
            .attribute("code")
            .and_then(|code| code.parse().ok())
            .unwrap_or(400),
        message: root
            .attribute("description")
            .unwrap_or("no reason given")
            .to_string(),
    }
}

pub fn parse_feed(xml: &str, now: i64) -> Result<Vec<SearchResult>> {
    let document = roxmltree::Document::parse(xml).map_err(|failure| Error::Unreadable {
        what: "the indexer".into(),
        detail: failure.to_string(),
    })?;
    if document.root_element().tag_name().name() == "error" {
        return Err(service_error(&document));
    }

    let mut results = Vec::new();
    for item in document
        .descendants()
        .filter(|node| node.has_tag_name("item"))
    {
        let attribute = |wanted: &str| {
            item.children()
                .filter(|node| node.tag_name().name() == "attr")
                .find(|node| node.attribute("name") == Some(wanted))
                .and_then(|node| node.attribute("value"))
                .map(str::to_string)
        };
        let child = |wanted: &str| {
            item.children()
                .find(|node| node.has_tag_name(wanted))
                .and_then(|node| node.text())
                .map(str::trim)
                .map(str::to_string)
        };
        let enclosure = item.children().find(|node| node.has_tag_name("enclosure"));

        let title = child("title").unwrap_or_else(|| "?".into());
        let Some(nzb_url) = enclosure
            .and_then(|node| node.attribute("url"))
            .map(str::to_string)
            .or_else(|| child("link"))
        else {
            continue;
        };

        let size = attribute("size")
            .or_else(|| {
                enclosure
                    .and_then(|node| node.attribute("length"))
                    .map(str::to_string)
            })
            .and_then(|text| text.parse().ok())
            .unwrap_or(0);

        results.push(SearchResult {
            tags: tags(&title),
            about: describe(&attribute),
            imdb: attribute("imdb"),
            cover_url: attribute("coverurl"),
            grabs: attribute("grabs")
                .and_then(|text| text.parse().ok())
                .unwrap_or(0),
            thumbs_up: attribute("thumbsup")
                .and_then(|text| text.parse().ok())
                .unwrap_or(0),
            thumbs_down: attribute("thumbsdown")
                .and_then(|text| text.parse().ok())
                .unwrap_or(0),
            age_days: child("pubDate")
                .and_then(|text| parse_feed_date(&text))
                .map(|published| age_days(now, published)),
            size_bytes: size,
            nzb_url,
            title,
        });
    }
    Ok(results)
}

fn describe(attribute: &impl Fn(&str) -> Option<String>) -> String {
    [
        attribute("imdbtitle"),
        attribute("imdbyear"),
        attribute("imdbscore").map(|score| format!("★{score}")),
        attribute("genre"),
        attribute("runtime"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::http::fake::FakeHttp;

    const NOW: i64 = 1_787_047_200;

    fn feed() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0" xmlns:newznab="http://www.newznab.com/DTD/2010/feeds/attributes/">
          <channel>
            <item>
              <title>Das.Boot.1981.SPANISH.1080p.BluRay.x264-TEST</title>
              <pubDate>Mon, 17 Aug 2026 10:00:00 +0000</pubDate>
              <enclosure url="https://indexer.test/nzb/1" length="5368709120"/>
              <newznab:attr name="size" value="5368709120"/>
              <newznab:attr name="grabs" value="501"/>
              <newznab:attr name="coverurl" value="https://indexer.test/covers/1.jpg"/>
              <newznab:attr name="imdb" value="0082096"/>
              <newznab:attr name="imdbtitle" value="Das Boot"/>
              <newznab:attr name="imdbyear" value="1981"/>
              <newznab:attr name="imdbscore" value="8.4"/>
              <newznab:attr name="genre" value="Drama, War"/>
              <newznab:attr name="runtime" value="149 min"/>
            </item>
          </channel>
        </rss>"#
            .to_string()
    }

    fn settings() -> IndexerSettings {
        IndexerSettings {
            name: "Test".into(),
            base_url: "https://indexer.test".into(),
            api_key: "secret-key".into(),
            enabled: true,
        }
    }

    fn indexer(answers: Vec<crate::http::Response>) -> Newznab<FakeHttp, FixedClock> {
        Newznab::new(
            settings(),
            FakeHttp::answering(answers),
            FixedClock::at(NOW),
        )
    }

    #[test]
    fn reads_everything_a_row_needs_from_one_item() {
        let results = parse_feed(&feed(), NOW).expect("parsed");
        let first = &results[0];
        assert_eq!(first.title, "Das.Boot.1981.SPANISH.1080p.BluRay.x264-TEST");
        assert_eq!(first.nzb_url, "https://indexer.test/nzb/1");
        assert_eq!(first.size_bytes, 5_368_709_120);
        assert_eq!(first.grabs, 501);
        assert_eq!(first.imdb.as_deref(), Some("0082096"));
        assert_eq!(
            first.cover_url.as_deref(),
            Some("https://indexer.test/covers/1.jpg")
        );
        assert_eq!(first.about, "Das Boot · 1981 · ★8.4 · Drama, War · 149 min");
        assert_eq!(first.age_days, Some(1.0));
        assert!(first.tags.contains(&Tag::Dub("es")));
    }

    #[test]
    fn an_indexer_error_is_reported_as_a_refusal() {
        let xml =
            r#"<?xml version="1.0"?><error code="100" description="Incorrect user credentials"/>"#;
        match parse_feed(xml, NOW) {
            Err(Error::Refused { message, .. }) => {
                assert_eq!(message, "Incorrect user credentials")
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_imdb_id_becomes_a_movie_lookup() {
        let indexer = indexer(vec![FakeHttp::ok(&feed())]);
        let query = Query::parse("tt0082096").expect("a query");
        indexer
            .search(&query, Some(Category::Movies))
            .expect("results");

        let url = indexer.http.last_url();
        assert!(url.contains("t=movie"), "{url}");
        assert!(url.contains("imdbid=0082096"), "{url}");
        assert!(
            !url.contains("&q="),
            "an id search should not also send a title: {url}"
        );
        assert!(url.contains("cat=2000"), "{url}");
    }

    fn empty_feed() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel/></rss>"#.to_string()
    }

    fn caps(tv_params: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><caps><limits max="100"/><searching>
               <search available="yes" supportedParams="q"/>
               <tv-search available="yes" supportedParams="{tv_params}"/>
               </searching></caps>"#
        )
    }

    fn show(name: &str, ids: ShowIds) -> Query {
        Query::Show {
            name: name.to_string(),
            ids,
        }
    }

    #[test]
    fn a_show_is_asked_for_as_whole_seasons_and_not_as_episodes() {
        let indexer = indexer(vec![FakeHttp::ok(&feed())]);
        indexer
            .search(
                &show("game of thrones", ShowIds::default()),
                Some(Category::Television),
            )
            .expect("results");
        let url = indexer.http.last_url();
        assert!(url.contains("t=tvsearch"), "{url}");
        assert!(url.contains("q=game+of+thrones"), "{url}");
        assert!(
            url.contains("ep=&") || url.ends_with("ep="),
            "the empty episode is what asks for the season itself: {url}"
        );
        assert!(url.contains("cat=5000"), "{url}");
    }

    #[test]
    fn a_show_with_an_id_is_asked_for_by_id_and_never_also_by_name() {
        let indexer = indexer(vec![
            FakeHttp::ok(&caps("q,tvdbid,tvmazeid,season,ep")),
            FakeHttp::ok(&feed()),
        ]);
        indexer
            .search(
                &show(
                    "money heist",
                    ShowIds {
                        tvdb: Some("327417".into()),
                        tvmaze: Some("27436".into()),
                        imdb: Some("tt6468322".into()),
                        tmdb: Some("71446".into()),
                    },
                ),
                Some(Category::Television),
            )
            .expect("results");
        let url = indexer.http.last_url();
        assert!(url.contains("tvdbid=327417"), "{url}");
        assert!(
            !url.contains("&q="),
            "the id is the show; a name beside it would let another show in: {url}"
        );
    }

    #[test]
    fn an_id_this_indexer_does_not_take_leaves_the_search_on_the_name() {
        let indexer = indexer(vec![
            FakeHttp::ok(&caps("q,rid,season,ep")),
            FakeHttp::ok(&feed()),
        ]);
        indexer
            .search(
                &show(
                    "money heist",
                    ShowIds {
                        tvdb: Some("327417".into()),
                        ..ShowIds::default()
                    },
                ),
                Some(Category::Television),
            )
            .expect("results");
        let url = indexer.http.last_url();
        assert!(url.contains("q=money+heist"), "{url}");
        assert!(!url.contains("tvdbid"), "{url}");
    }

    #[test]
    fn capabilities_are_asked_for_once_however_many_shows_are_searched() {
        let indexer = indexer(vec![
            FakeHttp::ok(&caps("q,tvdbid,season,ep")),
            FakeHttp::ok(&feed()),
            FakeHttp::ok(&feed()),
        ]);
        let asked = show(
            "gomorrah",
            ShowIds {
                tvdb: Some("281342".into()),
                ..ShowIds::default()
            },
        );
        indexer.search(&asked, None).expect("results");
        indexer.search(&asked, None).expect("results");
        let capability_queries = indexer
            .http
            .requests()
            .iter()
            .filter(|request| request.url.contains("t=caps"))
            .count();
        assert_eq!(capability_queries, 1);
    }

    #[test]
    fn an_indexer_that_answers_nothing_to_the_empty_episode_is_asked_again_without_it() {
        let indexer = indexer(vec![FakeHttp::ok(&empty_feed()), FakeHttp::ok(&feed())]);
        let results = indexer
            .search(
                &show("gomorrah", ShowIds::default()),
                Some(Category::Television),
            )
            .expect("results");
        assert_eq!(results.len(), 1, "the second question is what answered");
        let urls: Vec<String> = indexer
            .http
            .requests()
            .iter()
            .map(|request| request.url.clone())
            .collect();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("ep="), "{:?}", urls[0]);
        assert!(!urls[1].contains("ep="), "{:?}", urls[1]);
    }

    #[test]
    fn what_a_tv_search_accepts_is_read_from_the_capabilities() {
        assert_eq!(
            tv_search_params(&caps("q,rid,tvdbid,tvmazeid,season,ep")),
            vec!["q", "rid", "tvdbid", "tvmazeid", "season", "ep"]
        );
        assert!(tv_search_params("not xml at all").is_empty());
    }

    #[test]
    fn a_title_searches_by_text() {
        let indexer = indexer(vec![FakeHttp::ok(&feed())]);
        indexer
            .search(&Query::parse("das boot").expect("a query"), None)
            .expect("results");
        let url = indexer.http.last_url();
        assert!(url.contains("t=search"), "{url}");
        assert!(url.contains("q=das+boot"), "{url}");
    }

    #[test]
    fn titles_that_look_like_numbers_are_still_titles() {
        assert_eq!(Query::parse("1917"), Some(Query::Title("1917".into())));
        assert_eq!(
            Query::parse("Blade Runner 2049").unwrap(),
            Query::Title("Blade Runner 2049".into())
        );
        assert_eq!(
            Query::parse("  tt0082096 ").unwrap(),
            Query::Imdb("0082096".into())
        );
        assert_eq!(
            Query::parse("imdb:0082096").unwrap(),
            Query::Imdb("0082096".into())
        );
        assert_eq!(Query::parse("   "), None);
    }

    #[test]
    fn an_accented_title_is_asked_for_in_scene_ascii() {
        assert_eq!(
            Query::parse("El espíritu de la colmena"),
            Some(Query::Title("El espiritu de la colmena".into()))
        );
    }

    #[test]
    fn checking_the_key_does_not_spend_a_search() {
        let caps = r#"<caps><limits max="100" default="100"/></caps>"#;
        let indexer = indexer(vec![FakeHttp::ok(caps)]);
        let report = indexer.capabilities().expect("capabilities");
        assert!(report.contains("100"));
        let url = indexer.http.last_url();
        assert!(url.contains("t=caps"), "{url}");
        assert!(!url.contains("t=search"), "{url}");
    }

    #[test]
    fn cover_art_may_only_come_from_the_configured_indexer() {
        let indexer = indexer(vec![]);
        let refused = indexer.cover("https://somewhere-else.test/evil.jpg");
        assert!(matches!(refused, Err(Error::Setup(_))));
        assert!(indexer.http.requests().is_empty(), "it must not even ask");
    }

    #[test]
    fn cover_art_from_a_sister_host_of_the_indexer_is_allowed() {
        let indexer = indexer(vec![FakeHttp::ok("jpg-bytes")]);
        indexer
            .cover("https://imgs.indexer.test/covers/1.jpg")
            .expect("the same site");
        assert_eq!(indexer.http.requests().len(), 1);
    }

    #[test]
    fn the_request_identifies_itself_honestly() {
        let indexer = indexer(vec![FakeHttp::ok(&feed())]);
        indexer
            .search(&Query::parse("das boot").expect("a query"), None)
            .expect("results");
        let request = indexer.http.requests().pop().expect("a request");
        assert_eq!(
            request.headers.get("User-Agent").map(String::as_str),
            Some("MamaCine/1.0")
        );
    }
}
