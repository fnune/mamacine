use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const ROTATE_AT: u64 = 1024 * 1024;

pub struct Log {
    path: PathBuf,
    lock: Mutex<()>,
    standing: Mutex<std::collections::HashMap<String, String>>,
}

impl Log {
    pub fn open(directory: &std::path::Path) -> Log {
        Log {
            path: Log::path_in(directory),
            lock: Mutex::new(()),
            standing: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn path_in(directory: &std::path::Path) -> PathBuf {
        directory.join("mamacine.log")
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn folder(&self) -> &std::path::Path {
        self.path.parent().unwrap_or(&self.path)
    }

    pub fn from(&self, who: &str, text: &str) {
        self.line(&format!("[{who}] {text}"));
    }

    pub fn standing(&self, subject: &str, text: &str) {
        {
            let mut standing = self.standing.lock().expect("not poisoned");
            if standing.get(subject).is_some_and(|said| said == text) {
                return;
            }
            standing.insert(subject.to_string(), text.to_string());
        }
        self.line(text);
    }

    pub fn settled(&self, subject: &str) {
        self.standing.lock().expect("not poisoned").remove(subject);
    }

    pub fn line(&self, text: &str) {
        let _held = self.lock.lock().expect("not poisoned");
        if let Ok(data) = std::fs::metadata(&self.path) {
            if data.len() > ROTATE_AT {
                let _ = std::fs::rename(&self.path, self.path.with_extension("log.old"));
            }
        }
        let stamp = timestamp();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{stamp} {text}");
        }
        eprintln!("{stamp} {text}");
    }
}

fn timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let days = seconds / 86_400;
    let (year, month, day) = civil_date(days as i64);
    let rest = seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_logged_can_be_read_back() {
        let directory = std::env::temp_dir().join("mama-cine-log-test");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");

        let log = Log::open(&directory);
        log.line("the first thing that happened");
        log.line("the second");

        let written = std::fs::read_to_string(directory.join("mamacine.log")).expect("the file");
        assert!(
            written.contains("the first thing that happened"),
            "{written}"
        );
        assert_eq!(written.lines().count(), 2);
        assert!(written.starts_with("20"), "{written}");
    }

    #[test]
    fn a_complaint_that_repeats_every_second_is_written_once() {
        let directory = std::env::temp_dir().join("mama-cine-log-standing");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let log = Log::open(&directory);

        for _ in 0..100 {
            log.standing("progress", "cannot reach the downloader");
        }
        log.standing("progress", "the downloader refused the account");
        log.standing("disk", "cannot reach the downloader");

        let written = std::fs::read_to_string(directory.join("mamacine.log")).expect("the file");
        assert_eq!(
            written.lines().count(),
            3,
            "the same thing once, a different thing, and the same thing about something else: \
             {written}"
        );

        log.settled("progress");
        log.standing("progress", "the downloader refused the account");
        let written = std::fs::read_to_string(directory.join("mamacine.log")).expect("the file");
        assert_eq!(written.lines().count(), 4, "{written}");
    }

    #[test]
    fn a_log_that_grew_too_big_is_rotated_rather_than_growing_forever() {
        let directory = std::env::temp_dir().join("mama-cine-log-rotate");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");

        let log = Log::open(&directory);
        std::fs::write(
            directory.join("mamacine.log"),
            vec![b'x'; (ROTATE_AT + 1) as usize],
        )
        .expect("a full log");
        log.line("fresh");

        let fresh = std::fs::read_to_string(directory.join("mamacine.log")).expect("the file");
        assert!(fresh.contains("fresh"));
        assert!(fresh.len() < 200, "the new file starts over");
        assert!(
            directory.join("mamacine.log.old").exists(),
            "nothing is lost"
        );
    }
}
