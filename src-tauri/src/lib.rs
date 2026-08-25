mod disk;
mod finishing;
mod library;
mod log;
mod messages;
mod orchestrator;
mod settings_file;
mod supervisor;
mod text;
mod updater;

use finishing::Finisher;
use library::Library;
use log::Log;
use mamacine_core::clock::SystemClock;
use mamacine_core::http::Throttle;
use mamacine_core::indexer::{Indexer, Newznab};
use mamacine_core::lookup::{Lookup, Suggestion};
use mamacine_core::net::Network;
use mamacine_core::nzbget::{NzbgetRpc, ServerCheck};
use mamacine_core::opensubtitles::OpenSubtitles;
use mamacine_core::tmdb::Tmdb;
use mamacine_core::tvmaze::TvMaze;
use orchestrator::{Found, Grabbed, Orchestrator, Pieces, Progress, Version};
use serde::Serialize;
use std::sync::{Arc, Mutex, RwLock};
use supervisor::Nzbget;
use tauri::{Manager, State};
use tauri_plugin_notification::NotificationExt;
use text::Lang;
use updater::Plan;

struct Runtime {
    orchestrator: Arc<Orchestrator>,
    finisher: Arc<Finisher>,
    nzbget: Mutex<Nzbget>,
    subtitles: Arc<OpenSubtitles<Throttle<Network>, SystemClock>>,
    subtitle_settings: mamacine_core::settings::SubtitleSettings,
}

fn polite(floor_ms: u64) -> Throttle<Network> {
    Throttle::new(Network::new(), std::time::Duration::from_millis(floor_ms))
}

pub struct App {
    runtime: RwLock<Option<Runtime>>,
    problem: RwLock<Option<String>>,
    library: Arc<Library>,
    log: Arc<Log>,
    network: Network,
    lang: RwLock<Lang>,
    update: RwLock<Option<updater::Pending>>,
}

impl App {
    fn orchestrator(&self) -> Result<Arc<Orchestrator>, String> {
        self.runtime
            .read()
            .expect("not poisoned")
            .as_ref()
            .map(|runtime| Arc::clone(&runtime.orchestrator))
            .ok_or_else(|| self.said_problem())
    }

    fn finisher(&self) -> Result<Arc<Finisher>, String> {
        self.runtime
            .read()
            .expect("not poisoned")
            .as_ref()
            .map(|runtime| Arc::clone(&runtime.finisher))
            .ok_or_else(|| self.said_problem())
    }

    fn said_problem(&self) -> String {
        self.problem
            .read()
            .expect("not poisoned")
            .clone()
            .unwrap_or_else(|| self.lang().still_starting().to_string())
    }

    fn lang(&self) -> Lang {
        *self.lang.read().expect("not poisoned")
    }
}

async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|failure| failure.to_string())?
}

#[tauri::command]
async fn search(
    app: State<'_, Arc<App>>,
    query: String,
    kind: Option<String>,
    shown: Option<String>,
) -> Result<Found, String> {
    let app = app.inner().clone();
    off_thread(move || {
        app.orchestrator()?
            .search(&query, kind.as_deref(), shown.as_deref())
    })
    .await
}

#[tauri::command]
async fn pick_suggestion(
    app: State<'_, Arc<App>>,
    index: usize,
) -> Result<mamacine_core::lookup::Picked, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.pick(index)).await
}

#[tauri::command]
async fn suggest(app: State<'_, Arc<App>>, text: String) -> Result<Vec<Suggestion>, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.suggest(&text)).await
}

#[tauri::command]
async fn versions(
    app: State<'_, Arc<App>>,
    index: usize,
    series: bool,
) -> Result<Vec<Version>, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.versions(index, series)).await
}

#[tauri::command]
async fn have(
    app: State<'_, Arc<App>>,
    index: usize,
    series: bool,
) -> Result<orchestrator::Owned, String> {
    let app = app.inner().clone();
    off_thread(move || Ok(app.orchestrator()?.have(index, series))).await
}

#[tauri::command]
async fn grab(
    app: State<'_, Arc<App>>,
    index: usize,
    version: Option<usize>,
    series: bool,
    replacing: Option<i64>,
) -> Result<Grabbed, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.grab(index, version, series, replacing)).await
}

#[tauri::command]
async fn copies(app: State<'_, Arc<App>>, id: i64) -> Result<orchestrator::Copies, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.copies_of(id)).await
}

#[tauri::command]
async fn progress(app: State<'_, Arc<App>>) -> Result<Progress, String> {
    let app = app.inner().clone();
    off_thread(move || match app.orchestrator() {
        Ok(orchestrator) => {
            let mut progress = orchestrator.progress();
            progress.update = app
                .update
                .read()
                .expect("not poisoned")
                .as_ref()
                .map(|pending| pending.news.clone());
            Ok(progress)
        }
        Err(problem) => Ok(Progress {
            active: Vec::new(),
            finished: Vec::new(),
            shelf: app
                .library
                .all()
                .into_iter()
                .filter(|(_, entry)| entry.present())
                .map(|(id, entry)| orchestrator::Shelved {
                    id,
                    title: entry.title,
                    year: entry.year,
                    cover_url: entry.cover_url,
                    subtitle_note: entry.subtitle_note,
                    languages: entry.info,
                    series: entry.series,
                })
                .collect(),
            free_space: String::new(),
            free_bytes: 0,
            total_space: String::new(),
            total_bytes: 0,
            problem: Some(problem),
            update: app
                .update
                .read()
                .expect("not poisoned")
                .as_ref()
                .map(|pending| pending.news.clone()),
        }),
    })
    .await
}

#[tauri::command]
async fn try_more(app: State<'_, Arc<App>>, id: i64) -> Result<Grabbed, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.try_more(id)).await
}

#[tauri::command]
async fn cancel(app: State<'_, Arc<App>>, id: i64) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.cancel(id)).await
}

#[tauri::command]
async fn play(app: State<'_, Arc<App>>, id: i64) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let film = app.orchestrator()?.film_file(id)?;
        open_with_desktop(&film.display().to_string(), app.lang())
    })
    .await
}

#[tauri::command]
async fn episodes(
    app: State<'_, Arc<App>>,
    id: i64,
) -> Result<Vec<orchestrator::EpisodeRow>, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.episodes(id)).await
}

#[tauri::command]
async fn play_episode(app: State<'_, Arc<App>>, id: i64, position: usize) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let episode = app.orchestrator()?.episode_file(id, position)?;
        open_with_desktop(&episode.display().to_string(), app.lang())
    })
    .await
}

#[tauri::command]
async fn reveal(app: State<'_, Arc<App>>, id: i64) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let folder = app.orchestrator()?.folder_of(id)?;
        open_with_desktop(&folder.display().to_string(), app.lang())
    })
    .await
}

#[tauri::command]
async fn remove_film(app: State<'_, Arc<App>>, id: i64) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.remove(id)).await
}

#[tauri::command]
async fn fetch_subtitles(app: State<'_, Arc<App>>, id: i64) -> Result<String, String> {
    let app = app.inner().clone();
    off_thread(move || app.finisher()?.refetch_subtitles(id)).await
}

#[tauri::command]
async fn cover(app: State<'_, Arc<App>>, url: String) -> Result<String, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.image(&url)).await
}

#[tauri::command]
async fn synopsis(app: State<'_, Arc<App>>, index: usize) -> Result<String, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.synopsis(index)).await
}

#[tauri::command]
async fn library_synopsis(app: State<'_, Arc<App>>, id: i64) -> Result<String, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.library_synopsis(id)).await
}

#[tauri::command]
async fn open_imdb(app: State<'_, Arc<App>>, index: usize) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let id = app.orchestrator()?.imdb_of(index)?;
        let id = id.trim_start_matches('0');
        open_with_desktop(
            &format!("https://www.imdb.com/title/tt{id:0>7}/"),
            app.lang(),
        )
    })
    .await
}

#[tauri::command]
async fn open_imdb_season(app: State<'_, Arc<App>>, index: usize) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let (id, season) = app.orchestrator()?.imdb_season_of(index)?;
        open_with_desktop(
            &format!("https://www.imdb.com/title/{id}/episodes?season={season}"),
            app.lang(),
        )
    })
    .await
}

#[tauri::command]
async fn season_episodes(
    app: State<'_, Arc<App>>,
    index: usize,
) -> Result<Vec<mamacine_core::series::Episode>, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.season_episodes(index)).await
}

#[derive(Serialize)]
pub struct SettingsView {
    indexers: Vec<settings_file::StoredIndexer>,
    news_host: String,
    news_port: u16,
    news_user: String,
    news_password_set: bool,
    news_connections: u8,
    news_encrypted: bool,
    tmdb_key: String,
    subtitles_key: String,
    subtitles_agent: String,
    subtitles_user: String,
    subtitles_password_set: bool,
    destination: String,
    language: String,
    ui_language: String,
    app_language: &'static str,
    autostart: bool,
    keep_running: bool,
    ready: bool,
    settings_path: String,
    log_path: String,
}

fn view_of(handle: &tauri::AppHandle, stored: &settings_file::StoredSettings) -> SettingsView {
    SettingsView {
        settings_path: settings_file::path(handle)
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        log_path: handle
            .path()
            .app_data_dir()
            .map(|directory| Log::path_in(&directory).display().to_string())
            .unwrap_or_default(),
        ready: stored.indexers.iter().any(|indexer| {
            indexer.enabled && !indexer.key.trim().is_empty() && !indexer.url.trim().is_empty()
        }) && !stored.news_host.trim().is_empty(),
        indexers: stored.indexers.clone(),
        news_host: stored.news_host.clone(),
        news_port: settings_file::news_of(stored).port,
        news_user: stored.news_user.clone(),
        news_password_set: !stored.news_password.is_empty(),
        news_connections: settings_file::news_of(stored).connections,
        news_encrypted: stored.news_encrypted,
        tmdb_key: stored.tmdb_key.clone(),
        subtitles_key: stored.subtitles_key.clone(),
        subtitles_agent: stored.subtitles_agent.clone(),
        subtitles_user: stored.subtitles_user.clone(),
        subtitles_password_set: !stored.subtitles_password.is_empty(),
        destination: stored
            .destination
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        language: stored.language.clone(),
        ui_language: stored.ui_language.clone(),
        app_language: settings_file::ui_language_of(stored).code(),
        autostart: stored.autostart,
        keep_running: stored.keep_running,
    }
}

#[tauri::command]
async fn choose_folder(handle: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;

    let lang = settings_file::ui_language_of(&settings_file::read(&handle));

    let (sender, receiver) = std::sync::mpsc::channel();
    handle
        .dialog()
        .file()
        .set_title(lang.where_to_save_films())
        .pick_folder(move |folder| {
            let _ = sender.send(folder);
        });

    tauri::async_runtime::spawn_blocking(move || receiver.recv().ok().flatten())
        .await
        .ok()
        .flatten()
        .and_then(|folder| folder.into_path().ok())
        .map(|folder| folder.display().to_string())
}

#[tauri::command]
fn read_settings(handle: tauri::AppHandle) -> SettingsView {
    let stored = settings_file::read(&handle);
    view_of(&handle, &stored)
}

#[tauri::command]
async fn open_settings_file(handle: tauri::AppHandle) -> Result<(), String> {
    off_thread(move || {
        let path = settings_file::path(&handle).map_err(|failure| failure.to_string())?;
        if !path.exists() {
            let stored = settings_file::read(&handle);
            settings_file::write(&handle, &stored).map_err(|failure| failure.to_string())?;
        }
        let lang = settings_file::ui_language_of(&settings_file::read(&handle));
        open_with_desktop(&path.display().to_string(), lang)
    })
    .await
}

#[tauri::command]
async fn open_log_file(app: State<'_, Arc<App>>) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        app.log.line("the log was opened from the settings screen");
        open_with_desktop(&app.log.path().display().to_string(), app.lang())
    })
    .await
}

#[tauri::command]
async fn open_log_folder(app: State<'_, Arc<App>>) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || open_with_desktop(&app.log.folder().display().to_string(), app.lang())).await
}

#[tauri::command]
async fn save_settings(
    handle: tauri::AppHandle,
    app: State<'_, Arc<App>>,
    incoming: serde_json::Value,
) -> Result<SettingsView, String> {
    let app = app.inner().clone();
    off_thread(move || {
        let before = settings_file::read(&handle);
        let mut stored = before.clone();
        settings_file::apply(&mut stored, &incoming);
        settings_file::write(&handle, &stored).map_err(|failure| failure.to_string())?;
        sync_autostart(&handle, &app, stored.autostart);
        if stored != before {
            rebuild(&handle, &app);
            if let Some(problem) = app.problem.read().expect("not poisoned").clone() {
                return Err(app.lang().saved_but(&problem));
            }
        }
        Ok(view_of(&handle, &stored))
    })
    .await
}

#[tauri::command]
async fn check_settings(
    handle: tauri::AppHandle,
    app: State<'_, Arc<App>>,
    incoming: serde_json::Value,
) -> Result<String, String> {
    let app = app.inner().clone();
    off_thread(move || {
        let mut stored = settings_file::read(&handle);
        settings_file::apply(&mut stored, &incoming);
        let lang = settings_file::ui_language_of(&stored);
        let mut lines = Vec::new();

        let usable: Vec<&settings_file::StoredIndexer> = stored
            .indexers
            .iter()
            .filter(|indexer| indexer.enabled && !indexer.url.trim().is_empty())
            .collect();
        if usable.is_empty() {
            lines.push(lang.check_no_indexer().to_string());
        }
        for indexer in usable {
            let client = Newznab::new(
                mamacine_core::IndexerSettings {
                    name: indexer.name.clone(),
                    base_url: indexer.url.trim().to_string(),
                    api_key: indexer.key.trim().to_string(),
                    enabled: true,
                },
                polite(500),
                SystemClock,
            );
            let name = if indexer.name.is_empty() {
                lang.check_indexer_fallback_name()
            } else {
                &indexer.name
            };
            match client.capabilities() {
                Ok(_) => lines.push(lang.check_works(name)),
                Err(failure) => lines.push(format!(
                    "{name}: {}",
                    messages::explain(&failure, lang).said
                )),
            }
        }

        let news = settings_file::news_of(&stored);
        if news.host.trim().is_empty() {
            lines.push(lang.check_news_missing_host().to_string());
        } else {
            match app.orchestrator() {
                Ok(orchestrator) => match orchestrator.downloader.check_server(&news) {
                    ServerCheck::Working => lines.push(lang.check_news_works().to_string()),
                    ServerCheck::Refused(reason) => {
                        app.log.line(&format!("check news server: {reason}"));
                        lines.push(lang.check_news_refused().to_string());
                    }
                    ServerCheck::Unreachable(reason) => {
                        app.log.line(&format!("check news server: {reason}"));
                        lines.push(lang.check_news_unreachable().to_string());
                    }
                    ServerCheck::Unknown => lines.push(lang.check_news_unknown().to_string()),
                },
                Err(problem) => lines.push(lang.check_news_prefix(&problem)),
            }
        }

        if !stored.tmdb_key.trim().is_empty() {
            let tmdb = Tmdb::new(
                stored.tmdb_key.trim().to_string(),
                settings_file::tmdb_language_of(&stored, &settings_file::system_language().1),
                polite(250),
            );
            match tmdb.check() {
                Ok(()) => lines.push(lang.check_metadata_works().to_string()),
                Err(failure) => {
                    lines.push(lang.check_metadata_prefix(&messages::explain(&failure, lang).said))
                }
            }
        }

        let subtitles = settings_file::assemble(
            &stored,
            std::path::PathBuf::from("."),
            std::path::PathBuf::from("."),
            &settings_file::system_language().0,
        )
        .subtitles;
        if subtitles.can_download() {
            let running = app
                .runtime
                .read()
                .expect("not poisoned")
                .as_ref()
                .filter(|runtime| runtime.subtitle_settings == subtitles)
                .map(|runtime| Arc::clone(&runtime.subtitles));
            let checked = match running {
                Some(service) => service.check_account(),
                None => OpenSubtitles::new(subtitles, polite(500), SystemClock).check_account(),
            };
            match checked {
                Ok(()) => lines.push(lang.check_subtitles_works().to_string()),
                Err(failure) => {
                    lines.push(lang.check_subtitles_prefix(&messages::explain(&failure, lang).said))
                }
            }
        } else if subtitles.can_search() {
            lines.push(lang.check_subtitles_no_account().to_string());
        } else {
            lines.push(lang.check_subtitles_unconfigured().to_string());
        }

        Ok(lines.join("\n"))
    })
    .await
}

fn open_with_desktop(path: &str, lang: Lang) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer");
        command.arg(path);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    let mut child = command.spawn().map_err(|failure| failure.to_string())?;
    if cfg!(target_os = "windows") {
        return Ok(());
    }
    for _ in 0..40 {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(lang.could_not_open_it(status.code().unwrap_or(-1))),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(failure) => return Err(failure.to_string()),
        }
    }
    Ok(())
}

const UPDATE_REPO: &str = "fnune/mamacine";

/// Once a day: what GitHub Releases has, and what to do about it. An AppImage takes the new
/// copy by itself; anywhere else the window offers a button, and either way she is told once.
fn check_for_update(handle: &tauri::AppHandle, app: &App, running: &str) {
    use tauri_plugin_notification::NotificationExt;

    let lang = app.lang();
    let api = mamacine_core::updates::GithubReleases::new(UPDATE_REPO, polite(1000));
    let downloads = mamacine_core::updates::GithubReleases::new(UPDATE_REPO, Network::patient());
    let appimage = updater::running_appimage();
    let Some(announcement) = updater::check(
        &api,
        &downloads,
        running,
        appimage.as_deref(),
        cfg!(windows),
        &app.update,
        &app.log,
    ) else {
        return;
    };
    let (title, body) = if announcement.installed {
        (
            lang.update_installed_title(&announcement.version),
            lang.update_installed_body(),
        )
    } else {
        (
            lang.update_available_title(&announcement.version),
            lang.update_available_body(),
        )
    };
    let _ = handle
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

/// The button on the update banner: run the installer, or open the release page.
#[tauri::command]
async fn open_update(handle: tauri::AppHandle, app: State<'_, Arc<App>>) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let plan = app
            .update
            .read()
            .expect("not poisoned")
            .as_ref()
            .map(|pending| pending.plan.clone());
        match plan {
            Some(Plan::Open { url, .. }) => open_with_desktop(&url, app.lang()),
            Some(Plan::RunInstaller {
                installer_url,
                checksums_url,
                ..
            }) => {
                let into = handle
                    .path()
                    .app_data_dir()
                    .map_err(|failure| failure.to_string())?;
                let patient =
                    mamacine_core::updates::GithubReleases::new(UPDATE_REPO, Network::patient());
                let installer = updater::fetch_installer(
                    &patient,
                    &installer_url,
                    checksums_url.as_deref(),
                    &into,
                )
                .map_err(|failure| messages::explain(&failure, app.lang()).said)?;
                std::process::Command::new(&installer)
                    .spawn()
                    .map_err(|failure| failure.to_string())?;
                Ok(())
            }
            _ => Ok(()),
        }
    })
    .await
}

fn build_tray(handle: &tauri::AppHandle) -> tauri::Result<tauri::menu::MenuItem<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let status = MenuItem::with_id(handle, "status", "Mamá Cine", false, None::<&str>)?;
    let lang = settings_file::ui_language_of(&settings_file::read(handle));
    let open = MenuItem::with_id(handle, "open", lang.open_the_app(), true, None::<&str>)?;
    let quit = MenuItem::with_id(handle, "quit", lang.quit_entirely(), true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(handle)?;
    let menu = Menu::with_items(handle, &[&status, &separator, &open, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(
            handle
                .default_window_icon()
                .expect("the bundled icon")
                .clone(),
        )
        .tooltip("Mamá Cine")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|handle, event| match event.id.as_ref() {
            "open" => show_window(handle),
            "quit" => quit_entirely(handle),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(handle)?;
    Ok(status)
}

fn show_window(handle: &tauri::AppHandle) {
    if let Some(window) = handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn quit_entirely(handle: &tauri::AppHandle) {
    if let Some(app) = handle.try_state::<Arc<App>>() {
        if let Some(runtime) = app.runtime.write().expect("not poisoned").take() {
            runtime
                .nzbget
                .lock()
                .expect("not poisoned")
                .stop(&app.network);
        }
    }
    handle.exit(0);
}

fn sync_autostart(handle: &tauri::AppHandle, app: &App, wanted: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = handle.autolaunch();
    let already = autolaunch.is_enabled().unwrap_or(false);
    if wanted == already {
        return;
    }
    let outcome = if wanted {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    if let Err(failure) = outcome {
        app.log
            .line(&format!("autostart could not be updated: {failure}"));
    }
}

fn build_runtime(
    handle: &tauri::AppHandle,
    app: &App,
    previous: Option<&Runtime>,
) -> Result<Runtime, String> {
    let stored = settings_file::read(handle);
    let lang = settings_file::ui_language_of(&stored);
    *app.lang.write().expect("not poisoned") = lang;
    let settings = settings_file::load(handle)
        .map_err(|failure| messages::explain(&failure_of(failure), lang).said)?;
    let tools = settings_file::tools(handle);

    let nzbget =
        Nzbget::start(&settings, &tools, &app.network, &app.log, lang).map_err(|failure| {
            let explained = messages::explain(&failure, lang);
            app.log.line(&explained.why);
            explained.said
        })?;
    let downloader = NzbgetRpc::new(nzbget.port, &nzbget.password, Network::new());
    let indexers: Vec<(String, Box<dyn Indexer>)> = settings
        .indexers
        .iter()
        .filter(|indexer| indexer.usable())
        .map(|indexer| {
            (
                indexer.name.clone(),
                Box::new(Newznab::new(indexer.clone(), polite(500), SystemClock))
                    as Box<dyn Indexer>,
            )
        })
        .collect();

    app.library.reconcile(&settings.destination);

    let notify = |handle: tauri::AppHandle| -> orchestrator::Notify {
        Box::new(move |title: &str, body: &str| {
            let _ = handle
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show();
        })
    };

    let subtitles = match previous {
        Some(old) if old.subtitle_settings == settings.subtitles => Arc::clone(&old.subtitles),
        _ => Arc::new(OpenSubtitles::new(
            settings.subtitles.clone(),
            polite(500),
            SystemClock,
        )),
    };

    let finisher = Arc::new(Finisher {
        downloader: Box::new(NzbgetRpc::new(
            nzbget.port,
            &nzbget.password,
            Network::new(),
        )),
        subtitles: subtitles.clone(),
        library: Arc::clone(&app.library),
        log: Arc::clone(&app.log),
        language: settings.subtitles.language.clone(),
        remover: Arc::new(orchestrator::SystemRemover),
        notify: notify(handle.clone()),
        lang,
    });

    let orchestrator = Arc::new(Orchestrator::new(Pieces {
        indexers,
        downloader: Box::new(downloader),
        library: Arc::clone(&app.library),
        log: Arc::clone(&app.log),
        destination: settings.destination.clone(),
        news: settings.news.clone(),
        preference: settings_file::preference_of(&stored),
        subtitle_language: settings.subtitles.language.clone(),
        disk: Box::new(orchestrator::SystemDisk),
        remover: Box::new(orchestrator::SystemRemover),
        prober: Box::new(mamacine_core::nntp::NntpProbe),
        suggestions: if stored.tmdb_key.trim().is_empty() {
            Box::new(orchestrator::Keyless::new(
                Lookup::new(polite(500)),
                TvMaze::new(polite(500)),
            ))
        } else {
            Box::new(Tmdb::new(
                stored.tmdb_key.trim().to_string(),
                settings_file::tmdb_language_of(&stored, &settings_file::system_language().1),
                polite(250),
            ))
        },
        notify: notify(handle.clone()),
        lang,
    }));

    Ok(Runtime {
        orchestrator,
        finisher,
        nzbget: Mutex::new(nzbget),
        subtitles,
        subtitle_settings: settings.subtitles.clone(),
    })
}

fn failure_of(failure: Box<dyn std::error::Error>) -> mamacine_core::error::Error {
    mamacine_core::error::Error::Setup(failure.to_string())
}

fn rebuild(handle: &tauri::AppHandle, app: &App) {
    let old = app.runtime.write().expect("not poisoned").take();
    if let Some(runtime) = &old {
        runtime
            .nzbget
            .lock()
            .expect("not poisoned")
            .stop(&app.network);
    }
    if let Some(problem) = app.library.problem() {
        app.log.line(&format!("runtime not built: {problem}"));
        *app.problem.write().expect("not poisoned") = Some(problem);
        return;
    }
    match build_runtime(handle, app, old.as_ref()) {
        Ok(runtime) => {
            *app.runtime.write().expect("not poisoned") = Some(runtime);
            *app.problem.write().expect("not poisoned") = None;
        }
        Err(problem) => {
            app.log.line(&format!("runtime rebuild failed: {problem}"));
            *app.problem.write().expect("not poisoned") = Some(problem);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|handle, _args, _cwd| {
            show_window(handle);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|tauri_app| {
            let handle = tauri_app.handle().clone();
            let state = handle.path().app_data_dir()?;
            std::fs::create_dir_all(&state)?;
            let log = Arc::new(Log::open(&state));
            log.line(&format!(
                "Mamá Cine {} starting, its files in {}",
                tauri_app.package_info().version,
                state.display()
            ));
            let panics = Arc::clone(&log);
            let previously = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |panic| {
                panics.line(&format!("panic: {panic}"));
                previously(panic);
            }));
            let lang = settings_file::ui_language_of(&settings_file::read(&handle));
            let library = Arc::new(Library::open(&state, Arc::clone(&log), lang));
            let app = Arc::new(App {
                runtime: RwLock::new(None),
                problem: RwLock::new(None),
                library,
                log,
                network: Network::new(),
                lang: RwLock::new(lang),
                update: RwLock::new(None),
            });

            let building = Arc::clone(&app);
            let build_handle = handle.clone();
            std::thread::spawn(move || {
                rebuild(&build_handle, &building);

                let chasing = Arc::clone(&building);
                std::thread::spawn(move || loop {
                    if let Ok(orchestrator) = chasing.orchestrator() {
                        orchestrator.chase();
                    }
                    std::thread::sleep(std::time::Duration::from_secs(4));
                });
                let finishing = building;
                std::thread::spawn(move || loop {
                    if let Ok(finisher) = finishing.finisher() {
                        finisher.sweep();
                    }
                    std::thread::sleep(std::time::Duration::from_secs(5));
                });
            });

            let updates = Arc::clone(&app);
            let update_handle = handle.clone();
            let running = tauri_app.package_info().version.to_string();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(90));
                check_for_update(&update_handle, &updates, &running);
                std::thread::sleep(std::time::Duration::from_secs(24 * 3600 - 90));
            });

            handle.manage(Arc::clone(&app));
            let status = build_tray(&handle)?;

            let tray_app = Arc::clone(&app);
            let tray_handle = handle.clone();
            std::thread::spawn(move || loop {
                if let (Some(tray), Ok(orchestrator)) =
                    (tray_handle.tray_by_id("main"), tray_app.orchestrator())
                {
                    let report = orchestrator.tray_report();
                    let _ = tray.set_tooltip(Some(&report.tooltip));
                    let _ = status.set_text(&report.summary);
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
            });
            sync_autostart(&handle, &app, settings_file::read(&handle).autostart);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let handle = window.app_handle().clone();
                if settings_file::read(&handle).keep_running {
                    api.prevent_close();
                    let _ = window.hide();
                    std::thread::spawn(move || {
                        let Some(app) = handle.try_state::<Arc<App>>() else {
                            return;
                        };
                        let Ok(orchestrator) = app.orchestrator() else {
                            return;
                        };
                        let busy = orchestrator
                            .downloader
                            .queue()
                            .map(|queue| !queue.is_empty())
                            .unwrap_or(false);
                        if busy {
                            let _ = handle
                                .notification()
                                .builder()
                                .title(app.lang().keeps_downloading_title())
                                .body(app.lang().keeps_downloading_body())
                                .show();
                        }
                    });
                }
            }
            tauri::WindowEvent::Destroyed => {
                if let Some(app) = window.try_state::<Arc<App>>() {
                    if let Some(runtime) = app.runtime.write().expect("not poisoned").take() {
                        runtime
                            .nzbget
                            .lock()
                            .expect("not poisoned")
                            .stop(&app.network);
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            search,
            suggest,
            pick_suggestion,
            season_episodes,
            open_imdb_season,
            versions,
            have,
            grab,
            progress,
            open_update,
            try_more,
            cancel,
            play,
            episodes,
            play_episode,
            reveal,
            remove_film,
            copies,
            fetch_subtitles,
            cover,
            synopsis,
            library_synopsis,
            open_imdb,
            choose_folder,
            read_settings,
            open_settings_file,
            open_log_file,
            open_log_folder,
            save_settings,
            check_settings
        ])
        .run(tauri::generate_context!())
        .expect("the application starts");
}
