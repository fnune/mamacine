use crate::log::Log;
use crate::text::Lang;
use mamacine_core::error::{Error, Result};
use mamacine_core::http::HttpClient;
use mamacine_core::nzbget::{render_config, NzbgetRpc, Tools};
use mamacine_core::settings::Settings;
use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CONFIG_NAME: &str = "nzbget.conf";

pub struct Nzbget {
    process: Option<Child>,
    pidfile: std::path::PathBuf,
    pub port: u16,
    pub password: String,
}

impl Nzbget {
    pub fn start<H: HttpClient>(
        settings: &Settings,
        tools: &Tools,
        http: &H,
        log: &Arc<Log>,
        lang: Lang,
    ) -> Result<Nzbget> {
        let work = settings.state.join("nzbget");
        for directory in ["inter", "nzb", "queue", "tmp", "scripts"] {
            std::fs::create_dir_all(work.join(directory))?;
        }
        std::fs::create_dir_all(&settings.destination)?;

        let pidfile = work.join("nzbget.pid");
        reclaim_orphan(&pidfile);

        let port = free_port()?;
        let password = secret();
        let config_path = work.join(CONFIG_NAME);
        write_private(
            &config_path,
            render_config(settings, &work, port, &password, tools).as_bytes(),
        )?;

        let program = spelled_without_accents(&tools.nzbget, log);
        log.line(&format!(
            "starting {} (exists: {}) with {} on port {port}",
            program.display(),
            program.exists(),
            config_path.display()
        ));
        log.line(&format!(
            "unrar is {} (exists: {}), 7za is {} (exists: {})",
            tools.unrar.display(),
            tools.unrar.exists(),
            tools.sevenzip.display(),
            tools.sevenzip.exists()
        ));

        let mut command = Command::new(&program);
        without_the_appimage_surroundings(&mut command);
        command
            .current_dir(&work)
            .arg("-c")
            .arg(CONFIG_NAME)
            .arg("-s")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        off_the_screen(&mut command);
        let mut process = command.spawn().map_err(|failure| {
            log.line(&format!(
                "nzbget could not be started ({}): {failure}",
                program.display()
            ));
            Error::Setup(lang.downloader_would_not_start())
        })?;

        let ready = Arc::new(AtomicBool::new(false));
        forward(process.stdout.take(), log, &ready);
        forward(process.stderr.take(), log, &ready);

        let _ = std::fs::write(&pidfile, process.id().to_string());
        let mut started = Nzbget {
            process: Some(process),
            pidfile,
            port,
            password,
        };

        let rpc = NzbgetRpc::new(started.port, &started.password, http);
        let deadline = Instant::now() + Duration::from_secs(25);
        let mut ended = false;
        while Instant::now() < deadline {
            if rpc.is_ready() {
                ready.store(true, Ordering::Relaxed);
                log.line("nzbget is answering");
                return Ok(started);
            }
            match started.ended() {
                Some(status) if !status.success() => {
                    log.line(&format!("nzbget refused to run ({status})"));
                    report_own_log(&work, log);
                    return Err(Error::Setup(lang.downloader_closed_at_once()));
                }
                Some(status) if !ended => {
                    ended = true;
                    log.line(&format!(
                        "the nzbget we launched ended ({status}); still waiting for an answer"
                    ));
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        log.line("nzbget was still not answering after 25 seconds");
        report_own_log(&work, log);
        Err(Error::Setup(lang.downloader_never_answered()))
    }

    fn ended(&mut self) -> Option<std::process::ExitStatus> {
        self.process.as_mut()?.try_wait().ok().flatten()
    }

    pub fn stop<H: HttpClient>(&mut self, http: &H) {
        let rpc = NzbgetRpc::new(self.port, &self.password, http);
        let _ = rpc.shutdown();
        if let Some(mut process) = self.process.take() {
            for _ in 0..10 {
                match process.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                    Err(_) => break,
                }
            }
            let _ = process.kill();
        }
        let _ = std::fs::remove_file(&self.pidfile);
    }
}

fn report_own_log(work: &std::path::Path, log: &Arc<Log>) {
    let path = work.join("nzbget.log");
    let Ok(text) = std::fs::read_to_string(&path) else {
        log.line(&format!(
            "nzbget never wrote {}, so it stopped before it opened its own log",
            path.display()
        ));
        return;
    };
    for line in last_lines(&text, 30) {
        log.from("nzbget.log", line);
    }
}

fn last_lines(text: &str, wanted: usize) -> Vec<&str> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(wanted)..].to_vec()
}

fn forward(
    stream: Option<impl std::io::Read + Send + 'static>,
    log: &Arc<Log>,
    ready: &Arc<AtomicBool>,
) {
    let Some(stream) = stream else {
        return;
    };
    let log = Arc::clone(log);
    let ready = Arc::clone(ready);
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stream)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if !ready.load(Ordering::Relaxed) || is_trouble(&line) {
                log.from("nzbget", line.trim());
            }
        }
    });
}

fn is_trouble(line: &str) -> bool {
    line.contains("ERROR") || line.contains("WARNING") || line.contains("FATAL")
}

fn reclaim_orphan(pidfile: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(pidfile) else {
        return;
    };
    if let Ok(pid) = text.trim().parse::<u32>() {
        if is_an_nzbget(pid) {
            terminate(pid);
        }
    }
    let _ = std::fs::remove_file(pidfile);
}

#[cfg(unix)]
fn is_an_nzbget(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|name| name.trim() == "nzbget")
        .unwrap_or(false)
}

#[cfg(unix)]
fn terminate(pid: u32) {
    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    std::thread::sleep(Duration::from_millis(500));
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
}

#[cfg(windows)]
fn is_an_nzbget(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        ok != 0
            && String::from_utf16_lossy(&buffer[..size as usize])
                .to_lowercase()
                .contains("nzbget")
    }
}

#[cfg(windows)]
fn terminate(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 0);
            CloseHandle(handle);
        }
    }
}

#[cfg(windows)]
fn off_the_screen(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn off_the_screen(_command: &mut Command) {}

fn spelled_without_accents(nzbget: &std::path::Path, log: &Arc<Log>) -> std::path::PathBuf {
    if !cfg!(windows) || is_plain_ascii(nzbget) {
        return nzbget.to_path_buf();
    }
    let directory = a_folder_nobody_is_named_in();
    if !is_plain_ascii(&directory) {
        log.line(&format!(
            "{} has accents in it too, so nzbget will abort on {}",
            directory.display(),
            nzbget.display()
        ));
        return nzbget.to_path_buf();
    }
    let copy = directory.join(nzbget.file_name().unwrap_or("nzbget.exe".as_ref()));
    if is_the_same_program(&copy, nzbget) {
        return copy;
    }
    match std::fs::create_dir_all(&directory).and_then(|()| std::fs::copy(nzbget, &copy)) {
        Ok(_) => {
            log.line(&format!("nzbget copied to {}", copy.display()));
            copy
        }
        Err(failure) if copy.exists() => {
            log.line(&format!(
                "keeping the nzbget already in {} ({failure})",
                directory.display()
            ));
            copy
        }
        Err(failure) => {
            log.line(&format!(
                "nzbget could not be copied to {} ({failure}), so it will abort on {}",
                directory.display(),
                nzbget.display()
            ));
            nzbget.to_path_buf()
        }
    }
}

fn a_folder_nobody_is_named_in() -> std::path::PathBuf {
    std::env::var_os("ProgramData")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"))
        .join("com.fnune.mamacine")
        .join("bin")
}

fn is_plain_ascii(path: &std::path::Path) -> bool {
    path.as_os_str().to_str().is_some_and(str::is_ascii)
}

fn is_the_same_program(copy: &std::path::Path, source: &std::path::Path) -> bool {
    match (copy.metadata(), source.metadata()) {
        (Ok(there), Ok(here)) => {
            there.len() == here.len() && there.modified().ok() == here.modified().ok()
        }
        _ => false,
    }
}

/// A child of an AppImage inherits library paths into the mounted image, and a host program
/// started with those dies at once: xdg-open answered "error 4" the day this was found. Every
/// variable whose value points into the image is dropped for the child.
pub fn without_the_appimage_surroundings(command: &mut Command) {
    let Some(appdir) = std::env::var_os("APPDIR") else {
        return;
    };
    scrubbed_of(command, &appdir.to_string_lossy());
}

fn scrubbed_of(command: &mut Command, appdir: &str) {
    if appdir.trim().is_empty() {
        return;
    }
    for (name, value) in std::env::vars() {
        if value.contains(appdir) {
            command.env_remove(&name);
        }
    }
}

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn secret() -> String {
    let mut bytes = [0u8; 18];
    getrandom::fill(&mut bytes).expect("the system random source");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, contents: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reused_pid_that_is_not_an_nzbget_is_left_alive() {
        let directory = std::env::temp_dir().join("mama-cine-orphan-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let pidfile = directory.join("nzbget.pid");
        std::fs::write(&pidfile, std::process::id().to_string()).expect("a pidfile");

        reclaim_orphan(&pidfile);

        assert!(!pidfile.exists(), "the stale pidfile is cleaned up");
    }

    #[test]
    fn only_what_went_wrong_is_kept_once_nzbget_is_running() {
        assert!(is_trouble(
            "Wed Aug 19 2026 00:20:50 ERROR Could not bind socket"
        ));
        assert!(is_trouble(
            "Wed Aug 19 2026 00:20:50 WARNING Article download failed"
        ));
        assert!(!is_trouble(
            "Wed Aug 19 2026 00:20:50 INFO Download El Sur (1983) successful"
        ));
    }

    #[test]
    fn the_end_of_its_log_is_what_says_why_it_stopped() {
        let text = "one\ntwo\n\nthree\nfour\n";
        assert_eq!(last_lines(text, 2), vec!["three", "four"]);
        assert_eq!(
            last_lines(text, 90),
            vec!["one", "two", "three", "four"],
            "a log shorter than the tail is the whole log, not a panic"
        );
        assert!(last_lines("", 10).is_empty());
    }

    #[test]
    fn a_program_reached_through_a_name_with_an_accent_in_it_is_a_program_that_aborts() {
        assert!(!is_plain_ascii(std::path::Path::new(
            r"C:\Users\María Esther\AppData\Local\Mamá Cine\nzbget.exe"
        )));
        assert!(is_plain_ascii(std::path::Path::new(
            r"C:\ProgramData\com.fnune.mamacine\bin\nzbget.exe"
        )));
        assert!(
            is_plain_ascii(&a_folder_nobody_is_named_in()),
            "the folder we copy it to must be one nzbget can read: {}",
            a_folder_nobody_is_named_in().display()
        );
    }

    #[test]
    fn the_copy_is_made_once_and_then_recognised() {
        let directory = std::env::temp_dir().join("mama-cine-copy-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let source = directory.join("nzbget");
        std::fs::write(&source, b"a program").expect("a program");
        let copy = directory.join("copy");

        assert!(!is_the_same_program(&copy, &source));
        std::fs::copy(&source, &copy).expect("a copy");
        let when = source
            .metadata()
            .expect("a program")
            .modified()
            .expect("a time");
        std::fs::File::options()
            .write(true)
            .open(&copy)
            .expect("the copy")
            .set_modified(when)
            .expect("the time a copy carries");
        assert!(is_the_same_program(&copy, &source));

        std::fs::write(&source, b"a newer program").expect("an update");
        assert!(
            !is_the_same_program(&copy, &source),
            "an app update replaces nzbget, and the copy beside it is then the wrong one"
        );
    }

    #[test]
    fn nothing_is_moved_on_a_system_that_can_read_the_name_it_was_given() {
        if cfg!(windows) {
            return;
        }
        let directory = std::env::temp_dir().join("mama-cine-accent-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let nzbget = directory.join("María Esther").join("nzbget");
        let log = Arc::new(Log::open(&directory));
        assert_eq!(spelled_without_accents(&nzbget, &log), nzbget);
    }

    #[test]
    fn a_pidfile_naming_nobody_is_simply_removed() {
        let directory = std::env::temp_dir().join("mama-cine-orphan-dead");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let pidfile = directory.join("nzbget.pid");
        std::fs::write(&pidfile, "999999999").expect("a pidfile");
        reclaim_orphan(&pidfile);
        assert!(!pidfile.exists());
    }

    // --- the real thing ---------------------------------------------------------------
    //
    // A real nzbget, a real NNTP conversation, a real file on the disk: everything the app
    // drives, end to end, with nothing leaving this machine. The news server is this test,
    // listening on a local port.

    use mamacine_core::nzbget::Downloader;
    use mamacine_core::settings::{NewsServer, SubtitleSettings};
    use std::collections::HashMap;
    use std::io::BufReader;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn yenc_article(name: &str, data: &[u8]) -> Vec<u8> {
        let mut out = format!("=ybegin line=128 size={} name={name}\r\n", data.len()).into_bytes();
        let mut column = 0;
        for byte in data {
            if column >= 128 {
                out.extend(b"\r\n");
                column = 0;
            }
            let coded = byte.wrapping_add(42);
            if matches!(coded, 0 | b'\r' | b'\n' | b'=' | b'.') {
                out.push(b'=');
                out.push(coded.wrapping_add(64));
                column += 2;
            } else {
                out.push(coded);
                column += 1;
            }
        }
        out.extend(
            format!(
                "\r\n=yend size={} crc32={:08x}\r\n",
                data.len(),
                crc32(data)
            )
            .into_bytes(),
        );
        out
    }

    fn serve_articles(articles: HashMap<String, Vec<u8>>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let port = listener.local_addr().expect("an address").port();
        let articles = Arc::new(articles);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let articles = Arc::clone(&articles);
                std::thread::spawn(move || answer_nntp(stream, &articles));
            }
        });
        port
    }

    fn answer_nntp(mut stream: std::net::TcpStream, articles: &HashMap<String, Vec<u8>>) {
        let mut reader = BufReader::new(stream.try_clone().expect("a reader"));
        if stream.write_all(b"200 mamacine test server\r\n").is_err() {
            return;
        }
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let said = line.trim();
            let (verb, rest) = said.split_once(' ').unwrap_or((said, ""));
            let reply: Vec<u8> = match verb.to_uppercase().as_str() {
                "AUTHINFO" if rest.to_uppercase().starts_with("USER") => {
                    b"381 password please\r\n".to_vec()
                }
                "AUTHINFO" => b"281 welcome\r\n".to_vec(),
                "DATE" => b"111 20260825120000\r\n".to_vec(),
                "GROUP" => b"211 1 1 1 group\r\n".to_vec(),
                "STAT" => b"223 0 <x>\r\n".to_vec(),
                "ARTICLE" | "BODY" => {
                    let id = rest.trim().trim_start_matches('<').trim_end_matches('>');
                    match articles.get(id) {
                        Some(body) => {
                            let mut reply = if verb.eq_ignore_ascii_case("ARTICLE") {
                                format!("220 0 <{id}>\r\nMessage-ID: <{id}>\r\n\r\n").into_bytes()
                            } else {
                                format!("222 0 <{id}>\r\n").into_bytes()
                            };
                            reply.extend_from_slice(body);
                            reply.extend_from_slice(b".\r\n");
                            reply
                        }
                        None => b"430 no such article\r\n".to_vec(),
                    }
                }
                "QUIT" => {
                    let _ = stream.write_all(b"205 bye\r\n");
                    return;
                }
                _ => b"500 what?\r\n".to_vec(),
            };
            if stream.write_all(&reply).is_err() {
                return;
            }
        }
    }

    #[test]
    fn what_points_into_the_mounted_image_never_reaches_a_child() {
        std::env::set_var(
            "MAMACINE_TEST_POISONED",
            "/tmp/.mount_mamacine_test/usr/lib",
        );
        std::env::set_var("MAMACINE_TEST_CLEAN", "/usr/lib");
        let mut command = Command::new("true");
        scrubbed_of(&mut command, "/tmp/.mount_mamacine_test");
        let removed: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        assert!(
            removed.contains(&"MAMACINE_TEST_POISONED".to_string()),
            "{removed:?}"
        );
        assert!(
            !removed.contains(&"MAMACINE_TEST_CLEAN".to_string()),
            "{removed:?}"
        );
    }

    struct NoSubtitles;

    impl mamacine_core::opensubtitles::SubtitleSource for NoSubtitles {
        fn find(
            &self,
            _: &mamacine_core::opensubtitles::SubtitleQuery,
        ) -> mamacine_core::error::Result<Vec<mamacine_core::subtitles::Candidate>> {
            Ok(Vec::new())
        }
        fn download(&self, _: i64) -> mamacine_core::error::Result<Vec<u8>> {
            Ok(Vec::new())
        }
        fn downloads_remaining(&self) -> Option<i64> {
            None
        }
    }

    struct KeepEverything;

    impl crate::orchestrator::Remover for KeepEverything {
        fn remove(&self, _: &std::path::Path) -> std::result::Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn a_real_nzbget_downloads_a_real_post_without_leaving_this_machine() {
        if std::process::Command::new("nzbget")
            .arg("-v")
            .output()
            .is_err()
        {
            eprintln!("skipping: no nzbget on the PATH; the dev shell and CI both carry one");
            return;
        }

        let directory = std::env::temp_dir().join("mama-cine-real-nzbget");
        let _ = std::fs::remove_dir_all(&directory);
        let films = directory.join("films");
        let state = directory.join("state");
        std::fs::create_dir_all(&films).expect("a films folder");
        std::fs::create_dir_all(&state).expect("a state folder");

        let film: Vec<u8> = (0..96_000u32).map(|i| (i % 251) as u8).collect();
        let mut articles = HashMap::new();
        articles.insert(
            "una.pelicula.1@test".to_string(),
            yenc_article("Una.Pelicula.2020.mkv", &film),
        );
        let nntp_port = serve_articles(articles);

        let settings = Settings {
            indexers: Vec::new(),
            news: NewsServer {
                host: "127.0.0.1".into(),
                port: nntp_port,
                username: "reader".into(),
                password: "secret".into(),
                encrypted: false,
                connections: 2,
                retention_days: 0,
            },
            subtitles: SubtitleSettings {
                api_key: String::new(),
                user_agent: "mamacine test".into(),
                username: String::new(),
                password: String::new(),
                language: "es".into(),
                api_base: None,
            },
            destination: films,
            state,
        };
        let tools = Tools {
            nzbget: "nzbget".into(),
            unrar: "unrar".into(),
            sevenzip: "7za".into(),
        };
        let log = Arc::new(Log::open(&directory));
        let network = mamacine_core::net::Network::new();
        let mut nzbget = Nzbget::start(&settings, &tools, &network, &log, crate::text::Lang::Es)
            .expect("a real nzbget starts");

        let rpc = NzbgetRpc::new(
            nzbget.port,
            &nzbget.password,
            mamacine_core::net::Network::new(),
        );
        let nzb = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <nzb xmlns=\"http://www.newzbin.com/DTD/2003/nzb\">\n\
             <file poster=\"test@test\" date=\"1700000000\" \
             subject=\"&quot;Una.Pelicula.2020.mkv&quot; yEnc (1/1)\">\n\
             <groups><group>alt.binaries.test</group></groups>\n\
             <segments><segment bytes=\"{}\" number=\"1\">una.pelicula.1@test</segment></segments>\n\
             </file>\n\
             </nzb>\n",
            film.len()
        );
        let id = rpc
            .append("Una pelicula", nzb.as_bytes())
            .expect("appended");

        let deadline = Instant::now() + Duration::from_secs(90);
        let landed = loop {
            let finished = rpc
                .history()
                .unwrap_or_default()
                .into_iter()
                .find(|item| item.id == id);
            if let Some(item) = finished {
                if item.succeeded {
                    break item;
                }
                assert!(
                    Instant::now() < deadline,
                    "nzbget gave up on the post: {}",
                    item.status
                );
            }
            assert!(
                Instant::now() < deadline,
                "nzbget never finished; its log is beside {}",
                directory.display()
            );
            std::thread::sleep(Duration::from_millis(300));
        };

        let folder = std::path::PathBuf::from(landed.directory.clone().expect("a directory"));
        let video = crate::finishing::largest_video(&folder).expect("the film on the disk");
        assert_eq!(
            std::fs::read(&video).expect("the downloaded bytes"),
            film,
            "byte for byte what was posted"
        );

        let library = Arc::new(crate::library::Library::open(
            &directory,
            Arc::clone(&log),
            crate::text::Lang::Es,
        ));
        let finisher = crate::finishing::Finisher {
            downloader: Box::new(NzbgetRpc::new(
                nzbget.port,
                &nzbget.password,
                mamacine_core::net::Network::new(),
            )),
            subtitles: Arc::new(NoSubtitles),
            library: Arc::clone(&library),
            log: Arc::clone(&log),
            language: "es".into(),
            lang: crate::text::Lang::Es,
            remover: Arc::new(KeepEverything),
            notify: Box::new(|_, _| {}),
        };
        finisher.sweep();
        let entry = library.get(id).expect("a record of the film");
        assert!(entry.settled, "the finisher settled what nzbget landed");
        assert_eq!(entry.file.as_deref(), Some(video.as_path()));

        nzbget.stop(&network);
    }
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, contents: &[u8]) -> Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents)?;
    Ok(())
}
