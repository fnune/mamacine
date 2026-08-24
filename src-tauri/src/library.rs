//! What the app remembers about a download that nzbget does not: which film it is, and where it
//! ended up.
//!
//! nzbget knows a name and a folder, for as long as its history keeps them. Everything she thinks
//! of as hers is here instead: the poster, the year, the film's own identity, and the path on her
//! disk. "Mis películas" is this list, filtered by what is still there, so the shelf and the disk
//! can never disagree.

use mamacine_core::media::MediaInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One line of the story of getting this film, in the order it happened.
///
/// Two registers on purpose. `said` is what she reads: plain Spanish, no vocabulary she has not
/// already got, nothing she is expected to act on. `why` is the same moment for whoever is fixing
/// the app: article counts, statuses, release names. Her screen shows the first and keeps the
/// second within reach, so that nothing the app does to itself is invisible.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Note {
    /// Seconds since the epoch. The window formats it: it knows her clock and her language.
    pub at: i64,
    pub said: String,
    pub why: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Entry {
    /// Everything that has happened to this film, oldest first. The screen is a view of this, so
    /// the app cannot change what it is doing without saying so.
    pub story: Vec<Note>,
    pub title: String,
    pub year: Option<String>,
    pub cover_url: Option<String>,
    pub imdb: Option<String>,
    pub info: MediaInfo,
    /// Languages of subtitle files sitting beside the film.
    pub subtitle_files: Vec<String>,
    /// A season rather than a film: a folder of episodes, and the app says so plainly.
    pub series: bool,
    /// The show this season belongs to, and which seasons the folder holds. Kept because the
    /// episodes on her screen are named by the show database long after the search that found
    /// them is gone: without this, a season she downloaded last month is a list of numbers.
    pub show: mamacine_core::indexer::ShowIds,
    pub seasons: Option<(u32, u32)>,
    /// True once the finishing work has run, so a restart never repeats it.
    pub settled: bool,
    pub subtitle_note: String,
    /// The same film under any of its names: what "ya la tienes" is decided by.
    pub key: String,
    /// Where it landed. The shelf is these, filtered by the ones still on the disk.
    pub folder: Option<PathBuf>,
    /// The film itself inside that folder. A season has none: it is a folder of episodes.
    pub file: Option<PathBuf>,
    /// Which copy this is, of how many worth trying. Shown while falling through dead ones.
    pub attempt: usize,
    pub attempts_total: usize,
    /// This copy was dead and another one was started in its place.
    pub superseded_by: Option<i64>,
    /// Every copy was tried and none of them arrived.
    pub gave_up: bool,
    /// Copies not tried yet, best first, and the name they are filed under. Kept on disk rather
    /// than in memory so that closing the window mid-download does not turn the next dead copy
    /// back into a dead end. Cleared once the download is settled.
    pub remaining: Vec<mamacine_core::indexer::SearchResult>,
    pub filed_as: String,
    /// Copies that existed beyond the chase limit and were never tried. Giving up may only claim
    /// "todo lo que había venía dañado" when this is zero.
    pub untried: usize,
    /// How many attempts the chase may spend before giving up. Zero means the default limit;
    /// "probar más copias" raises it, because the button is her overruling the app's budget.
    pub allowance: usize,
    /// The nzb address of the copy this download actually fetched: what gets burned when the
    /// downloader refuses it.
    pub source: String,
    /// She does not have this any more: she deleted it, she swapped it for another copy, or it
    /// was a second record of a film one record already accounted for.
    ///
    /// Said outright rather than left to be inferred from an empty folder, because every way of
    /// inferring it has been wrong. Clearing `settled` made the finisher settle the record again
    /// on the next sweep, straight back onto whatever folder nzbget's history still named;
    /// clearing `folder` made the startup repair go looking for a folder with her title in it and
    /// hand over the copy that had replaced this one. A record that says what it is stops both.
    pub retired: bool,
    /// The copy she is swapping out. She asked for a different one because the one she had was
    /// in the wrong language, so the old folder goes to the papelera the moment this one lands
    /// — never before, because a swap that fails must leave her the film she already had.
    pub replaces: Option<i64>,
}

impl Entry {
    /// On the shelf only once it is both finished and still there. A folder she emptied herself is
    /// a film she no longer has, and the app must not claim otherwise.
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

/// The library file, as it is written today. The version is what makes updating the app safe:
/// an older file is migrated (with the original kept beside it), a newer file is refused rather
/// than half-read, and a change to this shape must come with a migration and a fixture test.
#[derive(Deserialize, Serialize)]
struct OnDisk {
    version: u32,
    entries: BTreeMap<String, Entry>,
    /// Copies nzbget itself refused, by their nzb address. The downloader's verdict is ground
    /// truth the probe cannot always predict (par2 that exists but covers other files), and a
    /// copy proven dead once must never cost her bandwidth twice.
    #[serde(default)]
    burned: std::collections::BTreeSet<String>,
}

const LIBRARY_VERSION: u32 = 3;

pub struct Library {
    path: PathBuf,
    entries: Mutex<BTreeMap<String, Entry>>,
    burned: Mutex<std::collections::BTreeSet<String>>,
    log: std::sync::Arc<crate::log::Log>,
    /// The file belongs to a newer version of the app: nothing was loaded, and nothing may be
    /// written, because half-understanding her records and then overwriting them destroys them.
    problem: Option<String>,
}

impl Library {
    pub fn open(directory: &Path, log: std::sync::Arc<crate::log::Log>) -> Library {
        let path = directory.join("library.json");
        let (entries, burned, problem, migrated) = read_records(&path, &log);
        let library = Library {
            path,
            entries: Mutex::new(entries),
            burned: Mutex::new(burned),
            log,
            problem,
        };
        if migrated {
            // completed now rather than on the next incidental change, so a crash in between
            // cannot leave the file half-owned by two versions
            library.save();
        }
        library
    }

    /// Why her records could not be used, when they could not. The app surfaces this instead of
    /// pretending she has nothing.
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

    /// Everything remembered, newest first: ids come from nzbget and only ever grow.
    pub fn all(&self) -> Vec<(i64, Entry)> {
        let entries = self.entries.lock().expect("not poisoned");
        let mut all: Vec<(i64, Entry)> = entries
            .iter()
            .filter_map(|(id, entry)| id.parse().ok().map(|id| (id, entry.clone())))
            .collect();
        all.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
        all
    }

    /// The copy of this film she actually has, if she has one.
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

    /// Remembers a copy the downloader itself refused, so it is never spent again.
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

    /// Adds a line to a film's story. Every change the app makes on its own goes through here, so
    /// that "it changed while I was watching and said nothing" cannot happen. The technical `why`
    /// also lands in the log, which is the only place on her machine anyone can read it later.
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

    /// Makes what is remembered agree with what is on the disk, once, at startup.
    ///
    /// Entries written before the library knew where films landed have no folder, and would drop
    /// off her shelf even though the film is right there. Anything still unaccounted for is left
    /// alone: it will simply not be on the shelf, which is the truth.
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
            // Only a record that never knew where its film landed goes looking. A folder that
            // was written down and is now gone is a film she threw away, and `folder_named`
            // matches on the title alone: after deleting one copy and downloading another, it
            // handed the deleted record the new copy's folder. Both records then said she had
            // the film, so her shelf showed it twice and "ya la tienes" answered for a copy
            // that was in the recycle bin.
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

    /// Two records cannot both be the film sitting in one folder.
    ///
    /// Repairing what the old rule already wrote down. A film she deleted was handed the folder
    /// of the copy that replaced it, and from then on her shelf held both: the copy she has, and
    /// the ghost of the one she threw away, pointing at the same place on the disk. The newer
    /// record is the copy she actually downloaded, so the older one lets go.
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
        // never write over a file this version does not fully understand
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

/// Reads whatever version of the file is there. Returns the entries, the reason they could not
/// be used (if they could not), and whether a migration needs writing back.
type Records = (
    BTreeMap<String, Entry>,
    std::collections::BTreeSet<String>,
    Option<String>,
    bool,
);

fn read_records(path: &Path, log: &crate::log::Log) -> Records {
    let none = std::collections::BTreeSet::new;
    let Ok(bytes) = std::fs::read(path) else {
        return (BTreeMap::new(), none(), None, false); // the first run ever
    };

    if let Ok(on_disk) = serde_json::from_slice::<OnDisk>(&bytes) {
        return match on_disk.version.cmp(&LIBRARY_VERSION) {
            std::cmp::Ordering::Equal => (on_disk.entries, on_disk.burned, None, false),
            // v2 lacked the burned list, which defaults empty: adopt and rewrite stamped
            std::cmp::Ordering::Less => (on_disk.entries, on_disk.burned, None, true),
            std::cmp::Ordering::Greater => {
                log.line(&format!(
                    "library.json is version {}, this app understands {}: leaving it untouched",
                    on_disk.version, LIBRARY_VERSION
                ));
                (
                    BTreeMap::new(),
                    none(),
                    Some(
                        "Los datos de las películas son de una versión más nueva de la \
                         aplicación. Hay que actualizar la aplicación para seguir."
                            .to_string(),
                    ),
                    false,
                )
            }
        };
    }

    // version 1 wrote a bare map of entries, with no version at all
    if let Ok(entries) = serde_json::from_slice::<BTreeMap<String, Entry>>(&bytes) {
        // the original stays beside the migrated file, so a migration bug never costs her shelf
        let _ = std::fs::copy(path, path.with_extension("v1.json"));
        log.line("library.json migrated from v1 (bare map)");
        return (entries, none(), None, true);
    }

    // unreadable: set it aside rather than silently overwriting her records with nothing, which
    // is what `.ok().unwrap_or_default()` used to do here
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

/// The name the app would give this entry today. Only ever used to fill in an older one.
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

/// A download folder is named after the release, not after the film: "The Red Turtle" arrives as
/// "The.Red.Turtle.2016.1080p.BluRay.x264". The title being in the name is as much as can be said.
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

        // she emptied the folder herself, which is the same as not having the film
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

    // She deleted a copy and downloaded another. On the next start this went looking for a
    // folder with her title in it, found the copy that replaced it, and wrote it into the record
    // of the one she had thrown away. Both records then claimed she had the film.
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

    // The old rule already wrote the ghost into her library: two records, one folder, one film
    // standing on her shelf twice. Starting the app has to put that right, not just stop doing it.
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

    // Closing the window mid-download used to lose the copies it had left to try, so the next
    // dead copy became a dead end again.
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

    // The app will be updated again. A file from version 1 (a bare map, no version stamp) must
    // load whole, and the original must survive beside the migrated file so a migration bug can
    // never cost her shelf. The fixture is a real v1 file, pinned: every future schema change
    // must keep this test passing by adding its own migration.
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

    // A downgrade must never half-read a newer file and then overwrite it: that destroys her
    // records. Refusing, and saying why, is the only honest behaviour.
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

    // A version-2 file (entries, no burned list yet) must load whole and come back stamped.
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

    // The downloader's refusal is ground truth the probe cannot always predict: a copy proven
    // dead once must never cost her bandwidth twice, not even after a restart.
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
