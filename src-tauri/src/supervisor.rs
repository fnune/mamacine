//! Starting and stopping the private nzbget instance that does the downloading.

use crate::log::Log;
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

/// What she is told when the downloader will not run. The reason itself is technical every time,
/// so it goes to the log, and the sentence says where the log is rather than pretending to
/// explain: the person who can act on it is whoever set the app up.
const IN_THE_LOG: &str = "En Ajustes, «Abrir el registro» dice por qué.";

/// Never written in full on the command line. See where it is passed.
const CONFIG_NAME: &str = "nzbget.conf";

pub struct Nzbget {
    process: Option<Child>,
    pidfile: std::path::PathBuf,
    pub port: u16,
    pub password: String,
}

impl Nzbget {
    /// Writes a config only this instance uses, on a port only this instance knows.
    pub fn start<H: HttpClient>(
        settings: &Settings,
        tools: &Tools,
        http: &H,
        log: &Arc<Log>,
    ) -> Result<Nzbget> {
        let work = settings.state.join("nzbget");
        for directory in ["inter", "nzb", "queue", "tmp", "scripts"] {
            std::fs::create_dir_all(work.join(directory))?;
        }
        std::fs::create_dir_all(&settings.destination)?;

        // an app that crashed or was killed leaves its private nzbget running, downloading
        // unseen: the app being off must mean nothing is going on
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

        // named from inside its own folder rather than in full: nzbget takes the `-c` argument
        // from the C runtime, which spells it in the machine's code page, and then reads it as
        // UTF-8. A bare `nzbget.conf` has nothing in it that the two spellings disagree about,
        // and nzbget expands it against this directory through the wide interface, which does
        // carry the accents.
        let mut command = Command::new(&program);
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
            Error::Setup(format!(
                "No he podido arrancar el descargador. {IN_THE_LOG}"
            ))
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
                // an nzbget that refuses its own configuration is gone in a second: waiting out
                // the deadline to then blame the clock describes the wait, not the failure
                Some(status) if !status.success() => {
                    log.line(&format!("nzbget refused to run ({status})"));
                    report_own_log(&work, log);
                    return Err(Error::Setup(format!(
                        "El descargador se ha cerrado nada más arrancar. {IN_THE_LOG}"
                    )));
                }
                // leaving with nothing to complain about can mean it put a server behind it, so
                // the answer is still worth waiting for
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
        Err(Error::Setup(format!(
            "El descargador no ha arrancado. {IN_THE_LOG}"
        )))
    }

    fn ended(&mut self) -> Option<std::process::ExitStatus> {
        self.process.as_mut()?.try_wait().ok().flatten()
    }

    pub fn stop<H: HttpClient>(&mut self, http: &H) {
        let rpc = NzbgetRpc::new(self.port, &self.password, http);
        let _ = rpc.shutdown();
        if let Some(mut process) = self.process.take() {
            // two seconds of grace, not ten: this runs while the window is closing, and a quit
            // that visibly hangs reads as a crash
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

/// What nzbget wrote to its own log before it stopped, into ours.
///
/// A program that dies by abort() takes its unflushed output with it, and output to a pipe is
/// buffered where output to a console is not: that is exactly the run whose reason is worth
/// having, and piping it is what loses it. Its log file is written by nzbget itself, so it
/// survives what the pipe does not.
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

/// nzbget's console output, into our log, on a thread of its own: a pipe nobody reads fills up
/// and stops the program writing to it.
///
/// Everything it says until it answers, because that is where a refusal to start is printed and
/// nowhere else: its own log file is not open yet. Only trouble after that, because it narrates
/// every download, and a megabyte of narration would rotate away the app's own history to say
/// what its log file already says.
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

/// Kills the nzbget a crashed instance left behind, and only that: the pid is checked to still
/// belong to an nzbget before anything is sent a signal, because pids get reused.
fn reclaim_orphan(pidfile: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(pidfile) else {
        return; // a clean shutdown removed it
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
    // SAFETY: plain signals to a pid just verified to be an nzbget of ours
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
    // SAFETY: a query-only handle, closed on every path
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
    // SAFETY: a terminate-only handle for a pid just verified to be an nzbget, closed after
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 0);
            CloseHandle(handle);
        }
    }
}

/// Windows gives a console program a console, and nzbget is a console program: a black window
/// stood beside the app with nothing in it she could act on, and its close button was a close
/// button for the downloader. Everything it prints is read off the pipes either way, so the
/// console was never showing anything the log does not already have.
#[cfg(windows)]
fn off_the_screen(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn off_the_screen(_command: &mut Command) {}

/// A spelling of nzbget that nzbget itself can read.
///
/// On Windows it asks the system for its own file name through the narrow interface, which
/// answers in the machine's code page, and then reads that answer as UTF-8. Under a profile
/// named «María Esther» the í comes back as a single byte, UTF-8 reads that byte as the start
/// of a three-letter sequence that is not there, the conversion throws, nothing catches it, and
/// the process aborts with 0xC0000409. All of that happens before it opens its own log, which
/// is why every failed start left nothing behind to read. Only what is inside the config file
/// is safe, because that it reads as UTF-8 from the first byte: the destination and the working
/// folders keep her name.
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
        // the one already there is a copy of the one we were about to write, and a start that
        // works is worth more than a copy that is certainly of today's build
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

/// Nothing that is hers goes here: the config file carries her news password and stays in her
/// own folder, under her own account. Only the program is copied, and a program is public.
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

/// Windows copies the modification time along with the file, so the nzbget an app update has
/// since replaced does not answer to this and is copied again.
fn is_the_same_program(copy: &std::path::Path, source: &std::path::Path) -> bool {
    match (copy.metadata(), source.metadata()) {
        (Ok(there), Ok(here)) => {
            there.len() == here.len() && there.modified().ok() == here.modified().ok()
        }
        _ => false,
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

    // Pids get reused: a stale pidfile must never kill whatever innocent process holds the
    // number now. This test IS that innocent process.
    #[test]
    fn a_reused_pid_that_is_not_an_nzbget_is_left_alive() {
        let directory = std::env::temp_dir().join("mama-cine-orphan-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let pidfile = directory.join("nzbget.pid");
        std::fs::write(&pidfile, std::process::id().to_string()).expect("a pidfile");

        reclaim_orphan(&pidfile);

        // still running, which is the point
        assert!(!pidfile.exists(), "the stale pidfile is cleaned up");
    }

    // nzbget narrates every download at INFO. Forwarding all of it into a log that rotates at a
    // megabyte would push out the app's own history to duplicate what nzbget's log already has.
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

    // Her Windows profile is «María Esther», and every start ended in an nzbget that aborted
    // with 0xC0000409 before it could write a word about why. The í is the whole reason: nzbget
    // reads its own file name back from the system in the machine's code page and then decodes
    // it as UTF-8, and the two disagree about that one letter.
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

    // Copying ten megabytes on every start would be a start she waits through.
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
        // Windows carries the modification time across on its own; here it is done by hand, so
        // that the test says what a copy on her machine looks like rather than what one looks
        // like on the machine this is written on.
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

    // Only Windows cannot read its own name. Nothing is copied anywhere else, and the paths
    // there are left exactly as they were found.
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
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, contents: &[u8]) -> Result<()> {
    // the file carries the news server password; on Windows it inherits the user's profile acl
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents)?;
    Ok(())
}
