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
        let config_path = work.join("nzbget.conf");
        write_private(
            &config_path,
            render_config(settings, &work, port, &password, tools).as_bytes(),
        )?;

        log.line(&format!(
            "starting {} (exists: {}) with {} on port {port}",
            tools.nzbget.display(),
            tools.nzbget.exists(),
            config_path.display()
        ));

        let mut process = Command::new(&tools.nzbget)
            .arg("-c")
            .arg(&config_path)
            .arg("-s")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|failure| {
                log.line(&format!(
                    "nzbget could not be started ({}): {failure}",
                    tools.nzbget.display()
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
