//! Configuration as a value. Built once at the composition root and passed down from there.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Several, because finding more things means asking more places. One that is misconfigured or
    /// down is a line in a list, not a broken app.
    pub indexers: Vec<IndexerSettings>,
    pub news: NewsServer,
    pub subtitles: SubtitleSettings,
    pub destination: PathBuf,
    pub state: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexerSettings {
    /// What it is called in the settings screen, and what is named when it fails.
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
    /// Must match the consumer name registered with the service, which enforces it on search.
    pub user_agent: String,
    pub username: String,
    pub password: String,
    pub language: String,
    /// Only set in tests, which point it at a fake.
    pub api_base: Option<String>,
}

impl SubtitleSettings {
    /// Searching needs only the key; downloading needs the account behind it.
    pub fn can_search(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub fn can_download(&self) -> bool {
        self.can_search() && !self.username.is_empty() && !self.password.is_empty()
    }
}
