//! Reading configuration from disk. Deliberately the only module that does.

use mamacine_core::nzbget::Tools;
use mamacine_core::settings::{IndexerSettings, NewsServer, Settings, SubtitleSettings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// One place to search. Adding another is how the app learns to find more.
#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct StoredIndexer {
    pub name: String,
    pub url: String,
    pub key: String,
    pub enabled: bool,
}

/// Bumped when the shape or meaning of a field changes; `read` migrates older files and `write`
/// refuses to clobber newer ones, because this app will be updated again.
pub const SETTINGS_VERSION: u32 = 2;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct StoredSettings {
    pub version: u32,
    pub indexers: Vec<StoredIndexer>,
    /// What the app stored before it could hold more than one. Read once, then written as a list.
    pub indexer_url: String,
    pub indexer_key: String,
    pub news_host: String,
    pub news_port: u16,
    pub news_user: String,
    pub news_password: String,
    pub news_encrypted: bool,
    pub news_connections: u8,
    pub tmdb_key: String,
    pub subtitles_key: String,
    pub subtitles_agent: String,
    pub subtitles_user: String,
    pub subtitles_password: String,
    pub destination: Option<PathBuf>,
    /// Which language she prefers a film in: "es", "original" or "any". A fact about her, set
    /// once here, rather than a chip she has to press before every search.
    pub language: String,
    /// Open with the computer, so the app is simply there, like the clock.
    pub autostart: bool,
    /// Closing the window keeps the downloads going, tucked into the tray by the clock. On by
    /// default: a download she started should not die because she tidied a window away.
    pub keep_running: bool,
}

impl Default for StoredSettings {
    fn default() -> StoredSettings {
        StoredSettings {
            version: 0,
            indexers: Vec::new(),
            indexer_url: String::new(),
            indexer_key: String::new(),
            news_host: String::new(),
            news_port: 0,
            news_user: String::new(),
            news_password: String::new(),
            // news servers hand out port 563 with TLS; plain text is the thing to opt into
            news_encrypted: true,
            news_connections: 0,
            tmdb_key: String::new(),
            subtitles_key: String::new(),
            subtitles_agent: String::new(),
            subtitles_user: String::new(),
            subtitles_password: String::new(),
            destination: None,
            language: "any".into(),
            autostart: false,
            keep_running: true,
        }
    }
}

pub fn path(handle: &AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let directory = handle.path().app_config_dir()?;
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join("settings.json"))
}

pub fn read(handle: &AppHandle) -> StoredSettings {
    let Ok(path) = path(handle) else {
        return StoredSettings::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return StoredSettings::default(); // the first run ever
    };
    match serde_json::from_slice::<StoredSettings>(&bytes) {
        Ok(mut stored) => {
            migrate(&mut stored);
            stored
        }
        Err(_) => {
            // a newer file that this version cannot read must survive a downgrade untouched;
            // anything else unreadable is set aside rather than silently replaced with nothing
            if !newer_than_app(&bytes) {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since| since.as_secs())
                    .unwrap_or(0);
                let _ = std::fs::rename(&path, path.with_extension(format!("broken-{stamp}.json")));
            }
            StoredSettings::default()
        }
    }
}

/// Whether the file on disk was written by a version of the app newer than this one.
pub fn newer_than_app(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| value.get("version").and_then(serde_json::Value::as_u64))
        .map(|version| version > u64::from(SETTINGS_VERSION))
        .unwrap_or(false)
}

/// A settings file written before the app could hold several indexers still describes one.
fn migrate(stored: &mut StoredSettings) {
    if !stored.indexers.is_empty() || stored.indexer_key.trim().is_empty() {
        return;
    }
    let url = non_empty(&stored.indexer_url, "https://api.nzbgeek.info");
    stored.indexers.push(StoredIndexer {
        name: name_from(&url),
        key: stored.indexer_key.clone(),
        enabled: true,
        url,
    });
    stored.indexer_url = String::new();
    stored.indexer_key = String::new();
}

/// A name she would recognise, taken from the address, until someone types a better one.
fn name_from(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|host| host.trim_start_matches("api.").to_string())
        .unwrap_or_else(|| "Buscador".to_string())
}

/// What the window sent, folded into what was stored. Pure, because the last defect here was
/// silent: the port arrived as the string every input field produces, the number parse quietly
/// failed, and editing the port did nothing at all.
pub fn apply(stored: &mut StoredSettings, incoming: &serde_json::Value) {
    let text = |field: &str| {
        incoming
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .map(str::to_string)
    };
    // an input field yields text even when it holds a number; both spellings must count
    let number = |field: &str| {
        incoming.get(field).and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
    };

    // the whole list arrives at once: rows are added and removed in the window, not here
    if let Some(rows) = incoming
        .get("indexers")
        .and_then(serde_json::Value::as_array)
    {
        stored.indexers = rows
            .iter()
            .filter_map(|row| {
                let field = |name: &str| {
                    row.get(name)
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                };
                let url = field("url");
                if url.is_empty() {
                    return None;
                }
                Some(StoredIndexer {
                    name: field("name"),
                    key: field("key"),
                    enabled: row
                        .get("enabled")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true),
                    url,
                })
            })
            .collect();
    }
    if let Some(value) = text("news_host") {
        stored.news_host = value
    }
    if let Some(value) = text("news_user") {
        stored.news_user = value
    }
    // a blank password field means "leave it as it is", so saving cannot silently erase one
    if let Some(value) = text("news_password").filter(|value| !value.is_empty()) {
        stored.news_password = value;
    }
    if let Some(value) = text("tmdb_key") {
        stored.tmdb_key = value
    }
    if let Some(value) = text("subtitles_key") {
        stored.subtitles_key = value
    }
    if let Some(value) = text("subtitles_agent") {
        stored.subtitles_agent = value
    }
    if let Some(value) = text("subtitles_user") {
        stored.subtitles_user = value
    }
    if let Some(value) = text("subtitles_password").filter(|value| !value.is_empty()) {
        stored.subtitles_password = value;
    }
    if let Some(value) = text("destination").filter(|value| !value.is_empty()) {
        stored.destination = Some(value.into());
    }
    if let Some(value) = text("language") {
        stored.language = value;
    }
    if let Some(port) = number("news_port") {
        stored.news_port = port as u16;
    }
    if let Some(connections) = number("news_connections") {
        stored.news_connections = connections.clamp(1, 50) as u8;
    }
    if let Some(encrypted) = incoming
        .get("news_encrypted")
        .and_then(serde_json::Value::as_bool)
    {
        stored.news_encrypted = encrypted;
    }
    if let Some(wanted) = incoming
        .get("autostart")
        .and_then(serde_json::Value::as_bool)
    {
        stored.autostart = wanted;
    }
    if let Some(wanted) = incoming
        .get("keep_running")
        .and_then(serde_json::Value::as_bool)
    {
        stored.keep_running = wanted;
    }
}

pub fn write(
    handle: &AppHandle,
    stored: &StoredSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path(handle)?;
    // half-understanding a newer file and then writing over it would destroy it
    if let Ok(existing) = std::fs::read(&path) {
        if newer_than_app(&existing) {
            return Err(
                "Los ajustes son de una versión más nueva de la aplicación. Hay que actualizarla."
                    .into(),
            );
        }
    }
    std::fs::write(&path, rendered(stored)?)?;
    restrict(&path);
    Ok(())
}

/// What actually lands on disk: always stamped with the version that wrote it.
pub fn rendered(stored: &StoredSettings) -> Result<Vec<u8>, serde_json::Error> {
    let stamped = StoredSettings {
        version: SETTINGS_VERSION,
        ..stored.clone()
    };
    serde_json::to_vec_pretty(&stamped)
}

/// The file holds two passwords, so it is readable by its owner and nobody else.
#[cfg(unix)]
fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path) {}

pub fn load(handle: &AppHandle) -> Result<Settings, Box<dyn std::error::Error>> {
    let stored = read(handle);
    Ok(assemble(
        &stored,
        default_destination(handle),
        handle.path().app_data_dir()?,
    ))
}

fn default_destination(handle: &AppHandle) -> PathBuf {
    handle
        .path()
        .video_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Mama Cine")
}

/// Stored fields into the value the rest of the app runs on. Pure, so the defaults are testable.
pub fn assemble(stored: &StoredSettings, fallback_films: PathBuf, state: PathBuf) -> Settings {
    Settings {
        indexers: stored
            .indexers
            .iter()
            .map(|indexer| IndexerSettings {
                name: non_empty(&indexer.name, &name_from(&indexer.url)),
                base_url: indexer.url.trim().to_string(),
                api_key: indexer.key.trim().to_string(),
                enabled: indexer.enabled,
            })
            .collect(),
        news: news_of(stored),
        subtitles: SubtitleSettings {
            api_key: stored.subtitles_key.clone(),
            user_agent: non_empty(&stored.subtitles_agent, "mamacine v1.0"),
            username: stored.subtitles_user.clone(),
            password: stored.subtitles_password.clone(),
            language: "es".into(),
            api_base: None,
        },
        state,
        destination: stored.destination.clone().unwrap_or(fallback_films),
    }
}

pub fn news_of(stored: &StoredSettings) -> NewsServer {
    NewsServer {
        host: stored.news_host.clone(),
        port: if stored.news_port == 0 {
            563
        } else {
            stored.news_port
        },
        username: stored.news_user.clone(),
        password: stored.news_password.clone(),
        encrypted: stored.news_encrypted,
        connections: if stored.news_connections == 0 {
            8
        } else {
            stored.news_connections
        },
        retention_days: 0,
    }
}

pub fn preference_of(stored: &StoredSettings) -> mamacine_core::release::Preference {
    match stored.language.as_str() {
        "es" => mamacine_core::release::Preference::Spanish,
        "original" => mamacine_core::release::Preference::Original,
        _ => mamacine_core::release::Preference::Any,
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

/// On Windows these travel with the app; in development they come from the shell.
pub fn tools(handle: &AppHandle) -> Tools {
    let places: Vec<PathBuf> = [
        handle.path().resource_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from)),
    ]
    .into_iter()
    .flatten()
    .collect();
    Tools {
        nzbget: beside_the_app(&places, "nzbget"),
        unrar: beside_the_app(&places, "unrar"),
        sevenzip: beside_the_app(&places, "7za"),
    }
}

/// The name a bundled program is filed under is the name plus this platform's suffix, and looking
/// for it without one found nothing on Windows: every tool fell back to a bare name, which only
/// ever worked because Windows happens to search the folder the app is running from. What that
/// hid is that a missing tool looked exactly like a present one.
fn beside_the_app(places: &[PathBuf], name: &str) -> PathBuf {
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    places
        .iter()
        .map(|directory| directory.join(&filename))
        .find(|path| path.exists())
        // in development they come from the shell, and PATH is the right place to look
        .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundled_program_is_found_under_the_name_this_platform_files_it_as() {
        let directory = std::env::temp_dir().join("mama-cine-tools-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let filename = format!("nzbget{}", std::env::consts::EXE_SUFFIX);
        std::fs::write(directory.join(&filename), b"").expect("a program to find");

        let places = vec![directory.clone()];
        assert_eq!(
            beside_the_app(&places, "nzbget"),
            directory.join(&filename),
            "the one that travels with the app wins over whatever is on the PATH"
        );
    }

    #[test]
    fn a_program_that_travels_with_nothing_is_left_to_the_path() {
        let places = vec![std::env::temp_dir().join("mama-cine-nothing-here")];
        assert_eq!(
            beside_the_app(&places, "nzbget"),
            PathBuf::from("nzbget"),
            "development runs it from the shell"
        );
    }

    // The window's input fields produce strings, serde's as_u64 answers None for a string, and
    // for weeks editing the port silently did nothing. Every numeric field must take both.
    #[test]
    fn a_port_typed_into_a_text_field_still_counts() {
        let mut stored = StoredSettings::default();
        apply(&mut stored, &serde_json::json!({ "news_port": "119" }));
        assert_eq!(stored.news_port, 119);
        apply(&mut stored, &serde_json::json!({ "news_port": 563 }));
        assert_eq!(stored.news_port, 563);
    }

    #[test]
    fn the_connections_field_is_saved_rather_than_dropped() {
        let mut stored = StoredSettings::default();
        apply(
            &mut stored,
            &serde_json::json!({ "news_connections": "12" }),
        );
        assert_eq!(stored.news_connections, 12);
    }

    #[test]
    fn a_blank_password_field_means_leave_it_alone() {
        let mut stored = StoredSettings {
            news_password: "kept".into(),
            ..StoredSettings::default()
        };
        apply(&mut stored, &serde_json::json!({ "news_password": "" }));
        assert_eq!(stored.news_password, "kept");
        apply(
            &mut stored,
            &serde_json::json!({ "news_password": "nueva" }),
        );
        assert_eq!(stored.news_password, "nueva");
    }

    #[test]
    fn what_the_window_did_not_mention_is_not_touched() {
        let mut stored = StoredSettings {
            news_host: "news.eweka.nl".into(),
            news_encrypted: false,
            ..StoredSettings::default()
        };
        apply(&mut stored, &serde_json::json!({ "news_user": "reader" }));
        assert_eq!(stored.news_host, "news.eweka.nl");
        assert!(!stored.news_encrypted, "absent fields must not reset");
    }

    #[test]
    fn her_language_is_a_setting_not_a_chip() {
        let mut stored = StoredSettings::default();
        assert_eq!(stored.language, "any", "works out of the box");
        apply(&mut stored, &serde_json::json!({ "language": "es" }));
        assert!(matches!(
            preference_of(&stored),
            mamacine_core::release::Preference::Spanish
        ));
    }

    #[test]
    fn encryption_is_on_unless_somebody_turns_it_off() {
        let stored = StoredSettings::default();
        assert!(news_of(&stored).encrypted);
        let mut stored = StoredSettings::default();
        apply(&mut stored, &serde_json::json!({ "news_encrypted": false }));
        assert!(!news_of(&stored).encrypted);
    }

    // The app will be updated again: what it writes says which version wrote it, and a file
    // from a newer version is recognised so nothing ever half-reads it and writes it back.
    #[test]
    fn what_is_written_is_stamped_and_a_newer_file_is_recognised() {
        let rendered = rendered(&StoredSettings::default()).expect("serializable");
        assert!(String::from_utf8_lossy(&rendered).contains("\"version\": 2"));
        assert!(
            !newer_than_app(&rendered),
            "what we write is never newer than us"
        );

        assert!(newer_than_app(br#"{"version": 3}"#));
        assert!(!newer_than_app(br#"{"version": 2}"#));
        assert!(!newer_than_app(
            br#"{"news_host": "old file without a version"}"#
        ));
        assert!(!newer_than_app(b"garbage"));
    }

    // A download she started must not die because she tidied the window away: keeping going is
    // the default, and both switches travel through the same tolerant apply as everything else.
    #[test]
    fn the_tray_and_autostart_switches_are_stored_and_default_sensibly() {
        let stored = StoredSettings::default();
        assert!(
            stored.keep_running,
            "closing the window keeps downloads going"
        );
        assert!(
            !stored.autostart,
            "starting with the computer is opted into"
        );

        let mut stored = StoredSettings::default();
        apply(
            &mut stored,
            &serde_json::json!({ "autostart": true, "keep_running": false }),
        );
        assert!(stored.autostart);
        assert!(!stored.keep_running);
    }

    #[test]
    fn sensible_defaults_stand_in_for_what_nobody_typed() {
        let news = news_of(&StoredSettings::default());
        assert_eq!(news.port, 563);
        assert_eq!(news.connections, 8);
    }
}
