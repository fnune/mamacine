//! Configuration as a value, passed down.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Several; one down is not fatal.
    pub indexers: Vec<IndexerSettings>,
    pub news: NewsServer,
    pub subtitles: SubtitleSettings,
    pub destination: PathBuf,
    pub state: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexerSettings {
    /// Named in settings and in failures.
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub enabled: bool,
}

impl IndexerSettings {
    pub fn usable(&self) -> bool {
        self.enabled && !self.base_url.trim().is_empty() && !self.api_key.trim().is_empty()
    }

    pub fn host(&self) -> Option<String> {
        let rest = self.base_url.split("://").nth(1)?;
        Some(rest.split('/').next()?.to_lowercase())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewsServer {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub encrypted: bool,
    pub connections: u8,
    pub retention_days: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubtitleSettings {
    pub api_key: String,
    /// Must match the registered consumer name.
    pub user_agent: String,
    pub username: String,
    pub password: String,
    pub language: String,
    /// Only set in tests.
    pub api_base: Option<String>,
}

impl SubtitleSettings {
    /// Searching needs only the key.
    pub fn can_search(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub fn can_download(&self) -> bool {
        self.can_search() && !self.username.is_empty() && !self.password.is_empty()
    }
}
