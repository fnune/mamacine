//! Driving nzbget, as a private instance.

use crate::error::{Error, Result};
use crate::http::{expect_success, HttpClient, Request};
use crate::settings::{NewsServer, Settings};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct QueueItem {
    pub id: i64,
    pub name: String,
    pub status: Status,
    pub downloaded_mb: i64,
    pub total_mb: i64,
    pub remaining_mb: i64,
}

impl QueueItem {
    pub fn percent(&self) -> f64 {
        if self.total_mb <= 0 {
            return 0.0;
        }
        (self.downloaded_mb as f64 / self.total_mb as f64 * 100.0).min(100.0)
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct HistoryItem {
    pub id: i64,
    pub name: String,
    pub succeeded: bool,
    pub status: String,
    pub directory: Option<String>,
    pub size_mb: i64,
    pub total_articles: i64,
    pub failed_articles: i64,
    /// nzbget reports tenths of a percent.
    pub health_percent: f64,
}

/// nzbget's vocabulary, not the person's.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Verifying,
    Repairing,
    Unpacking,
    Moving,
    Finishing,
    Other(String),
}

impl Status {
    pub fn from_nzbget(text: &str) -> Status {
        match text {
            "QUEUED" => Status::Queued,
            "DOWNLOADING" | "FETCHING" => Status::Downloading,
            "PAUSED" => Status::Paused,
            "LOADING_PARS" | "VERIFYING_SOURCES" | "VERIFYING_REPAIRED" => Status::Verifying,
            "REPAIRING" => Status::Repairing,
            "UNPACKING" => Status::Unpacking,
            "MOVING" | "RENAMING" => Status::Moving,
            "PP_QUEUED" | "EXECUTING_SCRIPT" | "PP_FINISHED" => Status::Finishing,
            other => Status::Other(other.to_lowercase().replace('_', " ")),
        }
    }
}

pub trait Downloader: Send + Sync {
    fn append(&self, name: &str, nzb: &[u8]) -> Result<i64>;
    fn queue(&self) -> Result<Vec<QueueItem>>;
    fn history(&self) -> Result<Vec<HistoryItem>>;
    fn download_rate(&self) -> Result<u64>;
    fn cancel(&self, id: i64) -> Result<()>;
    fn forget(&self, id: i64) -> Result<()>;
    /// Tells dead copies from a broken account.
    fn check_server(&self, news: &NewsServer) -> ServerCheck;
}

/// The two failures call for different actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerCheck {
    Working,
    /// Rejected the account: fix the settings.
    Refused(String),
    /// Transient: worth retrying on its own.
    Unreachable(String),
    Unknown,
}

pub struct NzbgetRpc<H> {
    endpoint: String,
    http: H,
}

impl<H: HttpClient> NzbgetRpc<H> {
    pub fn new(port: u16, password: &str, http: H) -> Self {
        NzbgetRpc {
            endpoint: format!("http://127.0.0.1:{port}/mamacine:{password}/jsonrpc"),
            http,
        }
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        let body = serde_json::json!({ "method": method, "params": params });
        let response = expect_success(
            "nzbget",
            self.http
                .send(Request::post_json(self.endpoint.clone(), &body))?,
        )?;
        let answer: Value =
            serde_json::from_slice(&response.body).map_err(|failure| Error::Unreadable {
                what: "nzbget".into(),
                detail: failure.to_string(),
            })?;
        if let Some(problem) = answer.get("error").filter(|value| !value.is_null()) {
            return Err(Error::Refused {
                what: "nzbget".into(),
                status: 500,
                message: problem.to_string(),
            });
        }
        Ok(answer.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn shutdown(&self) -> Result<()> {
        self.call("shutdown", Value::Array(vec![])).map(|_| ())
    }

    pub fn is_ready(&self) -> bool {
        self.call("version", Value::Array(vec![])).is_ok()
    }
}

fn number(item: &Value, field: &str) -> i64 {
    item.get(field).and_then(Value::as_i64).unwrap_or(0)
}

fn text(item: &Value, field: &str) -> String {
    item.get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

impl<H: HttpClient> Downloader for NzbgetRpc<H> {
    fn append(&self, name: &str, nzb: &[u8]) -> Result<i64> {
        let file_name = if name.ends_with(".nzb") {
            name.to_string()
        } else {
            format!("{name}.nzb")
        };
        let params = serde_json::json!([
            file_name,
            base64(nzb),
            "",
            0,
            false,
            false,
            "",
            0,
            "FORCE",
            []
        ]);
        let id = self.call("append", params)?.as_i64().unwrap_or(0);
        if id <= 0 {
            return Err(Error::Refused {
                what: "nzbget".into(),
                status: 400,
                message: "it would not accept the nzb file".into(),
            });
        }
        Ok(id)
    }

    fn queue(&self) -> Result<Vec<QueueItem>> {
        let answer = self.call("listgroups", Value::Array(vec![]))?;
        Ok(answer
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|item| QueueItem {
                id: number(item, "NZBID"),
                name: text(item, "NZBName"),
                status: Status::from_nzbget(&text(item, "Status")),
                downloaded_mb: number(item, "DownloadedSizeMB"),
                total_mb: number(item, "FileSizeMB"),
                remaining_mb: number(item, "RemainingSizeMB"),
            })
            .collect())
    }

    fn history(&self) -> Result<Vec<HistoryItem>> {
        let answer = self.call("history", serde_json::json!([false]))?;
        Ok(answer
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|item| {
                let status = text(item, "Status");
                let final_dir = text(item, "FinalDir");
                let directory = if final_dir.is_empty() {
                    text(item, "DestDir")
                } else {
                    final_dir
                };
                HistoryItem {
                    id: number(item, "NZBID"),
                    name: text(item, "Name"),
                    succeeded: status.starts_with("SUCCESS") || status.starts_with("WARNING"),
                    status,
                    directory: Some(directory).filter(|value| !value.is_empty()),
                    size_mb: number(item, "FileSizeMB"),
                    total_articles: number(item, "TotalArticles"),
                    failed_articles: number(item, "FailedArticles"),
                    health_percent: number(item, "Health") as f64 / 10.0,
                }
            })
            .collect())
    }

    fn download_rate(&self) -> Result<u64> {
        let answer = self.call("status", Value::Array(vec![]))?;
        Ok(answer
            .get("DownloadRate")
            .and_then(Value::as_u64)
            .unwrap_or(0))
    }

    fn cancel(&self, id: i64) -> Result<()> {
        self.call(
            "editqueue",
            serde_json::json!(["GroupFinalDelete", "", [id]]),
        )
        .map(|_| ())
    }

    fn forget(&self, id: i64) -> Result<()> {
        self.call(
            "editqueue",
            serde_json::json!(["HistoryFinalDelete", "", [id]]),
        )
        .map(|_| ())
    }

    fn check_server(&self, news: &NewsServer) -> ServerCheck {
        let params = serde_json::json!([
            news.host,
            news.port,
            news.username,
            news.password,
            news.encrypted,
            "",
            10,
            0
        ]);
        match self.call("testserver", params) {
            Ok(answer) => {
                let text = answer.as_str().unwrap_or_default().to_string();
                let lowered = text.to_lowercase();
                if text.is_empty() || lowered.contains("successful") {
                    ServerCheck::Working
                } else if lowered.contains("authorization") || lowered.contains("authentication") {
                    ServerCheck::Refused(text)
                } else {
                    ServerCheck::Unreachable(text)
                }
            }
            Err(_) => ServerCheck::Unknown,
        }
    }
}

/// Carries the news password; written owner-only.
pub fn render_config(
    settings: &Settings,
    work: &Path,
    port: u16,
    control_password: &str,
    tools: &Tools,
) -> String {
    let news = &settings.news;
    let work = work.display();
    format!(
        "MainDir={work}\n\
         DestDir={destination}\n\
         InterDir={work}/inter\n\
         NzbDir={work}/nzb\n\
         QueueDir={work}/queue\n\
         TempDir={work}/tmp\n\
         ScriptDir={work}/scripts\n\
         LockFile={work}/nzbget.lock\n\
         LogFile={work}/nzbget.log\n\
         WebDir=\n\
         WriteLog=rotate\n\
         RotateLog=3\n\
         OutputMode=log\n\
         CertCheck=no\n\
         ControlIP=127.0.0.1\n\
         ControlPort={port}\n\
         ControlUsername=mamacine\n\
         ControlPassword={control_password}\n\
         SecureControl=no\n\
         UMask=0077\n\
         ArticleCache=200\n\
         WriteBuffer=1024\n\
         DirectWrite=yes\n\
         CrcCheck=yes\n\
         HealthCheck=delete\n\
         ParCheck=force\n\
         ParRepair=yes\n\
         DirectRename=yes\n\
         ParScan=extended\n\
         ParQuick=yes\n\
         Unpack=yes\n\
         DirectUnpack=yes\n\
         UnpackCleanupDisk=yes\n\
         UnrarCmd={unrar}\n\
         SevenZipCmd={sevenzip}\n\
         NzbCleanupDisk=yes\n\
         ExtCleanupDisk=.par2,.sfv,.srr\n\
         DupeCheck=no\n\
         KeepHistory=30\n\
         DiskSpace=4000\n\
         {servers}",
        destination = settings.destination.display(),
        unrar = tools.unrar.display(),
        sevenzip = tools.sevenzip.display(),
        servers = render_server(1, news),
    )
}

fn render_server(index: u8, news: &NewsServer) -> String {
    format!(
        "Server{index}.Active=yes\n\
         Server{index}.Name=primary\n\
         Server{index}.Level=0\n\
         Server{index}.Host={host}\n\
         Server{index}.Port={port}\n\
         Server{index}.Username={username}\n\
         Server{index}.Password={password}\n\
         Server{index}.Encryption={encryption}\n\
         Server{index}.Connections={connections}\n\
         Server{index}.Retention={retention}\n",
        host = news.host,
        port = news.port,
        username = news.username,
        password = news.password,
        encryption = if news.encrypted { "yes" } else { "no" },
        connections = news.connections,
        retention = news.retention_days,
    )
}

/// Programs nzbget needs, shipped on Windows.
#[derive(Clone, Debug)]
pub struct Tools {
    pub nzbget: std::path::PathBuf,
    pub unrar: std::path::PathBuf,
    pub sevenzip: std::path::PathBuf,
}

pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let block = chunk.iter().enumerate().fold(0u32, |block, (index, byte)| {
            block | (*byte as u32) << (16 - 8 * index)
        });
        for slot in 0..4 {
            if slot <= chunk.len() {
                out.push(ALPHABET[(block >> (18 - 6 * slot) & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::settings::{IndexerSettings, SubtitleSettings};
    use std::path::PathBuf;

    fn settings() -> Settings {
        Settings {
            indexers: vec![IndexerSettings {
                name: "Test".into(),
                base_url: "https://indexer.test".into(),
                api_key: "key".into(),
                enabled: true,
            }],
            news: NewsServer {
                host: "news.eweka.nl".into(),
                port: 563,
                username: "reader".into(),
                password: "news-secret".into(),
                encrypted: true,
                connections: 8,
                retention_days: 0,
            },
            subtitles: SubtitleSettings {
                api_key: String::new(),
                user_agent: "mamacine v1.0".into(),
                username: String::new(),
                password: String::new(),
                language: "es".into(),
                api_base: None,
            },
            destination: PathBuf::from("/films"),
            state: PathBuf::from("/state"),
        }
    }

    fn tools() -> Tools {
        Tools {
            nzbget: PathBuf::from("nzbget"),
            unrar: PathBuf::from("/tools/unrar"),
            sevenzip: PathBuf::from("/tools/7z"),
        }
    }

    fn config() -> String {
        render_config(
            &settings(),
            Path::new("/state"),
            6789,
            "control-secret",
            &tools(),
        )
    }

    #[test]
    fn cleans_up_the_archives_after_unpacking() {
        assert!(config().contains("UnpackCleanupDisk=yes"));
    }

    #[test]
    fn deletes_a_download_that_cannot_be_repaired() {
        assert!(config().contains("HealthCheck=delete"));
    }

    #[test]
    fn carries_the_news_server_settings_through() {
        let rendered = config();
        assert!(rendered.contains("Server1.Host=news.eweka.nl"));
        assert!(rendered.contains("Server1.Port=563"));
        assert!(rendered.contains("Server1.Encryption=yes"));
        assert!(rendered.contains("Server1.Connections=8"));
        assert!(rendered.contains("Server1.Password=news-secret"));
    }

    #[test]
    fn keeps_the_control_interface_on_the_loopback_only() {
        let rendered = config();
        assert!(rendered.contains("ControlIP=127.0.0.1"));
        assert!(rendered.contains("ControlPassword=control-secret"));
        assert!(rendered.contains("SecureControl=no"));
    }

    #[test]
    fn base64_matches_the_standard_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(
            base64(b"<?xml version=\"1.0\"?>"),
            "PD94bWwgdmVyc2lvbj0iMS4wIj8+"
        );
    }

    #[test]
    fn appending_sends_the_file_encoded_and_returns_its_id() {
        let rpc = NzbgetRpc::new(
            6789,
            "control-secret",
            FakeHttp::answering(vec![FakeHttp::status(200, r#"{"result": 42}"#)]),
        );
        assert_eq!(rpc.append("Das Boot", b"foobar").expect("an id"), 42);

        let request = rpc.http.requests().pop().expect("a request");
        assert_eq!(
            request.url,
            "http://127.0.0.1:6789/mamacine:control-secret/jsonrpc"
        );
        let body = String::from_utf8(request.body.expect("a body")).expect("utf8");
        assert!(body.contains("Zm9vYmFy"), "{body}");
        assert!(
            body.contains("Das Boot.nzb"),
            "the name gains its extension: {body}"
        );
        assert!(
            body.contains("FORCE"),
            "nzbget must not refuse a retry as a duplicate of its own failed attempt: {body}"
        );
    }

    #[test]
    fn a_copy_that_finished_with_a_warning_is_a_copy_she_has() {
        let rpc = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(
                200,
                r#"{"result":[{"NZBID":7,"Name":"Game Of Thrones","Status":"WARNING/HEALTH",
                "DestDir":"/inter/x","FileSizeMB":30000,"TotalArticles":6257,"FailedArticles":71,
                "Health":987}]}"#,
            )]),
        );
        assert!(rpc.history().expect("history").remove(0).succeeded);
    }

    #[test]
    fn the_configuration_always_repairs_what_it_can() {
        assert!(config().contains("ParCheck=force"), "{}", config());
        assert!(config().contains("ParRepair=yes"), "{}", config());
        assert!(config().contains("DirectRename=yes"), "{}", config());
    }

    #[test]
    fn it_is_not_asked_for_a_web_interface_that_nobody_opens() {
        assert!(config().contains("\nWebDir=\n"), "{}", config());
    }

    #[test]
    fn the_configuration_leaves_duplicate_checking_to_the_app() {
        assert!(config().contains("DupeCheck=no"), "{}", config());
    }

    #[test]
    fn downloads_pause_before_the_disk_is_actually_full() {
        assert!(config().contains("DiskSpace=4000"), "{}", config());
    }

    #[test]
    fn reads_the_queue_into_something_a_row_can_show() {
        let rpc = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(
                200,
                r#"{"result":[{"NZBID":7,"NZBName":"Das Boot","Status":"DOWNLOADING",
                "DownloadedSizeMB":500,"FileSizeMB":2000,"RemainingSizeMB":1500}]}"#,
            )]),
        );
        let queue = rpc.queue().expect("a queue");
        assert_eq!(queue[0].status, Status::Downloading);
        assert_eq!(queue[0].percent(), 25.0);
    }

    #[test]
    fn reads_a_failure_with_the_numbers_that_explain_it() {
        let rpc = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(
                200,
                r#"{"result":[{"NZBID":6,"Name":"Das Boot","Status":"FAILURE/PAR","DestDir":"/inter/x",
                "FileSizeMB":2588,"TotalArticles":6899,"FailedArticles":332,"Health":949}]}"#,
            )]),
        );
        let entry = rpc.history().expect("history").remove(0);
        assert!(!entry.succeeded);
        assert_eq!(entry.failed_articles, 332);
        assert_eq!(entry.health_percent, 94.9);
        assert_eq!(entry.directory.as_deref(), Some("/inter/x"));
    }

    #[test]
    fn an_error_from_nzbget_is_not_silently_swallowed() {
        let rpc = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(
                200,
                r#"{"error":{"message":"unknown method"}}"#,
            )]),
        );
        assert!(matches!(rpc.queue(), Err(Error::Refused { .. })));
    }

    #[test]
    fn the_two_ways_a_server_fails_are_told_apart_because_they_need_different_actions() {
        let working = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(200, r#"{"result":""}"#)]),
        );
        assert_eq!(working.check_server(&settings().news), ServerCheck::Working);
        let body = String::from_utf8(
            working
                .http
                .requests()
                .pop()
                .expect("a request")
                .body
                .expect("a body"),
        )
        .expect("utf8");
        assert!(
            body.contains(r#",10,0]"#),
            "eight parameters, ending in timeout and certificate level: {body}"
        );

        let refused = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(
                200,
                r#"{"result":"Authorization for test server (news.eweka.nl) failed: 502 Authentication Failed"}"#,
            )]),
        );
        assert!(matches!(
            refused.check_server(&settings().news),
            ServerCheck::Refused(_)
        ));

        let unreachable = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(
                200,
                r#"{"result":"Could not resolve hostname news.eweka.nl: Error -2"}"#,
            )]),
        );
        assert!(matches!(
            unreachable.check_server(&settings().news),
            ServerCheck::Unreachable(_)
        ));
    }

    #[test]
    fn an_unaskable_question_is_never_reported_as_an_answer() {
        let silent = NzbgetRpc::new(1, "x", FakeHttp::answering(vec![]));
        assert_eq!(silent.check_server(&settings().news), ServerCheck::Unknown);

        let bad_call = NzbgetRpc::new(
            1,
            "x",
            FakeHttp::answering(vec![FakeHttp::status(
                200,
                r#"{"error":{"name":"JSONRPCError","code":2,"message":"Invalid parameters"}}"#,
            )]),
        );
        assert_eq!(
            bad_call.check_server(&settings().news),
            ServerCheck::Unknown
        );
    }

    #[test]
    fn nzbget_vocabulary_is_translated_rather_than_shown() {
        assert_eq!(Status::from_nzbget("VERIFYING_SOURCES"), Status::Verifying);
        assert_eq!(Status::from_nzbget("PP_QUEUED"), Status::Finishing);
        assert_eq!(
            Status::from_nzbget("SOMETHING_NEW"),
            Status::Other("something new".into())
        );
    }
}
