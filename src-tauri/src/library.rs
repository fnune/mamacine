use crate::text::Lang;
use mamacine_core::media::MediaInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Note {
    pub at: i64,
    pub said: String,
    pub why: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Entry {
    pub story: Vec<Note>,
    pub title: String,
    pub year: Option<String>,
    pub cover_url: Option<String>,
    pub imdb: Option<String>,
    pub info: MediaInfo,
    pub subtitle_files: Vec<String>,
    pub series: bool,
    pub show: mamacine_core::indexer::ShowIds,
    pub seasons: Option<(u32, u32)>,
    pub settled: bool,
    pub subtitle_note: String,
    pub key: String,
    pub folder: Option<PathBuf>,
    pub file: Option<PathBuf>,
    pub attempt: usize,
    pub attempts_total: usize,
    pub superseded_by: Option<i64>,
    pub gave_up: bool,
    pub remaining: Vec<mamacine_core::indexer::SearchResult>,
    pub filed_as: String,
    pub untried: usize,
    pub allowance: usize,
    pub source: String,
    pub retired: bool,
    pub replaces: Option<i64>,
}

impl Entry {
    pub fn present(&self) -> bool {
        !self.retired
            && self.settled
            && self
                .folder
                .as_ref()
                .map(|folder| folder.exists())
                .unwrap_or(false)
    }
}

#[derive(Deserialize, Serialize)]
struct OnDisk {
    version: u32,
    entries: BTreeMap<String, Entry>,
    #[serde(default)]
    burned: std::collections::BTreeSet<String>,
}

const LIBRARY_VERSION: u32 = 3;

pub struct Library {
    path: PathBuf,
    entries: Mutex<BTreeMap<String, Entry>>,
    burned: Mutex<std::collections::BTreeSet<String>>,
    log: std::sync::Arc<crate::log::Log>,
    problem: Option<String>,
}

impl Library {
    pub fn open(directory: &Path, log: std::sync::Arc<crate::log::Log>, lang: Lang) -> Library {
        let path = directory.join("library.json");
        let (entries, burned, problem, migrated) = read_records(&path, &log, lang);
        let library = Library {
            path,
            entries: Mutex::new(entries),
            burned: Mutex::new(burned),
            log,
            problem,
        };
        if migrated {
            library.save();
        }
        library
    }

    pub fn problem(&self) -> Option<String> {
        self.problem.clone()
    }

    pub fn get(&self, id: i64) -> Option<Entry> {
        self.entries
            .lock()
            .expect("not poisoned")
            .get(&id.to_string())
            .cloned()
    }

    pub fn put(&self, id: i64, entry: Entry) {
        self.entries
            .lock()
            .expect("not poisoned")
            .insert(id.to_string(), entry);
        self.save();
    }

    pub fn all(&self) -> Vec<(i64, Entry)> {
        let entries = self.entries.lock().expect("not poisoned");
        let mut all: Vec<(i64, Entry)> = entries
            .iter()
            .filter_map(|(id, entry)| id.parse().ok().map(|id| (id, entry.clone())))
            .collect();
        all.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
        all
    }

    pub fn present(&self, key: &str) -> Option<(i64, Entry)> {
        if key.is_empty() {
            return None;
        }
        self.all()
            .into_iter()
            .find(|(_, entry)| entry.key == key && entry.present())
    }

    pub fn update(&self, id: i64, change: impl FnOnce(&mut Entry)) {
        let mut entries = self.entries.lock().expect("not poisoned");
        let entry = entries.entry(id.to_string()).or_default();
        change(entry);
        drop(entries);
        self.save();
    }

    pub fn burn(&self, nzb_url: &str) {
        if nzb_url.is_empty() {
            return;
        }
        self.log.line(&format!("burned: {nzb_url}"));
        self.burned
            .lock()
            .expect("not poisoned")
            .insert(nzb_url.to_string());
        self.save();
    }

    pub fn is_burned(&self, nzb_url: &str) -> bool {
        self.burned.lock().expect("not poisoned").contains(nzb_url)
    }

    pub fn note(&self, id: i64, said: &str, why: &str) {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or_default();
        self.log.line(&format!("[{id}] {said} ({why})"));
        self.update(id, |entry| {
            entry.story.push(Note {
                at,
                said: said.to_string(),
                why: why.to_string(),
            })
        });
    }

    pub fn reconcile(&self, destination: &Path) {
        self.release_ghosts();
        for (id, entry) in self.all() {
            if !entry.settled || entry.retired {
                continue;
            }
            let missing = entry
                .folder
                .as_ref()
                .map(|folder| !folder.exists())
                .unwrap_or(true);
            let key = key_for(&entry);
            if !missing && key == entry.key {
                continue;
            }
            let found = entry
                .folder
                .is_none()
                .then(|| folder_named(&entry.title, destination))
                .flatten();
            self.update(id, |entry| {
                entry.key = key;
                if let Some(folder) = found {
                    entry.file = crate::finishing::largest_video(&folder);
                    entry.folder = Some(folder);
                }
            });
        }
    }

    fn release_ghosts(&self) {
        let mut claimed = std::collections::HashSet::new();
        let mut ghosts = Vec::new();
        for (id, entry) in self.all() {
            let Some(folder) = entry.folder.filter(|_| entry.settled && !entry.retired) else {
                continue;
            };
            if !claimed.insert(folder.clone()) {
                self.log.line(&format!(
                    "{id} and a newer record both name {}: releasing the older one",
                    folder.display()
                ));
                ghosts.push(id);
            }
        }
        for id in ghosts {
            self.update(id, |entry| {
                entry.retired = true;
                entry.folder = None;
                entry.file = None;
            });
        }
    }

    fn save(&self) {
        if self.problem.is_some() {
            return;
        }
        let entries = self.entries.lock().expect("not poisoned");
        let on_disk = OnDisk {
            version: LIBRARY_VERSION,
            entries: entries.clone(),
            burned: self.burned.lock().expect("not poisoned").clone(),
        };
        if let Ok(text) = serde_json::to_vec_pretty(&on_disk) {
            let _ = std::fs::write(&self.path, text);
        }
    }
}

type Records = (
    BTreeMap<String, Entry>,
    std::collections::BTreeSet<String>,
    Option<String>,
    bool,
);

fn read_records(path: &Path, log: &crate::log::Log, lang: Lang) -> Records {
    let none = std::collections::BTreeSet::new;
    let Ok(bytes) = std::fs::read(path) else {
        return (BTreeMap::new(), none(), None, false);
    };

    if let Ok(on_disk) = serde_json::from_slice::<OnDisk>(&bytes) {
        return match on_disk.version.cmp(&LIBRARY_VERSION) {
            std::cmp::Ordering::Equal => (on_disk.entries, on_disk.burned, None, false),
            std::cmp::Ordering::Less => (on_disk.entries, on_disk.burned, None, true),
            std::cmp::Ordering::Greater => {
                log.line(&format!(
                    "library.json is version {}, this app understands {}: leaving it untouched",
                    on_disk.version, LIBRARY_VERSION
                ));
                (
                    BTreeMap::new(),
                    none(),
                    Some(lang.library_from_a_newer_app().to_string()),
                    false,
                )
            }
        };
    }

    if let Ok(entries) = serde_json::from_slice::<BTreeMap<String, Entry>>(&bytes) {
        let _ = std::fs::copy(path, path.with_extension("v1.json"));
        log.line("library.json migrated from v1 (bare map)");
        return (entries, none(), None, true);
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let aside = path.with_extension(format!("broken-{stamp}.json"));
    let _ = std::fs::rename(path, &aside);
    log.line(&format!(
        "library.json was unreadable; set aside as {}",
        aside.display()
    ));
    (
        BTreeMap::new(),
        std::collections::BTreeSet::new(),
        None,
        false,
    )
}

fn key_for(entry: &Entry) -> String {
    if !entry.key.is_empty() {
        return entry.key.clone();
    }
    match entry
        .imdb
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(imdb) => format!("imdb:{}", imdb.trim_start_matches('0')),
        None => String::new(),
    }
}

fn folder_named(title: &str, destination: &Path) -> Option<PathBuf> {
    let plainly = |text: &str| {
        text.to_lowercase()
            .chars()
            .map(|letter| {
                if letter.is_alphanumeric() {
                    letter
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let wanted = plainly(title);
    if wanted.is_empty() {
        return None;
    }
    std::fs::read_dir(destination)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| plainly(name).contains(&wanted))
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("mama-cine-library-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        directory
    }

    fn open(directory: &Path) -> Library {
        Library::open(
            directory,
            std::sync::Arc::new(crate::log::Log::open(directory)),
            Lang::Es,
        )
    }

    fn settled(key: &str, folder: &Path) -> Entry {
        Entry {
            title: key.into(),
            key: key.into(),
            settled: true,
            folder: Some(folder.to_path_buf()),
            ..Entry::default()
        }
    }

    #[test]
    fn a_film_is_hers_only_while_it_is_on_the_disk() {
        let directory = scratch("present");
        let films = directory.join("films");
        std::fs::create_dir_all(&films).expect("a folder");

        let library = open(&directory);
        library.put(1, settled("imdb:1", &films));
        assert!(library.present("imdb:1").is_some());

        std::fs::remove_dir_all(&films).expect("removable");
        assert!(library.present("imdb:1").is_none());
    }

    #[test]
    fn a_download_that_never_finished_is_not_on_the_shelf() {
        let directory = scratch("unsettled");
        let library = open(&directory);
        library.put(
            1,
            Entry {
                key: "imdb:1".into(),
                folder: Some(directory.clone()),
                ..Entry::default()
            },
        );
        assert!(library.present("imdb:1").is_none());
        assert!(library.all().iter().any(|(id, _)| *id == 1));
    }

    #[test]
    fn nothing_is_ever_matched_by_a_missing_name() {
        let directory = scratch("empty-key");
        let library = open(&directory);
        library.put(1, settled("", &directory));
        assert!(library.present("").is_none());
    }

    #[test]
    fn an_entry_written_before_the_library_knew_where_films_land_is_repaired() {
        let directory = scratch("reconcile");
        let films = directory.join("films");
        let landed = films.join("The.Red.Turtle.2016.1080p.BluRay.x264");
        std::fs::create_dir_all(&landed).expect("a folder");
        std::fs::write(landed.join("turtle.mkv"), b"not really a film").expect("a file");

        let library = open(&directory);
        library.put(
            1,
            Entry {
                title: "The Red Turtle".into(),
                imdb: Some("03666024".into()),
                settled: true,
                ..Entry::default()
            },
        );
        library.reconcile(&films);

        let (_, entry) = library.present("imdb:3666024").expect("she has this one");
        assert_eq!(entry.folder.as_deref(), Some(landed.as_path()));
        assert_eq!(
            entry
                .file
                .and_then(|file| file.file_name().map(|name| name.to_owned())),
            Some("turtle.mkv".into())
        );
    }

    #[test]
    fn a_film_that_is_no_longer_anywhere_is_not_invented() {
        let directory = scratch("reconcile-missing");
        let films = directory.join("films");
        std::fs::create_dir_all(&films).expect("a folder");

        let library = open(&directory);
        library.put(
            1,
            Entry {
                title: "Das Boot".into(),
                imdb: Some("0082096".into()),
                settled: true,
                ..Entry::default()
            },
        );
        library.reconcile(&films);
        assert!(library.present("imdb:82096").is_none());
    }

    #[test]
    fn a_film_she_threw_away_is_never_handed_the_folder_of_the_one_that_replaced_it() {
        let directory = scratch("reconcile-replaced");
        let films = directory.join("films");
        let deleted = films.join("La.Virgen.Roja.2024.ITA.1080p");
        let kept = films.join("La.Virgen.Roja.2024.TRUEFRENCH.1080p");
        std::fs::create_dir_all(&kept).expect("the copy she kept");

        let library = open(&directory);
        library.put(
            1,
            Entry {
                title: "La virgen roja".into(),
                imdb: Some("30748104".into()),
                settled: true,
                folder: Some(deleted.clone()),
                ..Entry::default()
            },
        );
        library.reconcile(&films);

        let (_, entry) = library.get(1).map(|entry| (1, entry)).expect("the record");
        assert_eq!(
            entry.folder.as_deref(),
            Some(deleted.as_path()),
            "the record still names the folder she emptied, and so it is not on her shelf"
        );
        assert!(library.present("imdb:30748104").is_none());
    }

    #[test]
    fn one_folder_is_one_film_however_many_records_point_at_it() {
        let directory = scratch("ghost");
        let films = directory.join("films");
        let landed = films.join("La.Virgen.Roja.2024.TRUEFRENCH.1080p");
        std::fs::create_dir_all(&landed).expect("the copy she has");

        let library = open(&directory);
        let ghosted = |title: &str| Entry {
            title: title.into(),
            key: "imdb:30748104".into(),
            settled: true,
            folder: Some(landed.clone()),
            ..Entry::default()
        };
        library.put(1, ghosted("La virgen roja"));
        library.put(2, ghosted("La virgen roja"));

        library.reconcile(&films);

        assert!(library.get(1).expect("the older record").folder.is_none());
        assert_eq!(
            library.get(2).expect("the newer record").folder.as_deref(),
            Some(landed.as_path()),
            "the copy she actually downloaded keeps its folder"
        );
        assert_eq!(
            library.present("imdb:30748104").map(|(id, _)| id),
            Some(2),
            "and there is only one of it"
        );
    }

    #[test]
    fn the_copies_left_to_try_survive_the_window_being_closed() {
        let directory = scratch("remaining");
        let library = open(&directory);
        library.put(
            5,
            Entry {
                title: "Das Boot".into(),
                key: "imdb:82096".into(),
                filed_as: "Das Boot".into(),
                attempts_total: 3,
                attempt: 1,
                ..Entry::default()
            },
        );
        drop(library);

        let reopened = open(&directory);
        let (id, entry) = reopened
            .all()
            .into_iter()
            .find(|(_, entry)| !entry.settled)
            .expect("still in flight");
        assert_eq!(id, 5);
        assert_eq!(entry.filed_as, "Das Boot");
        assert_eq!(entry.attempts_total, 3);
    }

    #[test]
    fn a_version_one_library_is_migrated_whole_with_the_original_kept_beside_it() {
        let directory = scratch("migrate-v1");
        std::fs::write(
            directory.join("library.json"),
            include_str!("../fixtures/library.v1.json"),
        )
        .expect("the fixture in place");

        let library = open(&directory);
        assert!(library.problem().is_none());
        let (id, entry) = library.all().into_iter().next().expect("her film");
        assert_eq!(id, 1);
        assert_eq!(entry.title, "The Red Turtle");
        assert!(entry.settled);

        let migrated =
            std::fs::read_to_string(directory.join("library.json")).expect("written back");
        assert!(migrated.contains("\"version\": 3"), "{migrated}");
        assert!(
            directory.join("library.v1.json").exists(),
            "the original survives the migration"
        );

        let reopened = open(&directory);
        assert_eq!(reopened.all().len(), 1, "and the migrated file reads back");
    }

    #[test]
    fn a_library_from_a_newer_app_is_refused_untouched_rather_than_clobbered() {
        let directory = scratch("newer");
        let newer = r#"{"version": 99, "entries": {"1": {"title": "From the future"}}}"#;
        std::fs::write(directory.join("library.json"), newer).expect("a newer file");

        let library = open(&directory);
        assert!(library
            .problem()
            .expect("said plainly")
            .contains("versión más nueva"));
        assert!(
            library.all().is_empty(),
            "nothing half-understood is loaded"
        );

        library.put(2, Entry::default());
        assert_eq!(
            std::fs::read_to_string(directory.join("library.json")).expect("still there"),
            newer,
            "and nothing may write over it"
        );
    }

    #[test]
    fn an_unreadable_library_is_set_aside_not_silently_replaced_with_nothing() {
        let directory = scratch("corrupt");
        std::fs::write(directory.join("library.json"), b"{ half a fi").expect("a corrupt file");

        let library = open(&directory);
        assert!(library.problem().is_none());
        assert!(library.all().is_empty());
        let aside = std::fs::read_dir(&directory)
            .expect("readable")
            .flatten()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("library.broken-")
            });
        assert!(
            aside.is_some(),
            "the broken file survives for whoever debugs it"
        );

        library.put(1, settled("imdb:1", &directory));
        let fresh = std::fs::read_to_string(directory.join("library.json")).expect("a fresh file");
        assert!(fresh.contains("\"version\": 3"));
    }

    #[test]
    fn a_version_two_library_is_adopted_and_restamped() {
        let directory = scratch("migrate-v2");
        std::fs::write(
            directory.join("library.json"),
            r#"{"version": 2, "entries": {"1": {"title": "El Sur", "settled": true}}}"#,
        )
        .expect("a v2 file");
        let library = open(&directory);
        assert!(library.problem().is_none());
        assert_eq!(library.all().len(), 1);
        let rewritten =
            std::fs::read_to_string(directory.join("library.json")).expect("written back");
        assert!(rewritten.contains("\"version\": 3"), "{rewritten}");
    }

    #[test]
    fn a_burned_copy_stays_burned_across_restarts() {
        let directory = scratch("burned");
        let library = open(&directory);
        library.burn("https://indexer.test/dead-copy.nzb");
        assert!(library.is_burned("https://indexer.test/dead-copy.nzb"));
        assert!(!library.is_burned("https://indexer.test/other.nzb"));
        drop(library);
        let reopened = open(&directory);
        assert!(reopened.is_burned("https://indexer.test/dead-copy.nzb"));
    }

    #[test]
    fn what_is_remembered_survives_a_restart() {
        let directory = scratch("restart");
        let library = open(&directory);
        library.put(3, settled("imdb:3", &directory));
        drop(library);

        let reopened = open(&directory);
        assert_eq!(reopened.present("imdb:3").map(|(id, _)| id), Some(3));
    }
}
