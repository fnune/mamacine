//! The composition root. This is the only file that knows which concrete implementation is which,
//! and the only file that reads configuration from disk. Every decision lives in `orchestrator`,
//! where it can be tested; a command here is a lookup and a delegation, nothing more.

mod disk;
mod finishing;
mod library;
mod log;
mod messages;
mod orchestrator;
mod settings_file;
mod supervisor;

use finishing::Finisher;
use library::Library;
use log::Log;
use mamacine_core::clock::SystemClock;
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

/// Everything that is rebuilt when the settings change: the private nzbget, the clients that
/// carry credentials, and the orchestrator that ties them together. The library and the log
/// live outside it, so nothing she has is forgotten by pressing Guardar.
struct Runtime {
    orchestrator: Arc<Orchestrator>,
    finisher: Arc<Finisher>,
    nzbget: Mutex<Nzbget>,
}

pub struct App {
    runtime: RwLock<Option<Runtime>>,
    /// Why there is no runtime, in her words, when there is none.
    problem: RwLock<Option<String>>,
    library: Arc<Library>,
    log: Arc<Log>,
    network: Network,
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
            .unwrap_or_else(|| "La aplicación no ha terminado de arrancar.".to_string())
    }
}

/// Anything that waits on the network, a process or the disk belongs off the main thread. A
/// synchronous command runs on it, and the window cannot draw while one is in progress: a search
/// would freeze the very spinner meant to cover it, and the once-a-second progress poll would
/// stutter every scroll.
async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|failure| failure.to_string())?
}

// --- searching -------------------------------------------------------------------

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

/// What a tapped suggestion means: resolved here because only the provider knows how to turn its
/// own id into a query the indexer can answer.
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

// --- downloading -----------------------------------------------------------------

#[tauri::command]
async fn grab(
    app: State<'_, Arc<App>>,
    index: usize,
    version: Option<usize>,
    series: bool,
) -> Result<Grabbed, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.grab(index, version, series)).await
}

#[tauri::command]
async fn progress(app: State<'_, Arc<App>>) -> Result<Progress, String> {
    let app = app.inner().clone();
    off_thread(move || match app.orchestrator() {
        Ok(orchestrator) => Ok(orchestrator.progress()),
        // no downloader, but her films are still hers, and the reason is still sayable
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
        }),
    })
    .await
}

/// The give-up screen's button: carry on with the copies kept beyond the chase limit.
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

// --- her films -------------------------------------------------------------------

#[tauri::command]
async fn play(app: State<'_, Arc<App>>, id: i64) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let film = app.orchestrator()?.film_file(id)?;
        open_with_desktop(&film.display().to_string())
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
        open_with_desktop(&episode.display().to_string())
    })
    .await
}

#[tauri::command]
async fn reveal(app: State<'_, Arc<App>>, id: i64) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let folder = app.orchestrator()?.folder_of(id)?;
        open_with_desktop(&folder.display().to_string())
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

/// What the film is about, for the ficha. An empty answer means the film database has no words
/// for it, or there is no database configured; the screen stands without it either way.
#[tauri::command]
async fn synopsis(app: State<'_, Arc<App>>, index: usize) -> Result<String, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.synopsis(index)).await
}

/// The same words for something already on her shelf, which has no place in the search results
/// to be asked about.
#[tauri::command]
async fn library_synopsis(app: State<'_, Arc<App>>, id: i64) -> Result<String, String> {
    let app = app.inner().clone();
    off_thread(move || app.orchestrator()?.library_synopsis(id)).await
}

/// Built here from the film's own id rather than taken from the window, so the only page this can
/// ever open is the one for a film in the results.
#[tauri::command]
async fn open_imdb(app: State<'_, Arc<App>>, index: usize) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let id = app.orchestrator()?.imdb_of(index)?;
        let id = id.trim_start_matches('0');
        open_with_desktop(&format!("https://www.imdb.com/title/tt{id:0>7}/"))
    })
    .await
}

/// A season has no page of its own; the show's episode list, opened at that season, is the nearest
/// thing IMDb keeps.
#[tauri::command]
async fn open_imdb_season(app: State<'_, Arc<App>>, index: usize) -> Result<(), String> {
    let app = app.inner().clone();
    off_thread(move || {
        let (id, season) = app.orchestrator()?.imdb_season_of(index)?;
        open_with_desktop(&format!(
            "https://www.imdb.com/title/{id}/episodes?season={season}"
        ))
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

// --- settings --------------------------------------------------------------------

/// Never sends a password back to the window: it says whether one is set, not what it is.
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
    autostart: bool,
    keep_running: bool,
    ready: bool,
    /// Where all of this lives, so the screen can name the file it offers to open.
    settings_path: String,
}

fn view_of(handle: &tauri::AppHandle, stored: &settings_file::StoredSettings) -> SettingsView {
    SettingsView {
        settings_path: settings_file::path(handle)
            .map(|path| path.display().to_string())
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
        autostart: stored.autostart,
        keep_running: stored.keep_running,
    }
}

/// Asks the desktop for a folder. Typing a path is a thing to get wrong, and she would.
///
/// Asynchronous on purpose: a synchronous command runs on the main thread, and the blocking form of
/// this dialog waits for the main loop to show it. The app hangs waiting for itself.
#[tauri::command]
async fn choose_folder(handle: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;

    let (sender, receiver) = std::sync::mpsc::channel();
    handle
        .dialog()
        .file()
        .set_title("Dónde guardar las películas")
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

/// Opens the settings file itself, for the one person who will ever want it: whoever set the app
/// up for her. Written out first when it is not there yet, because the first run has nothing on
/// disk and "no pasa nada" is the worst answer a button can give.
#[tauri::command]
async fn open_settings_file(handle: tauri::AppHandle) -> Result<(), String> {
    off_thread(move || {
        let path = settings_file::path(&handle).map_err(|failure| failure.to_string())?;
        if !path.exists() {
            let stored = settings_file::read(&handle);
            settings_file::write(&handle, &stored).map_err(|failure| failure.to_string())?;
        }
        open_with_desktop(&path.display().to_string())
    })
    .await
}

/// Writes the file and then rebuilds the running app on it, because settings that only take
/// effect after a restart are settings that lie: the screen used to say "ya se puede buscar"
/// while the running app still held the old credentials.
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
            // saying "guardado" while the app failed to start on these settings would be a lie
            if let Some(problem) = app.problem.read().expect("not poisoned").clone() {
                return Err(format!("Se ha guardado, pero hay un problema: {problem}"));
            }
        }
        Ok(view_of(&handle, &stored))
    })
    .await
}

/// Checks the values as typed, not whatever happened to be loaded at startup: the old check
/// validated the credentials that had just been replaced.
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
        let mut lines = Vec::new();

        let usable: Vec<&settings_file::StoredIndexer> = stored
            .indexers
            .iter()
            .filter(|indexer| indexer.enabled && !indexer.url.trim().is_empty())
            .collect();
        if usable.is_empty() {
            lines.push("Buscadores: no hay ninguno configurado.".to_string());
        }
        for indexer in usable {
            let client = Newznab::new(
                mamacine_core::IndexerSettings {
                    name: indexer.name.clone(),
                    base_url: indexer.url.trim().to_string(),
                    api_key: indexer.key.trim().to_string(),
                    enabled: true,
                },
                Network::new(),
                SystemClock,
            );
            let name = if indexer.name.is_empty() {
                "Buscador"
            } else {
                &indexer.name
            };
            match client.capabilities() {
                Ok(_) => lines.push(format!("{name}: funciona.")),
                Err(failure) => {
                    lines.push(format!("{name}: {}", messages::explain(&failure).said))
                }
            }
        }

        let news = settings_file::news_of(&stored);
        if news.host.trim().is_empty() {
            lines.push("Servidor de descargas: falta la dirección.".to_string());
        } else {
            match app.orchestrator() {
                Ok(orchestrator) => match orchestrator.downloader.check_server(&news) {
                    ServerCheck::Working => {
                        lines.push("Servidor de descargas: funciona.".to_string())
                    }
                    ServerCheck::Refused(reason) => {
                        app.log.line(&format!("check news server: {reason}"));
                        lines.push(
                            "Servidor de descargas: ha rechazado el usuario o la contraseña."
                                .to_string(),
                        );
                    }
                    ServerCheck::Unreachable(reason) => {
                        app.log.line(&format!("check news server: {reason}"));
                        lines.push(
                            "Servidor de descargas: no se puede conectar. Revisa la dirección, o puede que no haya internet."
                                .to_string(),
                        );
                    }
                    ServerCheck::Unknown => lines.push(
                        "Servidor de descargas: no se ha podido comprobar ahora mismo.".to_string(),
                    ),
                },
                Err(problem) => lines.push(format!("Servidor de descargas: {problem}")),
            }
        }

        if !stored.tmdb_key.trim().is_empty() {
            let tmdb = Tmdb::new(
                stored.tmdb_key.trim().to_string(),
                "es-ES".into(),
                Network::new(),
            );
            match tmdb.check() {
                Ok(()) => lines.push("Fichas de películas: funciona.".to_string()),
                Err(failure) => lines.push(format!(
                    "Fichas de películas: {}",
                    messages::explain(&failure).said
                )),
            }
        }

        let subtitles = settings_file::assemble(
            &stored,
            std::path::PathBuf::from("."),
            std::path::PathBuf::from("."),
        )
        .subtitles;
        if subtitles.can_download() {
            let service = OpenSubtitles::new(subtitles, Network::new(), SystemClock);
            match service.check_account() {
                Ok(()) => lines.push("Subtítulos: funciona.".to_string()),
                Err(failure) => {
                    lines.push(format!("Subtítulos: {}", messages::explain(&failure).said))
                }
            }
        } else if subtitles.can_search() {
            lines.push(
                "Subtítulos: falta el usuario o la contraseña, así que no se pueden descargar."
                    .to_string(),
            );
        } else {
            lines.push("Subtítulos: sin configurar. La aplicación funciona igual.".to_string());
        }

        Ok(lines.join("\n"))
    })
    .await
}

// --- the desktop -----------------------------------------------------------------

/// Hands the path to the desktop and stays long enough to hear a refusal. The openers exit
/// almost at once, naming a code when no handler could be launched; swallowing that code was the
/// app's one silent catch, and "nothing at all happens" was how it looked from the sofa. A
/// handler that is still running after the grace period is a player doing its job.
fn open_with_desktop(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        // explorer rather than `cmd /C start`: cmd re-parses its line, and a folder with `&` in
        // its name would cut the command in half
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
    for _ in 0..40 {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!(
                    "El ordenador no ha podido abrirlo (error {}).",
                    status.code().unwrap_or(-1)
                ))
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(failure) => return Err(failure.to_string()),
        }
    }
    Ok(())
}

// --- the tray and the desktop's own switches ---------------------------------------

/// The app lives by the clock: the window is a view of it, not the app itself. Closing the view
/// keeps the downloads going when she asked for that, and this icon is the way back in.
fn build_tray(handle: &tauri::AppHandle) -> tauri::Result<tauri::menu::MenuItem<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    // a tray tooltip is a Windows idea and Linux ignores it, so the same status is also the top
    // line of the menu, where every desktop shows it
    let status = MenuItem::with_id(handle, "status", "Mamá Cine", false, None::<&str>)?;
    let open = MenuItem::with_id(handle, "open", "Abrir Mamá Cine", true, None::<&str>)?;
    // honest about the cost: leaving entirely is what stops a download halfway
    let quit = MenuItem::with_id(handle, "quit", "Salir del todo", true, None::<&str>)?;
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

/// Tells the operating system whether to open the app with the computer. The OS entry is derived
/// state: re-asserted from the setting on every start and every save, never migrated.
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

// --- building the runtime --------------------------------------------------------

fn build_runtime(handle: &tauri::AppHandle, app: &App) -> Result<Runtime, String> {
    let stored = settings_file::read(handle);
    let settings = settings_file::load(handle)
        .map_err(|failure| messages::explain(&failure_of(failure)).said)?;
    let tools = settings_file::tools(handle);

    let nzbget = Nzbget::start(&settings, &tools, &app.network)
        .map_err(|failure| messages::explain(&failure).said)?;
    let downloader = NzbgetRpc::new(nzbget.port, &nzbget.password, Network::new());
    let indexers: Vec<(String, Box<dyn Indexer>)> = settings
        .indexers
        .iter()
        .filter(|indexer| indexer.usable())
        .map(|indexer| {
            (
                indexer.name.clone(),
                Box::new(Newznab::new(indexer.clone(), Network::new(), SystemClock))
                    as Box<dyn Indexer>,
            )
        })
        .collect();

    // what she has is what is on the disk; anything remembered otherwise is corrected here
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

    let finisher = Arc::new(Finisher {
        downloader: Box::new(NzbgetRpc::new(
            nzbget.port,
            &nzbget.password,
            Network::new(),
        )),
        subtitles: Box::new(OpenSubtitles::new(
            settings.subtitles.clone(),
            Network::new(),
            SystemClock,
        )),
        library: Arc::clone(&app.library),
        log: Arc::clone(&app.log),
        language: settings.subtitles.language.clone(),
        notify: notify(handle.clone()),
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
        // TMDB when a key is set: titles in her language, the original named outright.
        // The keyless IMDb lookup otherwise, so the app works out of the box.
        suggestions: if stored.tmdb_key.trim().is_empty() {
            Box::new(orchestrator::Keyless::new(
                Lookup::new(Network::new()),
                TvMaze::new(Network::new()),
            ))
        } else {
            Box::new(Tmdb::new(
                stored.tmdb_key.trim().to_string(),
                // the language the interface itself speaks
                "es-ES".into(),
                Network::new(),
            ))
        },
        notify: notify(handle.clone()),
    }));

    Ok(Runtime {
        orchestrator,
        finisher,
        nzbget: Mutex::new(nzbget),
    })
}

fn failure_of(failure: Box<dyn std::error::Error>) -> mamacine_core::error::Error {
    mamacine_core::error::Error::Setup(failure.to_string())
}

/// Tears the old runtime down and builds a new one on the settings as they now are.
fn rebuild(handle: &tauri::AppHandle, app: &App) {
    let old = app.runtime.write().expect("not poisoned").take();
    if let Some(runtime) = old {
        runtime
            .nzbget
            .lock()
            .expect("not poisoned")
            .stop(&app.network);
    }
    // her records could not be used (a downgrade): downloading against a library that may not
    // remember what arrives would quietly orphan films, so the reason is surfaced instead
    if let Some(problem) = app.library.problem() {
        app.log.line(&format!("runtime not built: {problem}"));
        *app.problem.write().expect("not poisoned") = Some(problem);
        return;
    }
    match build_runtime(handle, app) {
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
        // registered first, as its docs require: a second launch would race a second nzbget onto
        // the same queue directory
        .plugin(tauri_plugin_single_instance::init(|handle, _args, _cwd| {
            // the window may be tucked into the tray: launching again means "show it to me"
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
            let library = Arc::new(Library::open(&state, Arc::clone(&log)));
            let app = Arc::new(App {
                runtime: RwLock::new(None),
                problem: RwLock::new(None),
                library,
                log,
                network: Network::new(),
            });

            // built off the main thread: nzbget takes seconds to answer, and a window that
            // appears late looks like an app that did not start
            let building = Arc::clone(&app);
            let build_handle = handle.clone();
            std::thread::spawn(move || {
                rebuild(&build_handle, &building);

                // a copy that turns out to be dead is replaced by the next one without her doing
                // anything: on usenet the first copy failing is ordinary, not an error to hand over
                let chasing = Arc::clone(&building);
                std::thread::spawn(move || loop {
                    if let Ok(orchestrator) = chasing.orchestrator() {
                        orchestrator.chase();
                    }
                    std::thread::sleep(std::time::Duration::from_secs(4));
                });
                // the finishing work runs on its own thread so a slow probe never stalls the window
                let finishing = building;
                std::thread::spawn(move || loop {
                    if let Ok(finisher) = finishing.finisher() {
                        finisher.sweep();
                    }
                    std::thread::sleep(std::time::Duration::from_secs(5));
                });
            });

            handle.manage(Arc::clone(&app));
            let status = build_tray(&handle)?;

            // the tray answers "how is it going?" without opening the window
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
                    // closing the view is not stopping the work: the app tucks itself in by
                    // the clock, and says so when something is actually still coming down
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
                                .title("Mamá Cine sigue descargando")
                                .body("Se queda en el icono pequeño junto al reloj y avisará cuando la película esté lista.")
                                .show();
                        }
                    });
                }
            }
            tauri::WindowEvent::Destroyed => {
                if let Some(app) = window.try_state::<Arc<App>>() {
                    // leaving nzbget running would hold the port and keep downloading unseen
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
            try_more,
            cancel,
            play,
            episodes,
            play_episode,
            reveal,
            remove_film,
            fetch_subtitles,
            cover,
            synopsis,
            library_synopsis,
            open_imdb,
            choose_folder,
            read_settings,
            open_settings_file,
            save_settings,
            check_settings
        ])
        .run(tauri::generate_context!())
        .expect("the application starts");
}
