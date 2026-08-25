use mamacine_core::nzbget::Tools;
use mamacine_core::settings::{IndexerSettings, NewsServer, Settings, SubtitleSettings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct StoredIndexer {
    pub name: String,
    pub url: String,
    pub key: String,
    pub enabled: bool,
}

pub const SETTINGS_VERSION: u32 = 2;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct StoredSettings {
    pub version: u32,
    pub indexers: Vec<StoredIndexer>,
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
    pub language: String,
    pub subtitles_language: String,
    pub tmdb_language: String,
    /// The language the interface speaks ("es", "en"). Empty means the computer's.
    pub ui_language: String,
    pub autostart: bool,
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
            news_encrypted: true,
            news_connections: 0,
            tmdb_key: String::new(),
            subtitles_key: String::new(),
            subtitles_agent: String::new(),
            subtitles_user: String::new(),
            subtitles_password: String::new(),
            destination: None,
            language: "any".into(),
            subtitles_language: String::new(),
            tmdb_language: String::new(),
            ui_language: String::new(),
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
        return StoredSettings::default();
    };
    match serde_json::from_slice::<StoredSettings>(&bytes) {
        Ok(mut stored) => {
            migrate(&mut stored);
            stored
        }
        Err(_) => {
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

pub fn newer_than_app(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| value.get("version").and_then(serde_json::Value::as_u64))
        .map(|version| version > u64::from(SETTINGS_VERSION))
        .unwrap_or(false)
}

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

fn name_from(url: &str) -> String {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(|host| host.trim_start_matches("api.").to_string())
        .unwrap_or_else(|| "Buscador".to_string())
}

pub fn apply(stored: &mut StoredSettings, incoming: &serde_json::Value) {
    let text = |field: &str| {
        incoming
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .map(str::to_string)
    };
    let number = |field: &str| {
        incoming.get(field).and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        })
    };

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
    if let Some(value) = text("subtitles_language").filter(|value| !value.is_empty()) {
        stored.subtitles_language = value;
    }
    if let Some(value) = text("tmdb_language").filter(|value| !value.is_empty()) {
        stored.tmdb_language = value;
    }
    if let Some(value) = text("ui_language") {
        stored.ui_language = value;
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
    if let Ok(existing) = std::fs::read(&path) {
        if newer_than_app(&existing) {
            return Err(ui_language_of(stored).settings_from_a_newer_app().into());
        }
    }
    std::fs::write(&path, rendered(stored)?)?;
    restrict(&path);
    Ok(())
}

pub fn rendered(stored: &StoredSettings) -> Result<Vec<u8>, serde_json::Error> {
    let stamped = StoredSettings {
        version: SETTINGS_VERSION,
        ..stored.clone()
    };
    serde_json::to_vec_pretty(&stamped)
}

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
        &system_language().0,
    ))
}

pub fn system_language() -> (String, String) {
    locale_of_this_computer()
        .as_deref()
        .and_then(language_from_locale)
        .unwrap_or_else(|| ("es".into(), "es-ES".into()))
}

#[cfg(unix)]
fn locale_of_this_computer() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
}

#[cfg(windows)]
fn locale_of_this_computer() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserDefaultLocaleName, LOCALE_NAME_MAX_LENGTH};
    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH as usize];
    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if written <= 1 {
        return None;
    }
    String::from_utf16(&buffer[..written as usize - 1]).ok()
}

fn language_from_locale(locale: &str) -> Option<(String, String)> {
    let base = locale.split('.').next()?.trim().replace('_', "-");
    let mut parts = base.split('-');
    let code = parts.next()?.to_lowercase();
    if code.len() != 2 || !code.chars().all(|letter| letter.is_ascii_alphabetic()) {
        return None;
    }
    let locale = match parts.next().map(str::to_uppercase) {
        Some(region) if region.len() == 2 => format!("{code}-{region}"),
        _ => code.clone(),
    };
    Some((code, locale))
}

fn default_destination(handle: &AppHandle) -> PathBuf {
    handle
        .path()
        .video_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Mama Cine")
}

pub fn assemble(
    stored: &StoredSettings,
    fallback_films: PathBuf,
    state: PathBuf,
    fallback_language: &str,
) -> Settings {
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
            language: non_empty(&stored.subtitles_language, fallback_language),
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

pub fn tmdb_language_of(stored: &StoredSettings, fallback: &str) -> String {
    non_empty(&stored.tmdb_language, fallback)
}

/// Which language the interface speaks: the setting, else the computer's, else English,
/// except that an unset language on a Spanish computer stays Spanish.
pub fn ui_language_of(stored: &StoredSettings) -> crate::text::Lang {
    resolve_ui_language(&stored.ui_language, &system_language().0)
}

fn resolve_ui_language(setting: &str, system_code: &str) -> crate::text::Lang {
    crate::text::Lang::from_code(setting)
        .or_else(|| crate::text::Lang::from_code(system_code))
        .unwrap_or(crate::text::Lang::En)
}

pub fn preference_of(stored: &StoredSettings) -> mamacine_core::release::Preference {
    match stored.language.as_str() {
        "original" => mamacine_core::release::Preference::Original,
        "any" => mamacine_core::release::Preference::Any,
        code => mamacine_core::release::known_language(code)
            .map(mamacine_core::release::Preference::Language)
            .unwrap_or(mamacine_core::release::Preference::Any),
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

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

fn beside_the_app(places: &[PathBuf], name: &str) -> PathBuf {
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    places
        .iter()
        .map(|directory| directory.join(&filename))
        .find(|path| path.exists())
        .map(as_windows_writes_it)
        .unwrap_or_else(|| PathBuf::from(name))
}

fn as_windows_writes_it(path: PathBuf) -> PathBuf {
    let Some(rest) = path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) else {
        return path;
    };
    let letters = rest.as_bytes();
    let names_a_drive = letters.len() >= 3
        && letters[0].is_ascii_alphabetic()
        && letters[1] == b':'
        && letters[2] == b'\\';
    if names_a_drive {
        return PathBuf::from(rest);
    }
    path
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
    fn a_drive_letter_is_written_the_way_the_rest_of_windows_writes_it() {
        assert_eq!(
            as_windows_writes_it(PathBuf::from(r"\\?\C:\Users\María Esther\unrar.exe")),
            PathBuf::from(r"C:\Users\María Esther\unrar.exe")
        );
        assert_eq!(
            as_windows_writes_it(PathBuf::from(r"C:\Users\María Esther\unrar.exe")),
            PathBuf::from(r"C:\Users\María Esther\unrar.exe")
        );
        assert_eq!(
            as_windows_writes_it(PathBuf::from(r"\\?\UNC\casa\peliculas\unrar.exe")),
            PathBuf::from(r"\\?\UNC\casa\peliculas\unrar.exe"),
            "a share named this way is left alone: the prefix is the only thing holding it up"
        );
        assert_eq!(
            as_windows_writes_it(PathBuf::from("/usr/bin/unrar")),
            PathBuf::from("/usr/bin/unrar")
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
            mamacine_core::release::Preference::Language("es")
        ));
        apply(&mut stored, &serde_json::json!({ "language": "fr" }));
        assert!(matches!(
            preference_of(&stored),
            mamacine_core::release::Preference::Language("fr")
        ));
        apply(&mut stored, &serde_json::json!({ "language": "xx" }));
        assert!(
            matches!(
                preference_of(&stored),
                mamacine_core::release::Preference::Any
            ),
            "a code nobody recognises falls back to the setting that works everywhere"
        );
    }

    #[test]
    fn the_fetched_languages_follow_the_computer_until_somebody_chooses() {
        let stored = StoredSettings::default();
        let settings = assemble(&stored, PathBuf::from("."), PathBuf::from("."), "de");
        assert_eq!(settings.subtitles.language, "de");
        assert_eq!(tmdb_language_of(&stored, "de-DE"), "de-DE");

        let mut stored = StoredSettings::default();
        apply(
            &mut stored,
            &serde_json::json!({ "subtitles_language": "fr", "tmdb_language": "fr-FR" }),
        );
        let settings = assemble(&stored, PathBuf::from("."), PathBuf::from("."), "de");
        assert_eq!(settings.subtitles.language, "fr");
        assert_eq!(tmdb_language_of(&stored, "de-DE"), "fr-FR");
    }

    #[test]
    fn a_settings_file_from_before_the_language_fields_follows_the_computer_too() {
        let stored: StoredSettings =
            serde_json::from_str(r#"{"version": 2, "news_host": "news.test"}"#).expect("readable");
        let settings = assemble(&stored, PathBuf::from("."), PathBuf::from("."), "es");
        assert_eq!(settings.subtitles.language, "es");
    }

    #[test]
    fn the_interface_language_follows_the_setting_then_the_computer_then_english() {
        use crate::text::Lang;
        assert_eq!(resolve_ui_language("", "es"), Lang::Es);
        assert_eq!(resolve_ui_language("", "en"), Lang::En);
        assert_eq!(
            resolve_ui_language("", "fr"),
            Lang::En,
            "unsupported computers read English"
        );
        assert_eq!(
            resolve_ui_language("es", "fr"),
            Lang::Es,
            "the setting wins"
        );
        assert_eq!(resolve_ui_language("en", "es"), Lang::En);
    }

    #[test]
    fn a_locale_is_read_in_the_spellings_computers_actually_use() {
        assert_eq!(
            language_from_locale("es_ES.UTF-8"),
            Some(("es".into(), "es-ES".into()))
        );
        assert_eq!(
            language_from_locale("fr-FR"),
            Some(("fr".into(), "fr-FR".into()))
        );
        assert_eq!(language_from_locale("de"), Some(("de".into(), "de".into())));
        assert_eq!(language_from_locale("C"), None, "no language named");
        assert_eq!(language_from_locale("POSIX"), None);
        assert_eq!(language_from_locale(""), None);
    }

    #[test]
    fn encryption_is_on_unless_somebody_turns_it_off() {
        let stored = StoredSettings::default();
        assert!(news_of(&stored).encrypted);
        let mut stored = StoredSettings::default();
        apply(&mut stored, &serde_json::json!({ "news_encrypted": false }));
        assert!(!news_of(&stored).encrypted);
    }

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
