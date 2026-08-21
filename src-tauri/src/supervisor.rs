//! Starting and stopping the private nzbget instance that does the downloading.

use mamacine_core::error::{Error, Result};
use mamacine_core::http::HttpClient;
use mamacine_core::nzbget::{render_config, NzbgetRpc, Tools};
use mamacine_core::settings::Settings;
use std::io::Write;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct Nzbget {
    process: Option<Child>,
    pidfile: std::path::PathBuf,
    pub port: u16,
    pub password: String,
}

impl Nzbget {
    /// Writes a config only this instance uses, on a port only this instance knows.
    pub fn start<H: HttpClient>(settings: &Settings, tools: &Tools, http: &H) -> Result<Nzbget> {
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

        let process = Command::new(&tools.nzbget)
            .arg("-c")
            .arg(&config_path)
            .arg("-s")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|failure| {
                Error::Setup(format!(
                    "nzbget could not be started ({}): {failure}",
                    tools.nzbget.display()
                ))
            })?;

        let _ = std::fs::write(&pidfile, process.id().to_string());
        let started = Nzbget {
            process: Some(process),
            pidfile,
            port,
            password,
        };

        let rpc = NzbgetRpc::new(started.port, &started.password, http);
        let deadline = Instant::now() + Duration::from_secs(25);
        while Instant::now() < deadline {
            if rpc.is_ready() {
                return Ok(started);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(Error::Setup(
            "nzbget did not start in time. The log is in the application folder.".into(),
        ))
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
