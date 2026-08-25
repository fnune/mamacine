//! The subtitle service, metered by somebody else.

use crate::clock::{parse_utc_instant, Clock};
use crate::error::{Error, Result};
use crate::http::{expect_success, HttpClient, Request, Response};
use crate::settings::SubtitleSettings;
use crate::subtitles::Candidate;
use serde_json::Value;
use std::sync::Mutex;

const TOKEN_LIFETIME_SECONDS: i64 = 20 * 3600;
const SEARCH_CACHE_SECONDS: i64 = 7 * 86_400;
/// A throttle pauses; only 406 spends the day.
pub const THROTTLE_PAUSE_SECONDS: i64 = 15 * 60;
const QUOTA_FALLBACK_SECONDS: i64 = 24 * 3600;

#[derive(Default)]
struct Cached {
    token: Option<(String, i64)>,
    searches: Vec<(String, i64, Vec<Candidate>)>,
    downloads_remaining: Option<i64>,
    downloads_blocked_until: Option<i64>,
}

pub trait SubtitleSource: Send + Sync {
    fn find(&self, query: &SubtitleQuery) -> Result<Vec<Candidate>>;
    fn download(&self, file_id: i64) -> Result<Vec<u8>>;
    fn downloads_remaining(&self) -> Option<i64>;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubtitleQuery {
    pub language: String,
    pub imdb_id: Option<String>,
    pub movie_hash: Option<String>,
    pub file_name: Option<String>,
}

impl SubtitleQuery {
    fn parameters(&self) -> Vec<(&str, String)> {
        let mut parameters = vec![("languages", self.language.clone())];
        if let Some(id) = &self.imdb_id {
            if let Ok(number) = id.trim_start_matches('0').parse::<u64>() {
                parameters.push(("imdb_id", number.to_string()));
            }
        }
        if let Some(hash) = &self.movie_hash {
            parameters.push(("moviehash", hash.clone()));
        }
        if self.imdb_id.is_none() {
            if let Some(name) = &self.file_name {
                parameters.push(("query", name.clone()));
            }
        }
        parameters.sort();
        parameters
    }

    fn cache_key(&self) -> String {
        self.parameters()
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&")
    }
}

pub struct OpenSubtitles<H, C> {
    settings: SubtitleSettings,
    http: H,
    clock: C,
    cached: Mutex<Cached>,
}

impl<H: HttpClient, C: Clock> OpenSubtitles<H, C> {
    pub fn new(settings: SubtitleSettings, http: H, clock: C) -> Self {
        OpenSubtitles {
            settings,
            http,
            clock,
            cached: Mutex::new(Cached::default()),
        }
    }

    fn request(&self, request: Request) -> Request {
        request
            .header("Api-Key", self.settings.api_key.clone())
            .header("User-Agent", self.settings.user_agent.clone())
            .header("Accept", "application/json")
    }

    fn json(&self, request: Request) -> Result<Value> {
        let response = expect_success("opensubtitles", self.http.send(request)?)?;
        serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
            what: "opensubtitles".into(),
            detail: failure.to_string(),
        })
    }

    fn token(&self) -> Result<String> {
        if !self.settings.can_download() {
            return Err(Error::Setup(
                "Downloading subtitles needs the OpenSubtitles account itself, not only the key. \
                 Use the username shown on your profile, not your email address."
                    .into(),
            ));
        }
        let now = self.clock.unix_seconds();
        if let Some((token, issued)) = &self.cached.lock().expect("not poisoned").token {
            if now - issued < TOKEN_LIFETIME_SECONDS {
                return Ok(token.clone());
            }
        }

        let body = serde_json::json!({
            "username": self.settings.username,
            "password": self.settings.password,
        });
        let answer = self.json(self.request(Request::post_json(self.url("/login"), &body)))?;
        let token = answer
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Unreadable {
                what: "opensubtitles".into(),
                detail: "the login returned no token".into(),
            })?
            .to_string();
        self.cached.lock().expect("not poisoned").token = Some((token.clone(), now));
        Ok(token)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.settings.base_url().trim_end_matches('/'))
    }

    /// Logging in validates key and account, spending nothing.
    pub fn check_account(&self) -> Result<()> {
        self.token().map(|_| ())
    }
}

impl SubtitleSettings {
    pub fn base_url(&self) -> String {
        self.api_base
            .clone()
            .unwrap_or_else(|| "https://api.opensubtitles.com/api/v1".to_string())
    }
}

impl<H: HttpClient, C: Clock> SubtitleSource for OpenSubtitles<H, C> {
    fn find(&self, query: &SubtitleQuery) -> Result<Vec<Candidate>> {
        if !self.settings.can_search() {
            return Err(Error::Setup("No OpenSubtitles key is set.".into()));
        }
        let key = query.cache_key();
        let now = self.clock.unix_seconds();
        {
            let mut cached = self.cached.lock().expect("not poisoned");
            cached
                .searches
                .retain(|(_, at, _)| now - at < SEARCH_CACHE_SECONDS);
            if let Some((_, _, found)) =
                cached.searches.iter().find(|(cached, _, _)| cached == &key)
            {
                return Ok(found.clone());
            }
        }

        let query_string: Vec<String> = query
            .parameters()
            .iter()
            .map(|(name, value)| format!("{name}={}", crate::indexer::encode_component(value)))
            .collect();
        let url = self.url(&format!("/subtitles?{}", query_string.join("&")));
        let answer = self.json(self.request(Request::get(url)))?;
        let found = parse_candidates(&answer);

        let mut cached = self.cached.lock().expect("not poisoned");
        cached.searches.push((key, now, found.clone()));
        Ok(found)
    }

    fn download(&self, file_id: i64) -> Result<Vec<u8>> {
        if self.downloads_remaining() == Some(0) {
            return Err(Error::Setup(
                "Today's subtitle allowance is used up. It resets tomorrow.".into(),
            ));
        }
        let body = serde_json::json!({ "file_id": file_id });
        let request = self
            .request(Request::post_json(self.url("/download"), &body))
            .header("Authorization", format!("Bearer {}", self.token()?));
        let response = self.http.send(request)?;
        if !(200..300).contains(&response.status) {
            self.note_refusal(&response);
            return Err(expect_success("opensubtitles", response).expect_err("not a success"));
        }
        let answer: Value =
            serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
                what: "opensubtitles".into(),
                detail: failure.to_string(),
            })?;

        if let Some(remaining) = answer.get("remaining").and_then(Value::as_i64) {
            let mut cached = self.cached.lock().expect("not poisoned");
            cached.downloads_remaining = Some(remaining);
            if remaining <= 0 {
                cached.downloads_blocked_until =
                    Some(reset_named(&answer, self.clock.unix_seconds()));
            }
        }
        let link = answer
            .get("link")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Unreadable {
                what: "opensubtitles".into(),
                detail: "the download gave no link".into(),
            })?;

        let file =
            Request::get(link.to_string()).header("User-Agent", self.settings.user_agent.clone());
        Ok(expect_success("the subtitle file host", self.http.send(file)?)?.body)
    }

    fn downloads_remaining(&self) -> Option<i64> {
        let now = self.clock.unix_seconds();
        let mut cached = self.cached.lock().expect("not poisoned");
        match cached.downloads_blocked_until {
            Some(until) if now < until => return Some(0),
            Some(_) => {
                cached.downloads_blocked_until = None;
                if cached.downloads_remaining == Some(0) {
                    cached.downloads_remaining = None;
                }
            }
            None => {}
        }
        cached.downloads_remaining
    }
}

impl<H: HttpClient, C: Clock> OpenSubtitles<H, C> {
    fn note_refusal(&self, response: &Response) {
        let now = self.clock.unix_seconds();
        let mut cached = self.cached.lock().expect("not poisoned");
        match response.status {
            406 => {
                cached.downloads_remaining = Some(0);
                let answer: Value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
                cached.downloads_blocked_until = Some(reset_named(&answer, now));
            }
            429 => cached.downloads_blocked_until = Some(now + THROTTLE_PAUSE_SECONDS),
            _ => {}
        }
    }
}

fn reset_named(answer: &Value, now: i64) -> i64 {
    answer
        .get("reset_time_utc")
        .and_then(Value::as_str)
        .and_then(parse_utc_instant)
        .filter(|instant| *instant > now)
        .unwrap_or(now + QUOTA_FALLBACK_SECONDS)
}

pub fn parse_candidates(answer: &Value) -> Vec<Candidate> {
    let Some(items) = answer.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let attributes = item.get("attributes")?;
            let file = attributes.get("files")?.as_array()?.first()?;
            Some(Candidate {
                file_id: file.get("file_id")?.as_i64()?,
                release: attributes
                    .get("release")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                hash_match: attributes
                    .get("moviehash_match")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                fps: attributes
                    .get("fps")
                    .and_then(Value::as_f64)
                    .filter(|fps| *fps > 0.0),
                downloads: attributes
                    .get("download_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                rating: attributes
                    .get("ratings")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                trusted: attributes
                    .get("from_trusted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                machine_translated: attributes
                    .get("ai_translated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || attributes
                        .get("machine_translated")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                foreign_parts_only: attributes
                    .get("foreign_parts_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                uploader: attributes
                    .get("uploader")
                    .and_then(|node| node.get("uploader_id"))
                    .and_then(Value::as_i64),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::http::fake::FakeHttp;
    use crate::http::Response;

    fn settings() -> SubtitleSettings {
        SubtitleSettings {
            api_key: "a-key".into(),
            user_agent: "mamacine v1.0".into(),
            username: "mother".into(),
            password: "hunter2".into(),
            language: "es".into(),
            api_base: Some("https://subs.test/api/v1".into()),
        }
    }

    fn service(answers: Vec<Response>) -> OpenSubtitles<FakeHttp, FixedClock> {
        OpenSubtitles::new(
            settings(),
            FakeHttp::answering(answers),
            FixedClock::at(1000),
        )
    }

    fn search_answer() -> Response {
        FakeHttp::status(
            200,
            r#"{"data":[
                {"attributes":{"release":"PAL.Version","download_count":9000,"fps":25.0,
                  "moviehash_match":false,"files":[{"file_id":11}]}},
                {"attributes":{"release":"Exact","download_count":5,"fps":23.976,
                  "moviehash_match":true,"files":[{"file_id":22}]}}]}"#,
        )
    }

    fn query() -> SubtitleQuery {
        SubtitleQuery {
            language: "es".into(),
            imdb_id: Some("0082096".into()),
            movie_hash: Some("e946157bfd22b62f".into()),
            file_name: Some("Das Boot 1981".into()),
        }
    }

    #[test]
    fn reads_every_signal_a_ranking_needs() {
        let service = service(vec![search_answer()]);
        let found = service.find(&query()).expect("candidates");
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].file_id, 22);
        assert!(found[1].hash_match);
        assert_eq!(found[0].fps, Some(25.0));
        assert_eq!(found[0].downloads, 9000);
    }

    #[test]
    fn searches_by_id_and_hash_and_strips_leading_zeros() {
        let service = service(vec![search_answer()]);
        service.find(&query()).expect("candidates");
        let url = service.http.last_url();
        assert!(url.contains("imdb_id=82096"), "{url}");
        assert!(url.contains("moviehash=e946157bfd22b62f"), "{url}");
        assert!(
            !url.contains("query="),
            "an id is better than a title: {url}"
        );
    }

    #[test]
    fn falls_back_to_the_file_name_when_there_is_no_id() {
        let service = service(vec![search_answer()]);
        let mut without_id = query();
        without_id.imdb_id = None;
        service.find(&without_id).expect("candidates");
        assert!(service.http.last_url().contains("query=Das+Boot+1981"));
    }

    #[test]
    fn a_repeated_search_is_answered_from_the_cache() {
        let service = service(vec![search_answer()]);
        service.find(&query()).expect("candidates");
        service.find(&query()).expect("candidates from cache");
        assert_eq!(
            service.http.requests().len(),
            1,
            "the service should be asked once"
        );
    }

    #[test]
    fn the_cache_expires_rather_than_going_stale_forever() {
        let service = service(vec![search_answer(), search_answer()]);
        service.find(&query()).expect("candidates");
        service.clock.advance(SEARCH_CACHE_SECONDS + 1);
        service.find(&query()).expect("candidates again");
        assert_eq!(service.http.requests().len(), 2);
    }

    #[test]
    fn one_login_covers_every_download() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"token":"signed"}"#),
            FakeHttp::status(200, r#"{"link":"https://files.test/1","remaining":9}"#),
            FakeHttp::status(200, "hola"),
            FakeHttp::status(200, r#"{"link":"https://files.test/2","remaining":8}"#),
            FakeHttp::status(200, "adios"),
        ]);
        assert_eq!(service.download(11).expect("first"), b"hola");
        assert_eq!(service.download(22).expect("second"), b"adios");
        let logins = service
            .http
            .requests()
            .iter()
            .filter(|request| request.url.ends_with("/login"))
            .count();
        assert_eq!(logins, 1);
        assert_eq!(service.downloads_remaining(), Some(8));
    }

    #[test]
    fn the_file_host_is_never_sent_the_api_key() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"token":"signed"}"#),
            FakeHttp::status(200, r#"{"link":"https://files.test/1","remaining":9}"#),
            FakeHttp::status(200, "hola"),
        ]);
        service.download(11).expect("a subtitle");
        let last = service.http.requests().pop().expect("a request");
        assert_eq!(last.url, "https://files.test/1");
        assert!(!last.headers.contains_key("Api-Key"));
        assert!(!last.headers.contains_key("Authorization"));
    }

    #[test]
    fn stops_asking_once_the_allowance_is_gone() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"token":"signed"}"#),
            FakeHttp::status(200, r#"{"link":"https://files.test/1","remaining":0}"#),
            FakeHttp::status(200, "hola"),
        ]);
        service.download(11).expect("the last one");
        let before = service.http.requests().len();
        let refused = service.download(22);
        assert!(matches!(refused, Err(Error::Setup(_))));
        assert_eq!(
            service.http.requests().len(),
            before,
            "it must not even ask"
        );
    }

    #[test]
    fn a_refusal_for_too_many_requests_is_the_allowance_answering() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"token":"signed"}"#),
            FakeHttp::status(429, r#"{"message":"throttle limit reached"}"#),
        ]);
        assert!(service.download(11).is_err());
        assert_eq!(service.downloads_remaining(), Some(0));
        let before = service.http.requests().len();
        assert!(matches!(service.download(22), Err(Error::Setup(_))));
        assert_eq!(
            service.http.requests().len(),
            before,
            "the next episode must not ask again"
        );
    }

    #[test]
    fn a_throttle_pause_ends_rather_than_lasting_until_a_restart() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"token":"signed"}"#),
            FakeHttp::status(429, r#"{"message":"throttle limit reached"}"#),
            FakeHttp::status(200, r#"{"link":"https://files.test/1","remaining":9}"#),
            FakeHttp::status(200, "hola"),
        ]);
        assert!(service.download(11).is_err());
        assert_eq!(
            service.downloads_remaining(),
            Some(0),
            "paused while throttled"
        );
        service.clock.advance(THROTTLE_PAUSE_SECONDS + 1);
        assert_eq!(
            service.downloads_remaining(),
            None,
            "unknown, not still zero"
        );
        assert_eq!(service.download(11).expect("after the pause"), b"hola");
    }

    #[test]
    fn the_daily_quota_refusal_blocks_until_the_reset_it_names() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"token":"signed"}"#),
            FakeHttp::status(
                406,
                r#"{"remaining":0,"message":"quota reached","reset_time_utc":"1970-01-01T01:00:00.000Z"}"#,
            ),
            FakeHttp::status(200, r#"{"link":"https://files.test/1","remaining":19}"#),
            FakeHttp::status(200, "hola"),
        ]);
        assert!(matches!(
            service.download(11),
            Err(Error::Refused { status: 406, .. })
        ));
        let before = service.http.requests().len();
        assert!(matches!(service.download(22), Err(Error::Setup(_))));
        assert_eq!(
            service.http.requests().len(),
            before,
            "it must not ask while the allowance is spent"
        );
        service.clock.advance(2601);
        assert_eq!(service.download(22).expect("after the reset"), b"hola");
    }

    #[test]
    fn a_quota_refusal_naming_no_reset_blocks_for_a_day_not_forever() {
        let service = service(vec![
            FakeHttp::status(200, r#"{"token":"signed"}"#),
            FakeHttp::status(406, r#"{"message":"quota reached"}"#),
        ]);
        assert!(service.download(11).is_err());
        assert_eq!(service.downloads_remaining(), Some(0));
        service.clock.advance(24 * 3600 + 1);
        assert_eq!(
            service.downloads_remaining(),
            None,
            "a new day starts unknown"
        );
    }

    #[test]
    fn expired_searches_are_pruned_rather_than_hoarded() {
        let service = service(vec![search_answer(), search_answer()]);
        service.find(&query()).expect("candidates");
        service.clock.advance(SEARCH_CACHE_SECONDS + 1);
        service.find(&query()).expect("candidates again");
        assert_eq!(
            service.cached.lock().expect("not poisoned").searches.len(),
            1,
            "the stale answer is gone, not merely shadowed"
        );
    }

    #[test]
    fn says_plainly_when_the_account_is_missing() {
        let mut settings = settings();
        settings.username = String::new();
        let service = OpenSubtitles::new(settings, FakeHttp::answering(vec![]), FixedClock::at(0));
        match service.download(11) {
            Err(Error::Setup(message)) => {
                assert!(message.contains("not your email"), "{message}");
            }
            other => panic!("expected a setup error, got {other:?}"),
        }
    }
}
