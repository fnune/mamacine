//! Getting films: the decisions between the window and the services, owned by one testable thing.
//!
//! Everything volatile arrives as a trait object, so every behaviour here can be exercised with
//! fakes: the four rounds of field bugs all lived in this layer, back when it was welded to the
//! real network inside the composition root.

use crate::library::{Entry, Library};
use crate::log::Log;
use crate::messages;
use mamacine_core::films::{group, Film};
use mamacine_core::identity::{film_key, season_key};
use mamacine_core::indexer::{Category, Indexer, Query, SearchResult, ShowIds};
use mamacine_core::lookup::{Picked, Suggestion};
use mamacine_core::media::MediaInfo;
use mamacine_core::nzbget::{Downloader, QueueItem, ServerCheck, Status};
use mamacine_core::release::Preference;
use mamacine_core::search::gather;
use mamacine_core::series::{group_seasons, Episode, Season};
use mamacine_core::settings::NewsServer;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// How many copies are tried before giving up. Chasing every copy the indexer listed spent 200 GB
/// and a day to arrive at "no"; three failures in a row say the rest are not worth her bandwidth.
pub const CHASE_LIMIT: usize = 3;

/// How much room is left where her films go, and how big that disk is. Behind a trait so a test
/// can fill the disk.
pub trait Disk: Send + Sync {
    fn space(&self, path: &Path) -> Option<crate::disk::Space>;
}

pub struct SystemDisk;

impl Disk for SystemDisk {
    fn space(&self, path: &Path) -> Option<crate::disk::Space> {
        crate::disk::space(path)
    }
}

/// Deleting a film she asked to remove. Behind a trait so a test never touches a real bin.
pub trait Remover: Send + Sync {
    fn remove(&self, folder: &Path) -> Result<(), String>;
}

/// The recycle bin rather than deletion: a mistaken tap must be recoverable by anyone.
pub struct SystemRemover;

impl Remover for SystemRemover {
    fn remove(&self, folder: &Path) -> Result<(), String> {
        trash::delete(folder).map_err(|failure| failure.to_string())
    }
}

/// Title suggestions as she types, and what picking one means. Behind a trait so a test can
/// answer instantly, and because there are two providers: TMDB when a key is configured (titles
/// in her language, the original named outright), the keyless IMDb lookup otherwise.
pub trait Suggest: Send + Sync {
    fn suggest(&self, text: &str) -> mamacine_core::error::Result<Vec<Suggestion>>;
    fn resolve(&self, suggestion: &Suggestion) -> mamacine_core::error::Result<Picked>;
    fn poster(&self, url: &str) -> mamacine_core::error::Result<(String, Vec<u8>)>;
    /// What the film is about, by IMDb id. Only TMDB can say; the default is the honest silence
    /// of a provider that has no words for it.
    fn synopsis(&self, _imdb: &str) -> mamacine_core::error::Result<Option<String>> {
        Ok(None)
    }
    /// The episodes a run of seasons holds, named where the provider names them.
    fn episodes(
        &self,
        _show: &ShowIds,
        _first: u32,
        _last: u32,
    ) -> mamacine_core::error::Result<Vec<Episode>> {
        Ok(Vec::new())
    }
    /// Which show a name is, for a season she already owns whose search is long gone. A different
    /// question from suggesting titles as she types: there is one right answer and she is not
    /// there to choose. Empty ids mean the provider could not say, which is an answer.
    fn show_named(&self, _name: &str) -> mamacine_core::error::Result<ShowIds> {
        Ok(ShowIds::default())
    }
}

/// The provider that needs no key: IMDb names the titles she is offered, TVMaze turns the one she
/// picks into the ids an indexer files television under. Two services because neither does both,
/// and no key for either, which is what makes this the default.
pub struct Keyless<H> {
    titles: mamacine_core::lookup::Lookup<H>,
    shows: mamacine_core::tvmaze::TvMaze<H>,
}

impl<H: mamacine_core::http::HttpClient> Keyless<H> {
    pub fn new(
        titles: mamacine_core::lookup::Lookup<H>,
        shows: mamacine_core::tvmaze::TvMaze<H>,
    ) -> Self {
        Keyless { titles, shows }
    }
}

impl<H: mamacine_core::http::HttpClient + Send + Sync> Suggest for Keyless<H> {
    fn suggest(&self, text: &str) -> mamacine_core::error::Result<Vec<Suggestion>> {
        self.titles.suggest(text)
    }
    /// A show costs one more question, to the only service that answers it without a key. When it
    /// cannot be reached the pick fails, and the window falls back to searching by the name she
    /// picked, which is what a nameless pick would have done anyway.
    fn resolve(&self, suggestion: &Suggestion) -> mamacine_core::error::Result<Picked> {
        let picked = mamacine_core::lookup::resolve(suggestion);
        let Some(imdb) = picked.show.imdb.clone() else {
            return Ok(picked);
        };
        Ok(Picked {
            show: self.shows.ids_for(&imdb)?,
            ..picked
        })
    }
    fn poster(&self, url: &str) -> mamacine_core::error::Result<(String, Vec<u8>)> {
        self.titles.poster(url)
    }
    fn episodes(
        &self,
        show: &ShowIds,
        first: u32,
        last: u32,
    ) -> mamacine_core::error::Result<Vec<Episode>> {
        match &show.tvmaze {
            Some(id) => self.shows.episodes(id, first, last),
            None => Ok(Vec::new()),
        }
    }
    fn show_named(&self, name: &str) -> mamacine_core::error::Result<ShowIds> {
        self.shows.show_named(name)
    }
}

impl<H: mamacine_core::http::HttpClient + Send + Sync> Suggest for mamacine_core::tmdb::Tmdb<H> {
    fn suggest(&self, text: &str) -> mamacine_core::error::Result<Vec<Suggestion>> {
        mamacine_core::tmdb::Tmdb::suggest(self, text)
    }
    fn resolve(&self, suggestion: &Suggestion) -> mamacine_core::error::Result<Picked> {
        mamacine_core::tmdb::Tmdb::resolve(self, suggestion)
    }
    fn poster(&self, url: &str) -> mamacine_core::error::Result<(String, Vec<u8>)> {
        mamacine_core::tmdb::Tmdb::poster(self, url)
    }
    fn synopsis(&self, imdb: &str) -> mamacine_core::error::Result<Option<String>> {
        mamacine_core::tmdb::Tmdb::synopsis(self, imdb)
    }
    fn episodes(
        &self,
        show: &ShowIds,
        first: u32,
        last: u32,
    ) -> mamacine_core::error::Result<Vec<Episode>> {
        match &show.tmdb {
            Some(id) => mamacine_core::tmdb::Tmdb::episodes(self, id, first, last),
            None => Ok(Vec::new()),
        }
    }
    fn show_named(&self, name: &str) -> mamacine_core::error::Result<ShowIds> {
        mamacine_core::tmdb::Tmdb::show_named(self, name)
    }
}

/// A video in a season's folder: its file name, the season and episode that name states, and
/// where it is.
type EpisodeFile = (String, Option<(u32, u32)>, PathBuf);

/// Something she should see even if the window is closed or she is on another screen.
pub type Notify = Box<dyn Fn(&str, &str) + Send + Sync>;

/// A download and what to do when it turns out to be dead: the release that failed is abandoned
/// and the next best one is started, without asking.
pub struct Attempt {
    pub seed: Entry,
    /// Untried copies, best first. The chase stops after `CHASE_LIMIT` failures, but the rest
    /// stay listed so "probar más copias" is a real button rather than a new search.
    pub remaining: Vec<SearchResult>,
    pub total: usize,
    /// The name nzbget files it under, kept so every copy lands in the same place.
    pub name: String,
}

#[derive(Serialize)]
pub struct FilmCard {
    pub index: usize,
    pub imdb: Option<String>,
    pub title: String,
    pub year: Option<String>,
    pub about: String,
    pub cover_url: Option<String>,
    pub quality: String,
    pub size: String,
    pub size_bytes: u64,
    pub room: &'static str,
    /// How much this looks like what she typed. The window orders the mixed list by it, so the
    /// thing she named is not buried under everything that merely mentions it.
    pub relevance: f64,
}

#[derive(Serialize)]
pub struct SeasonCard {
    pub index: usize,
    pub show: String,
    /// The show's own IMDb id, when the search knew which show this is.
    pub imdb: Option<String>,
    pub label: String,
    pub size: String,
    pub size_bytes: u64,
    pub quality: String,
    pub cover_url: Option<String>,
    pub grabs: u64,
    pub room: &'static str,
    pub relevance: f64,
}

/// One episode of a season she owns, as the screen that plays it needs it.
#[derive(Default, Serialize)]
pub struct EpisodeRow {
    pub label: String,
    /// Subtitles she can read, beside the file or inside it.
    pub subtitles: bool,
    /// What the show database calls it, and what happens in it. Absent for a file whose number
    /// could not be read, and for a season downloaded before the app kept the show's ids.
    pub title: Option<String>,
    pub overview: Option<String>,
    pub season: Option<u32>,
    pub number: Option<u32>,
}

/// One search, answered with everything it found: films and whole seasons in the same list, so
/// which kind a name belongs to is the app's problem rather than a chip she has to press first.
#[derive(Serialize)]
pub struct Found {
    pub films: Vec<FilmCard>,
    pub seasons: Vec<SeasonCard>,
    /// A place that could not answer, named. The rest of the results still stand.
    pub notice: Option<String>,
    /// The search knew which title she meant, whether she picked it, typed its id or typed its
    /// name. An empty answer then means the sites do not carry it, and saying "no hay nada con ese
    /// nombre" would blame her for their gaps.
    pub exact: bool,
}

/// One release of a film, described as she would judge it rather than as it is named.
#[derive(Serialize)]
pub struct Version {
    pub index: usize,
    pub quality: String,
    pub size: String,
    pub size_bytes: u64,
    pub language: String,
    /// The language group a chip can stand for: "es", "latino" or "original".
    pub voice: &'static str,
    pub grabs: u64,
    pub chosen: bool,
    pub name: String,
    /// Whether it fits on her disk, decided by the same rule that will later refuse it.
    pub room: &'static str,
    /// The room it actually needs while it downloads and unpacks, which is more than its size:
    /// the number behind the warning, so the warning never has to be taken on faith.
    pub needs: String,
    /// How long it will take at the speed downloads have actually run at, when that is known.
    pub minutes: Option<i64>,
}

#[derive(Serialize)]
pub struct Active {
    pub id: i64,
    pub title: String,
    pub status: &'static str,
    pub percent: f64,
    pub beneath: String,
    pub cover_url: Option<String>,
    pub year: Option<String>,
    pub attempt: usize,
    pub attempts_total: usize,
    pub series: bool,
    pub speed: String,
    pub story: Vec<crate::library::Note>,
}

#[derive(Serialize)]
pub struct Finished {
    pub id: i64,
    pub title: String,
    pub ok: bool,
    pub detail: String,
    pub subtitle_note: String,
    pub cover_url: Option<String>,
    pub year: Option<String>,
    pub languages: MediaInfo,
    pub series: bool,
    pub next_id: Option<i64>,
    /// It failed, but the app has not decided what to do yet: the screen must keep waiting
    /// rather than flash a failure that is about to be untrue.
    pub retrying: bool,
    pub attempt: usize,
    pub attempts_total: usize,
    /// Copies kept beyond the chase limit: what "probar más copias" would try.
    pub untried: usize,
    pub story: Vec<crate::library::Note>,
}

#[derive(Serialize)]
pub struct Shelved {
    pub id: i64,
    pub title: String,
    pub year: Option<String>,
    pub cover_url: Option<String>,
    pub subtitle_note: String,
    pub languages: MediaInfo,
    pub series: bool,
}

/// What the detail screen needs to not lie: on the shelf, on its way, or neither.
#[derive(Serialize)]
pub struct Owned {
    pub have: Option<i64>,
    pub downloading: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Grabbed {
    pub id: i64,
    pub already: bool,
}

#[derive(Serialize)]
pub struct Progress {
    pub active: Vec<Active>,
    pub finished: Vec<Finished>,
    pub shelf: Vec<Shelved>,
    pub free_space: String,
    pub free_bytes: u64,
    /// The whole disk, so "quedan 40 GB" can be judged: 40 of 100 and 40 of 4000 are different
    /// facts, and hiding either is deciding for her.
    pub total_space: String,
    pub total_bytes: u64,
    /// The downloader stopped answering: said plainly instead of freezing the screen in place.
    pub problem: Option<String>,
}

/// What the tray icon shows: the tooltip on hover, and the same thing in one line for the menu,
/// which is where Linux has to read it because a tray tooltip is a Windows idea.
pub struct TrayReport {
    pub tooltip: String,
    pub summary: String,
}

pub struct Orchestrator {
    pub indexers: Vec<(String, Box<dyn Indexer>)>,
    pub downloader: Box<dyn Downloader>,
    pub library: Arc<Library>,
    pub log: Arc<Log>,
    pub destination: PathBuf,
    pub news: NewsServer,
    pub preference: Preference,
    /// The language her subtitles are in, so a screen can say which episodes have none.
    pub subtitle_language: String,
    pub disk: Box<dyn Disk>,
    pub remover: Box<dyn Remover>,
    pub suggestions: Box<dyn Suggest>,
    pub prober: Box<dyn mamacine_core::nntp::Prober>,
    pub notify: Notify,
    covers: Mutex<HashMap<String, String>>,
    found: Mutex<Vec<Film>>,
    seasons: Mutex<Vec<Season>>,
    attempts: Mutex<HashMap<i64, Attempt>>,
    /// Films whose chase is waiting for the news server to come back. Their attempts stay in
    /// `attempts`; a server problem is transient and must never consume a film.
    stalled: Mutex<std::collections::HashSet<i64>>,
    /// What is wrong with the news server right now, in her words, shown as the banner.
    server_trouble: Mutex<Option<String>>,
    server_checked: Mutex<Option<std::time::Instant>>,
    /// How long between retries while the server is down. Public so tests need not wait.
    pub server_recheck: std::time::Duration,
    /// The last suggestions and the text they answer, so a tap can name one by position rather
    /// than post a struct back, and so submitting right after typing reuses what was fetched.
    suggested: Mutex<(String, Vec<Suggestion>)>,
    /// What she last picked, kept here rather than sent to the window and back: the ids a show is
    /// filed under are ours to use and nothing the window could interpret.
    picked: Mutex<Option<Picked>>,
    /// Which show the seasons on screen belong to, when the search knew. Empty after a search by
    /// name, where the packs are whatever the indexer matched and no show was identified.
    searched_show: Mutex<ShowIds>,
    /// The episodes of a season, by show and season, asked of the show database once.
    listed: Mutex<HashMap<String, Vec<Episode>>>,
    /// The best download rate seen this session, for "tardará unos X minutos" estimates.
    last_rate: Mutex<u64>,
    /// What each film is about, by IMDb id. An empty answer is kept too: the database saying
    /// nothing once is not a reason to ask it again.
    synopses: Mutex<HashMap<String, String>>,
}

pub struct Pieces {
    pub indexers: Vec<(String, Box<dyn Indexer>)>,
    pub downloader: Box<dyn Downloader>,
    pub library: Arc<Library>,
    pub log: Arc<Log>,
    pub destination: PathBuf,
    pub news: NewsServer,
    pub preference: Preference,
    pub subtitle_language: String,
    pub disk: Box<dyn Disk>,
    pub remover: Box<dyn Remover>,
    pub suggestions: Box<dyn Suggest>,
    pub prober: Box<dyn mamacine_core::nntp::Prober>,
    pub notify: Notify,
}

impl Orchestrator {
    pub fn new(pieces: Pieces) -> Orchestrator {
        let orchestrator = Orchestrator {
            indexers: pieces.indexers,
            downloader: pieces.downloader,
            library: pieces.library,
            log: pieces.log,
            destination: pieces.destination,
            news: pieces.news,
            preference: pieces.preference,
            subtitle_language: pieces.subtitle_language,
            disk: pieces.disk,
            remover: pieces.remover,
            suggestions: pieces.suggestions,
            prober: pieces.prober,
            notify: pieces.notify,
            covers: Mutex::new(HashMap::new()),
            found: Mutex::new(Vec::new()),
            seasons: Mutex::new(Vec::new()),
            attempts: Mutex::new(HashMap::new()),
            stalled: Mutex::new(std::collections::HashSet::new()),
            server_trouble: Mutex::new(None),
            server_checked: Mutex::new(None),
            server_recheck: std::time::Duration::from_secs(60),
            suggested: Mutex::new((String::new(), Vec::new())),
            picked: Mutex::new(None),
            searched_show: Mutex::new(ShowIds::default()),
            listed: Mutex::new(HashMap::new()),
            last_rate: Mutex::new(0),
            synopses: Mutex::new(HashMap::new()),
        };
        // a download interrupted by closing the window keeps the copies it had left to try
        for (id, entry) in orchestrator.library.all() {
            // a gave-up film keeps its untried copies for the button, not for a silent resume
            if entry.settled || entry.gave_up || entry.remaining.is_empty() {
                continue;
            }
            orchestrator.attempts.lock().expect("not poisoned").insert(
                id,
                Attempt {
                    total: entry.attempts_total,
                    name: entry.filed_as.clone(),
                    remaining: entry.remaining.clone(),
                    seed: Entry {
                        remaining: Vec::new(),
                        ..entry
                    },
                },
            );
        }
        orchestrator
    }

    // --- search ------------------------------------------------------------------

    /// `kind` is what is already known about the name: a suggestion she picked says whether it is
    /// a film or a series, and asking the other category anyway buried the seasons she chose
    /// under a wall of parodies and episode reviews.
    pub fn search(
        &self,
        query: &str,
        kind: Option<&str>,
        shown: Option<&str>,
    ) -> Result<Found, String> {
        let Some(parsed) = Query::parse(query) else {
            return Ok(Found {
                films: Vec::new(),
                seasons: Vec::new(),
                notice: None,
                exact: false,
            });
        };
        let indexers = || {
            self.indexers
                .iter()
                .map(|(name, indexer)| (name.as_str(), indexer.as_ref()))
        };
        if self.indexers.is_empty() {
            return Err(
                "No hay ningún buscador configurado. Hay que rellenar los ajustes primero."
                    .to_string(),
            );
        }

        let nothing = || mamacine_core::search::Gathered {
            results: Vec::new(),
            problems: Vec::new(),
        };
        // she typed a name and pressed Buscar, which must mean what tapping the first row means:
        // work out what it names, then ask the indexer for that. An id she typed already names one
        // film, and a suggestion she tapped arrived identified.
        let identified = match (&parsed, kind) {
            (Query::Imdb(_), _) | (_, Some(_)) => Vec::new(),
            _ => self.identify(query),
        };
        let identified_as = |series: bool| identified.iter().find(|found| found.series == series);
        // an id names one title beyond doubt, but a name she typed still has to say whether she
        // meant the film or the series of the same name. The provider offered them in an order,
        // and that order is its own answer: what it put first is what the name means, and the
        // other kind stays on the screen below it rather than tying with it.
        let certain = |series: bool| match identified.first() {
            Some(first) if first.series != series => 2.0,
            _ => 3.0,
        };

        // the film she meant, asked for by whatever names it: an IMDb id where the provider
        // registers one, the international name where it does not. Her own words are the question
        // of last resort, for when nothing recognised them at all: a provider that answered with a
        // series and no film has said there is no film, and asking anyway spends a search hit to
        // be told the same thing in noise.
        let film_question = match (identified_as(false), identified.is_empty()) {
            (Some(film), _) => Query::parse(&film.query),
            (None, true) => Some(parsed.clone()),
            (None, false) => None,
        };
        // the name an answer has to resemble, or nothing at all when the question named one film
        // outright and there is no room left for the indexer to have misunderstood
        let judge_films_by = match &film_question {
            None | Some(Query::Imdb(_)) => None,
            Some(_) => Some(
                identified_as(false)
                    .map(|film| film.query.as_str())
                    .unwrap_or(query),
            ),
        };
        let films_found = match (kind, &film_question) {
            (Some("series"), _) | (_, None) => nothing(),
            (_, Some(question)) => gather(indexers(), question, Some(Category::Movies)),
        };

        let picked_show = self.picked_show(query);
        let show = match picked_show.any() {
            true => picked_show,
            false => identified_as(true)
                .map(|found| found.show.clone())
                .unwrap_or_default(),
        };
        *self.searched_show.lock().expect("not poisoned") = show.clone();
        // the name the packs carry, which the indexer only falls back on when no id identified the
        // show. Her own words are the last resort, the same as for a film.
        let show_named = identified_as(true)
            .map(|found| found.query.as_str())
            .unwrap_or(query);
        // the same last resort as a film's: television is asked for the show that was identified,
        // or for her words when nothing at all was, and not at all when the name turned out to
        // mean a film
        let television = identified.is_empty() || identified_as(true).is_some();
        // an id names one film exactly; asking television for it would answer with noise
        let seasons_found = match &parsed {
            Query::Imdb(_) => nothing(),
            _ if kind == Some("film") => nothing(),
            _ if !television => nothing(),
            _ => gather(
                indexers(),
                &Query::Show {
                    // releases are filed in scene ASCII, so the name rung has to be folded too
                    name: mamacine_core::search::fold(show_named.trim()),
                    ids: show.clone(),
                },
                Some(Category::Television),
            ),
        };
        let problems_everywhere =
            !films_found.problems.is_empty() || !seasons_found.problems.is_empty();
        if films_found.results.is_empty() && seasons_found.results.is_empty() && problems_everywhere
        {
            let all: Vec<(String, mamacine_core::error::Error)> = films_found
                .problems
                .into_iter()
                .chain(seasons_found.problems)
                .collect();
            return Err(self.explain_problems(&all));
        }

        let free = self.disk.space(&self.destination).map(|space| space.free);
        let mut films = group(films_found.results, self.preference);
        // an id search answers with exactly the film she picked, so every group is that film and
        // may carry the name she picked it under; the picked name then follows it everywhere
        if let (Query::Imdb(_), Some(name)) = (&parsed, shown) {
            for film in &mut films {
                film.title = name.to_string();
            }
        }
        let film_cards = films
            .iter()
            .enumerate()
            .map(|(index, film)| FilmCard {
                index,
                imdb: film.imdb.clone(),
                title: film.title.clone(),
                year: film.year.clone(),
                about: film.about.clone(),
                cover_url: film.cover_url.clone(),
                quality: film
                    .best()
                    .map(definition_of)
                    .unwrap_or_default()
                    .to_string(),
                size: film.best().map(size_of).unwrap_or_default(),
                size_bytes: film.best().map(|release| release.size_bytes).unwrap_or(0),
                room: room_of(
                    free,
                    film.best().map(|release| release.size_bytes).unwrap_or(0),
                ),
                relevance: match judge_films_by {
                    Some(asked) => looks_like(asked, &film.title, &film.releases),
                    // asked by an id, which names one film: whatever came back is that film
                    None => certain(false),
                },
            })
            // sharing not one word with what she typed, in the card or in any release, is the
            // indexer free-associating: "el sur" answered with Tinker Bell and Pumpkinhead
            .filter(|card| card.relevance > 0.0)
            .collect();

        // asked by id, every pack that came back is that one show under whichever name it was
        // released: it takes the name the show was identified as, not the words she reached for
        // it with, and there is nothing left to second-guess
        let named = show.any().then(|| {
            shown
                .or(identified_as(true).map(|found| found.title.as_str()))
                .unwrap_or(query)
        });
        let seasons = group_seasons(seasons_found.results, self.preference, named);
        let season_cards = seasons
            .iter()
            .enumerate()
            .map(|(index, season)| SeasonCard {
                index,
                show: season.show.clone(),
                imdb: show.imdb.clone(),
                label: season.label.clone(),
                size: season.best().map(size_of).unwrap_or_default(),
                size_bytes: season.best().map(|release| release.size_bytes).unwrap_or(0),
                quality: season
                    .best()
                    .map(|release| definition_of(release).to_string())
                    .unwrap_or_default(),
                cover_url: season
                    .releases
                    .iter()
                    .find_map(|release| release.cover_url.clone()),
                grabs: season.best().map(|release| release.grabs).unwrap_or(0),
                room: room_of(
                    free,
                    season.best().map(|release| release.size_bytes).unwrap_or(0),
                ),
                relevance: match named {
                    Some(_) => certain(true),
                    None => looks_like(show_named, &season.show, &season.releases),
                },
            })
            .filter(|card| card.relevance > 0.0)
            .collect();

        let problems: Vec<(String, mamacine_core::error::Error)> = films_found
            .problems
            .into_iter()
            .chain(seasons_found.problems)
            .collect();
        let notice = (!problems.is_empty()).then(|| self.explain_problems(&problems));

        *self.found.lock().expect("not poisoned") = films;
        *self.seasons.lock().expect("not poisoned") = seasons;
        Ok(Found {
            films: film_cards,
            seasons: season_cards,
            notice,
            exact: kind.is_some() || matches!(parsed, Query::Imdb(_)) || !identified.is_empty(),
        })
    }

    /// Each indexer that could not answer, with what actually happened to it: a rejected key, a
    /// spent quota and a dead connection call for different actions, and one vague sentence for
    /// all three hid the one fact she could have acted on. The english detail goes to the log.
    fn explain_problems(&self, problems: &[(String, mamacine_core::error::Error)]) -> String {
        problems
            .iter()
            .map(|(name, error)| {
                let explained = messages::explain(error);
                self.log.line(&format!("search: {name}: {}", explained.why));
                format!("{name}: {}", explained.said)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn suggest(&self, text: &str) -> Result<Vec<Suggestion>, String> {
        let found = self
            .suggestions
            .suggest(text)
            .map_err(|failure| messages::explain(&failure).said)?;
        *self.suggested.lock().expect("not poisoned") = (text.to_string(), found.clone());
        Ok(found)
    }

    /// What a tapped suggestion means: the query the indexer can answer, and the name everything
    /// downstream should call the thing.
    pub fn pick(&self, index: usize) -> Result<Picked, String> {
        let suggestion = self
            .suggested
            .lock()
            .expect("not poisoned")
            .1
            .get(index)
            .cloned()
            .ok_or_else(|| "Ese título ya no está en la lista.".to_string())?;
        let picked = self
            .suggestions
            .resolve(&suggestion)
            .map_err(|failure| messages::explain(&failure).said)?;
        *self.picked.lock().expect("not poisoned") = Some(picked.clone());
        Ok(picked)
    }

    /// The suggestions for this text, reusing the ones the typing already fetched. The window
    /// asks for them on every keystroke and submitting asks the same question again, and that
    /// second question is a call somebody else is paying for.
    fn suggestions_for(&self, text: &str) -> Vec<Suggestion> {
        {
            let (asked, found) = &*self.suggested.lock().expect("not poisoned");
            if same_text(asked, text) {
                return found.clone();
            }
        }
        match self.suggestions.suggest(text) {
            Ok(found) => found,
            // a title provider that cannot be reached costs her the translation, not the search
            Err(failure) => {
                self.log.line(&format!(
                    "search: no suggestions for \"{text}\": {}",
                    messages::explain(&failure).why
                ));
                Vec::new()
            }
        }
    }

    /// What she means, worked out before an indexer is asked anything.
    ///
    /// Tapping a suggestion resolves the title and then searches by the id that names it. Typing
    /// the same name and pressing Buscar used to send her words straight to an indexer, so the two
    /// ways of starting a search answered differently: "El castillo ambulante" found the film when
    /// tapped and nothing at all when typed, because the releases are named "Howl\'s Moving Castle"
    /// and no indexer has ever heard of the Spanish name. She does not know the two are different.
    /// So they are one thing: her words are identified first, exactly as a tap does it, and the
    /// search that follows is the search a tap would have run.
    ///
    /// The best film and the best series, because a typed name says which of the two it is only
    /// when she picks a row. What the provider offered is taken as it stands: matching a name in
    /// any language to the thing it names is the one job it has, and it does it against a list of
    /// aliases nothing here can see. IMDb answers "juego de tronos" with "Game of Thrones" and
    /// says nothing about why, and a check that the answer resembles the question would throw that
    /// away. Where it recognised nothing, her own words are the question.
    ///
    /// The order is the provider's, and the first offer is its answer to which of the two she
    /// meant.
    fn identify(&self, typed: &str) -> Vec<Picked> {
        let suggestions = self.suggestions_for(typed);
        let mut resolved: Vec<Picked> = Vec::new();
        // walked in the provider's own order, so the first one resolved is the first one it
        // offered, and one of each kind because a typed name says which it is only when tapped.
        // One attempt each: a provider that could not resolve the best title it has will not do
        // better with the next, and every attempt is a call somebody else is paying for.
        let (mut tried_film, mut tried_series) = (false, false);
        for suggestion in &suggestions {
            let tried = match suggestion.series {
                true => &mut tried_series,
                false => &mut tried_film,
            };
            if *tried {
                continue;
            }
            *tried = true;
            match self.suggestions.resolve(suggestion) {
                Ok(picked) => resolved.push(picked),
                // identifying it is what fails, not the search: her words are still a question
                Err(failure) => self.log.line(&format!(
                    "search: \"{typed}\" looks like \"{}\", but: {}",
                    suggestion.title,
                    messages::explain(&failure).why
                )),
            }
        }
        resolved
    }

    /// The ids of the show she picked, when the search that follows is for that same show. A name
    /// she typed herself resolves to nothing, and the search asks by name.
    fn picked_show(&self, query: &str) -> ShowIds {
        self.picked
            .lock()
            .expect("not poisoned")
            .as_ref()
            .filter(|picked| picked.series && picked.query == query)
            .map(|picked| picked.show.clone())
            .unwrap_or_default()
    }

    // --- what a tap needs --------------------------------------------------------

    pub fn versions(&self, index: usize, series: bool) -> Result<Vec<Version>, String> {
        let releases = if series {
            let seasons = self.seasons.lock().expect("not poisoned");
            seasons
                .get(index)
                .map(|season| season.releases.clone())
                .ok_or_else(|| "Esa temporada ya no está en los resultados.".to_string())?
        } else {
            let found = self.found.lock().expect("not poisoned");
            found
                .get(index)
                .map(|film| film.releases.clone())
                .ok_or_else(|| "Esa película ya no está en los resultados.".to_string())?
        };
        let free = self.disk.space(&self.destination).map(|space| space.free);
        let rate = *self.last_rate.lock().expect("not poisoned");
        Ok(releases
            .iter()
            .enumerate()
            .map(|(position, release)| Version {
                index: position,
                quality: definition_of(release).to_string(),
                size: size_of(release),
                size_bytes: release.size_bytes,
                language: language_of(release),
                voice: voice_of(release),
                grabs: release.grabs,
                chosen: position == 0,
                name: release.title.clone(),
                room: room_of(free, release.size_bytes),
                needs: gigabytes(mamacine_core::space::needed_for(release.size_bytes)),
                minutes: minutes_for(release.size_bytes, rate),
            })
            .collect())
    }

    /// Whether she already has this — answered by her folder — and whether it is already on its
    /// way. The detail screen offered "Descargar" for a season that was downloading at that very
    /// moment, because it only knew about the shelf.
    pub fn have(&self, index: usize, series: bool) -> Owned {
        let Some(key) = self.key_at(index, series) else {
            return Owned {
                have: None,
                downloading: None,
            };
        };
        Owned {
            have: self.library.present(&key).map(|(id, _)| id),
            downloading: self.in_flight(&key),
        }
    }

    fn key_at(&self, index: usize, series: bool) -> Option<String> {
        if series {
            let seasons = self.seasons.lock().expect("not poisoned");
            seasons.get(index).map(season_key)
        } else {
            let found = self.found.lock().expect("not poisoned");
            found.get(index).map(film_key)
        }
    }

    // --- downloading -------------------------------------------------------------

    pub fn grab(
        &self,
        index: usize,
        version: Option<usize>,
        series: bool,
    ) -> Result<Grabbed, String> {
        let (seed, releases, name, key) = if series {
            let season = self
                .seasons
                .lock()
                .expect("not poisoned")
                .get(index)
                .cloned()
                .ok_or_else(|| "Esa temporada ya no está en los resultados.".to_string())?;
            let key = season_key(&season);
            let show = self.searched_show.lock().expect("not poisoned").clone();
            let seed = Entry {
                title: format!("{} · {}", season.show, season.label),
                cover_url: season
                    .releases
                    .iter()
                    .find_map(|release| release.cover_url.clone()),
                series: true,
                // what her own copy of this season is, kept while the search that found it is
                // still on screen: her screen names its episodes from these long afterwards
                imdb: show.imdb.clone(),
                seasons: Some((season.first, season.last)),
                show,
                key: key.clone(),
                ..Entry::default()
            };
            let name = format!("{} {}", season.show, season.label);
            (seed, season.releases, name, key)
        } else {
            let film = self
                .found
                .lock()
                .expect("not poisoned")
                .get(index)
                .cloned()
                .ok_or_else(|| "Esa película ya no está en los resultados.".to_string())?;
            let key = film_key(&film);
            let seed = Entry {
                title: film.title.clone(),
                year: film.year.clone(),
                cover_url: film.cover_url.clone(),
                imdb: film.imdb.clone(),
                key: key.clone(),
                ..Entry::default()
            };
            // nzbget will only ever know the name; the film itself is remembered in the library
            let name = film.title.clone();
            (seed, film.releases, name, key)
        };

        if let Some((id, _)) = self.library.present(&key) {
            return Ok(Grabbed { id, already: true });
        }
        // a second press must follow the download that is already running, not start a twin
        if let Some(id) = self.in_flight(&key) {
            return Ok(Grabbed { id, already: false });
        }

        let ordered = candidates_from(&releases, version);
        self.start(Attempt {
            total: ordered.len(),
            remaining: ordered,
            seed,
            name,
        })
        .map(|id| Grabbed { id, already: false })
    }

    fn in_flight(&self, key: &str) -> Option<i64> {
        if let Some(id) = self
            .attempts
            .lock()
            .expect("not poisoned")
            .iter()
            .find(|(_, attempt)| attempt.seed.key == key)
            .map(|(id, _)| *id)
        {
            return Some(id);
        }
        let queue = self.downloader.queue().ok()?;
        queue.iter().map(|item| item.id).find(|id| {
            self.library
                .get(*id)
                .map(|entry| entry.key == key && !entry.settled)
                .unwrap_or(false)
        })
    }

    /// Starts the first copy that fits on the disk, and keeps the rest for when it fails.
    fn start(&self, mut attempt: Attempt) -> Result<i64, String> {
        let total = attempt.total.max(attempt.remaining.len());
        if attempt.remaining.is_empty() {
            return Err("No hay ninguna copia para descargar.".to_string());
        }
        let free = self.disk.space(&self.destination).map(|space| space.free);
        let mut too_big = None;
        let mut trouble: Option<String> = None;
        // a copy passed over is the app deciding something; each one becomes a line of the story
        let mut skipped: Vec<(String, String)> = Vec::new();

        while !attempt.remaining.is_empty() {
            let release = attempt.remaining.remove(0);
            // asked before it starts, rather than discovered at ninety percent with a full disk
            if let Some(free) = free {
                if mamacine_core::space::room_for(free, release.size_bytes)
                    == mamacine_core::space::Room::NotEnough
                {
                    too_big.get_or_insert(release.size_bytes);
                    skipped.push((
                        format!(
                            "Una copia de {} no cabe en el disco, así que la he descartado.",
                            gigabytes(release.size_bytes)
                        ),
                        release.title.clone(),
                    ));
                    continue;
                }
            }
            let Some(indexer) = self.indexer_for(&release.nzb_url) else {
                trouble.get_or_insert("Ese buscador ya no está en los ajustes.".into());
                skipped.push((
                    "No he podido pedir una de las copias, así que he pasado a la siguiente."
                        .into(),
                    format!("{}: no configured indexer", release.nzb_url),
                ));
                continue;
            };
            // the downloader's past refusal is ground truth the probe cannot always predict:
            // a copy proven dead must never cost her bandwidth twice
            if self.library.is_burned(&release.nzb_url) {
                self.log
                    .line(&format!("skipping burned copy: {}", release.title));
                trouble.get_or_insert(messages::NO_WORKING_COPY.to_string());
                continue;
            }
            let nzb = match indexer.fetch_nzb(&release.nzb_url) {
                Ok(nzb) => nzb,
                Err(failure) => {
                    let explained = messages::explain(&failure);
                    self.log.line(&format!("start: {}", explained.why));
                    trouble.get_or_insert(explained.said);
                    skipped.push((
                        "No he podido pedir una de las copias, así que he pasado a la siguiente."
                            .into(),
                        explained.why,
                    ));
                    continue;
                }
            };
            // asked before a byte is downloaded: nzbget can only prove a rotten copy by
            // fetching gigabytes until the failure count is beyond repair, while a sampled
            // STAT conversation knows in seconds. Inconclusive means nzbget decides. The skip
            // is silent on her screen — nothing visible changed, so there is nothing to say —
            // and the log keeps the numbers.
            if self.rotten(&release.title, &nzb).is_some() {
                trouble.get_or_insert(messages::NO_WORKING_COPY.to_string());
                continue;
            }
            let id = match self.downloader.append(&attempt.name, &nzb) {
                Ok(id) => id,
                Err(failure) => {
                    let explained = messages::explain(&failure);
                    self.log.line(&format!("start: {}", explained.why));
                    trouble.get_or_insert(explained.said);
                    skipped.push((
                        "No he podido pedir una de las copias, así que he pasado a la siguiente."
                            .into(),
                        explained.why,
                    ));
                    continue;
                }
            };

            let mut entry = attempt.seed.clone();
            // counted by what nzbget was actually handed: a copy skipped by the probe or the
            // disk cost her nothing and must not spend the chase allowance
            entry.attempt = attempt.seed.attempt + 1;
            entry.source = release.nzb_url.clone();
            entry.attempts_total = total;
            entry.remaining = attempt.remaining.clone();
            entry.filed_as = attempt.name.clone();
            let first = entry.story.is_empty();
            self.library.put(id, entry);
            for (said, why) in &skipped {
                self.library.note(id, said, why);
            }
            // the story belongs to the film, not to the copy, and starting a copy is a thing that
            // happened to it
            self.library.note(
                id,
                &format!(
                    "{} ({}).",
                    if first {
                        "Empieza la descarga"
                    } else {
                        "Empieza la descarga de otra copia"
                    },
                    gigabytes(release.size_bytes)
                ),
                &format!(
                    "intento {} (de hasta {total} copias): {}",
                    attempt.seed.attempt + 1,
                    release.title,
                ),
            );
            // the kept plan must carry the count forward, or the next copy would believe it is
            // the first again and the chase limit would never arrive
            attempt.seed.attempt += 1;
            self.attempts
                .lock()
                .expect("not poisoned")
                .insert(id, attempt);
            return Ok(id);
        }

        if let Some(size) = too_big {
            let free = free.unwrap_or(0);
            return Err(format!(
                "No hay sitio en el disco. Hace falta un hueco de unos {} y quedan {}. \
                 Quita alguna película que ya hayas visto y vuelve a intentarlo.",
                gigabytes(mamacine_core::space::needed_for(size)),
                gigabytes(free),
            ));
        }
        Err(trouble.unwrap_or_else(|| "No se ha podido empezar la descarga.".to_string()))
    }

    /// The technical reason a copy is skipped without downloading, when the sample is certain.
    /// A minimum of articles keeps the probe honest, and any failure to ask defers to nzbget.
    fn rotten(&self, title: &str, nzb: &[u8]) -> Option<String> {
        let contents = mamacine_core::nzb::read(nzb).ok()?;
        if contents.data_ids.len() < 50 {
            return None;
        }
        let ratio = |statuses: &[bool]| {
            if statuses.is_empty() {
                0.0
            } else {
                statuses.iter().filter(|gone| **gone).count() as f64 / statuses.len() as f64
            }
        };
        let ids = contents.sample(300);
        let missing = match self.prober.statuses(&self.news, &ids) {
            Ok(statuses) => ratio(&statuses),
            Err(failure) => {
                self.log
                    .line(&format!("probe {title} inconclusive: {failure}"));
                return None;
            }
        };
        // takedowns hit the repair data too — often first — so only surviving par counts
        let par_ids = contents.sample_par(100);
        let par_missing = match self.prober.statuses(&self.news, &par_ids) {
            Ok(statuses) => ratio(&statuses),
            Err(_) => 0.0, // unknown: assume the paper coverage, nzbget remains the backstop
        };
        let effective = contents.effective_par(par_missing);
        self.log.line(&format!(
            "probe {title}: data {:.1}% missing of {}, par {:.1}% on paper, {:.1}% usable",
            missing * 100.0,
            ids.len(),
            contents.par_ratio() * 100.0,
            effective * 100.0
        ));
        if mamacine_core::nzb::beyond_repair(missing, effective) {
            return Some(format!(
                "{title}: {:.1}% missing vs {:.1}% usable par",
                missing * 100.0,
                effective * 100.0
            ));
        }
        // damage within the paper coverage: worth one more question — is the repair data even
        // real? Posts disguise data as ".par2" to dodge scanners; nzbget then finds nothing to
        // repair with and any damage is fatal. Seen live: ten fake par2 volumes on one season.
        if missing > 0.005 {
            let mut authentic = None;
            for id in contents.par_index_segments().into_iter().take(2) {
                // a failed fetch is no evidence either way
                if let Ok(bytes) = self.prober.fetch_body(&self.news, id) {
                    authentic = Some(mamacine_core::par2::contains_packets(&bytes));
                    if authentic == Some(true) {
                        break;
                    }
                }
            }
            if authentic == Some(false) {
                self.log
                    .line(&format!("probe {title}: the par2 files are not par2"));
                return Some(format!(
                    "{title}: {:.1}% missing and the repair files are fakes",
                    missing * 100.0
                ));
            }
        }
        None
    }

    fn indexer_for(&self, url: &str) -> Option<&dyn Indexer> {
        let host = host_of(url)?;
        self.indexers
            .iter()
            .find(|(_, indexer)| indexer.host().as_deref() == Some(host.as_str()))
            .map(|(_, indexer)| indexer.as_ref())
    }

    // --- the chase ---------------------------------------------------------------

    /// Watches for copies that turned out to be dead and quietly starts the next one. Usenet drops
    /// articles as a release ages, so the first copy failing is expected rather than exceptional.
    pub fn chase(&self) {
        // while the news server is down the chase waits and retries on its own: a transient
        // outage must never consume a film, and hammering the server helps nobody
        if self.server_trouble.lock().expect("not poisoned").is_some() {
            let due = self
                .server_checked
                .lock()
                .expect("not poisoned")
                .map(|at| at.elapsed() >= self.server_recheck)
                .unwrap_or(true);
            if !due {
                return;
            }
            *self.server_checked.lock().expect("not poisoned") = Some(std::time::Instant::now());
            match self.downloader.check_server(&self.news) {
                ServerCheck::Working => {
                    *self.server_trouble.lock().expect("not poisoned") = None;
                    let waiting: Vec<i64> =
                        self.stalled.lock().expect("not poisoned").drain().collect();
                    for id in waiting {
                        let Some(attempt) = self.attempts.lock().expect("not poisoned").remove(&id)
                        else {
                            continue;
                        };
                        self.library.note(
                            id,
                            "Ya puedo conectarme otra vez; sigo probando copias.",
                            "the news server answers again",
                        );
                        self.continue_after(id, attempt);
                    }
                }
                ServerCheck::Refused(reason) => {
                    self.log
                        .line(&format!("news server still refusing: {reason}"));
                    *self.server_trouble.lock().expect("not poisoned") =
                        Some(messages::SERVER_REFUSED.to_string());
                    return;
                }
                ServerCheck::Unreachable(reason) => {
                    self.log
                        .line(&format!("news server still unreachable: {reason}"));
                    *self.server_trouble.lock().expect("not poisoned") =
                        Some(messages::SERVER_UNREACHABLE.to_string());
                    return;
                }
                // nothing was learned; keep waiting rather than flip state on no evidence
                ServerCheck::Unknown => return,
            }
        }

        let Ok(history) = self.downloader.history() else {
            return;
        };
        // only downloads that existed before this tick may be judged "vanished": one started a
        // moment ago has not had time to appear anywhere yet
        let tracked_before: std::collections::HashSet<i64> = self
            .attempts
            .lock()
            .expect("not poisoned")
            .keys()
            .copied()
            .collect();
        for item in &history {
            if item.succeeded {
                continue;
            }
            if abandoned_deliberately(&item.status) {
                // she stopped it herself; carrying on behind her is the opposite of what she asked
                self.attempts.lock().expect("not poisoned").remove(&item.id);
                continue;
            }
            if self
                .stalled
                .lock()
                .expect("not poisoned")
                .contains(&item.id)
            {
                continue; // already answered: waiting for the server, not for another discard note
            }
            let Some(attempt) = self.attempts.lock().expect("not poisoned").remove(&item.id) else {
                // a failed copy nobody is handling: a crash between decisions, or state written
                // by an older build. The screen presents an undecided failure as "working on
                // it", so an orphan must be decided here or that would be a forever-state.
                let Some(entry) = self.library.get(item.id) else {
                    continue;
                };
                if entry.settled || entry.gave_up || entry.superseded_by.is_some() {
                    // already decided: nothing left to do but bury the corpse. A failed item
                    // keeps its partial gigabytes in nzbget's work folder until somebody
                    // deletes the history entry, and 48 GB of dead seasons proved nobody did.
                    if !entry.settled {
                        let _ = self.downloader.forget(item.id);
                    }
                    continue;
                }
                self.library.note(
                    item.id,
                    messages::GAVE_UP_ON_THIS_COPY,
                    &format!("adopted orphan: {}", explain_failure(item)),
                );
                self.library.burn(&entry.source);
                self.continue_after(
                    item.id,
                    Attempt {
                        total: entry.attempts_total,
                        name: entry.filed_as.clone(),
                        remaining: entry.remaining.clone(),
                        seed: Entry {
                            remaining: Vec::new(),
                            ..entry
                        },
                    },
                );
                continue;
            };
            self.library.note(
                item.id,
                messages::GAVE_UP_ON_THIS_COPY,
                &explain_failure(item),
            );
            self.library
                .burn(&self.library.get(item.id).unwrap_or_default().source);
            self.continue_after(item.id, attempt);
            // the library and the story carry everything the screen needs; the corpse's partial
            // gigabytes in nzbget's work folder carry nothing
            if !self
                .stalled
                .lock()
                .expect("not poisoned")
                .contains(&item.id)
            {
                let _ = self.downloader.forget(item.id);
            }
        }

        // a copy nzbget knows nothing about — not queued, not in history — is a copy it lost or
        // rejected on arrival. Seen live: the library believed copy 5 of 7 was downloading and
        // the screen said "empezando" forever, while nzbget had never heard of it.
        let Ok(queue) = self.downloader.queue() else {
            return;
        };
        let known: std::collections::HashSet<i64> = queue
            .iter()
            .map(|item| item.id)
            .chain(history.iter().map(|item| item.id))
            .collect();
        let stalled = self.stalled.lock().expect("not poisoned").clone();
        let vanished: Vec<i64> = self
            .attempts
            .lock()
            .expect("not poisoned")
            .keys()
            .filter(|id| {
                tracked_before.contains(id) && !known.contains(id) && !stalled.contains(id)
            })
            .copied()
            .collect();
        for id in vanished {
            let Some(attempt) = self.attempts.lock().expect("not poisoned").remove(&id) else {
                continue;
            };
            self.library.note(
                id,
                "Esa descarga se ha perdido por el camino, así que pruebo con otra copia.",
                "gone from nzbget: neither the queue nor the history knows the id",
            );
            self.continue_after(id, attempt);
        }
    }

    /// One copy is dead or lost; what happens to the film now. The next copy inherits the story:
    /// it is the same film, and she is owed the whole of it.
    fn continue_after(&self, id: i64, mut attempt: Attempt) {
        // a run of dead copies and a broken account look identical from the failures alone;
        // the downloader can ask the server. A server problem is transient and never consumes
        // the film: the chase waits and retries on its own.
        match self.downloader.check_server(&self.news) {
            ServerCheck::Refused(reason) => {
                self.stall(id, attempt, messages::SERVER_REFUSED, &reason);
                return;
            }
            ServerCheck::Unreachable(reason) => {
                self.stall(id, attempt, messages::SERVER_UNREACHABLE, &reason);
                return;
            }
            ServerCheck::Working | ServerCheck::Unknown => {}
        }

        let entry = self.library.get(id).unwrap_or_default();
        if attempt.remaining.is_empty() {
            let said = messages::gave_up(entry.series, entry.attempt, 0);
            self.library.note(id, &said, "no copies left to try");
            self.give_up(id, &attempt, &said);
            return;
        }
        // enough failures in a row say the rest are not worth her bandwidth, but the rest stay
        // listed: "probar más copias" is her call to make, and it costs one button
        let allowed = if entry.allowance == 0 {
            CHASE_LIMIT
        } else {
            entry.allowance
        };
        if entry.attempt >= allowed {
            let said = messages::gave_up(entry.series, entry.attempt, attempt.remaining.len());
            self.library
                .note(id, &said, "chase limit reached; the rest are kept for her");
            let kept = attempt.remaining.clone();
            self.library.update(id, |entry| {
                entry.gave_up = true;
                entry.untried = kept.len();
                entry.remaining = kept;
            });
            let entry = self.library.get(id).unwrap_or_default();
            (self.notify)(&entry.title, &said);
            return;
        }
        attempt.seed.story = self.library.get(id).unwrap_or_default().story;
        match self.start(attempt) {
            Ok(next) => {
                self.library.update(id, |entry| {
                    entry.superseded_by = Some(next);
                    entry.remaining.clear();
                });
            }
            Err(failure) => {
                // the reason the chase stopped is part of the story, not a stderr aside
                self.library
                    .note(id, &failure, "no further copy could start");
                let attempt = Attempt {
                    seed: Entry::default(),
                    remaining: Vec::new(),
                    total: 0,
                    name: String::new(),
                };
                self.give_up(id, &attempt, &failure);
            }
        }
    }

    /// The chase pauses for this film until the server answers again. Said once per outage, not
    /// once per tick, and the attempt is kept whole.
    fn stall(&self, id: i64, attempt: Attempt, said: &'static str, why: &str) {
        let first_word = self.server_trouble.lock().expect("not poisoned").is_none();
        if first_word {
            self.library.note(id, said, why);
            let entry = self.library.get(id).unwrap_or_default();
            (self.notify)(&entry.title, said);
        }
        *self.server_trouble.lock().expect("not poisoned") = Some(said.to_string());
        *self.server_checked.lock().expect("not poisoned") = Some(std::time::Instant::now());
        self.stalled.lock().expect("not poisoned").insert(id);
        self.attempts
            .lock()
            .expect("not poisoned")
            .insert(id, attempt);
    }

    /// She pressed the button the give-up screen offers: carry on with the copies that were
    /// kept beyond the chase limit, under a fresh allowance.
    pub fn try_more(&self, id: i64) -> Result<Grabbed, String> {
        let entry = self
            .library
            .get(id)
            .filter(|entry| entry.gave_up && !entry.remaining.is_empty())
            .ok_or_else(|| "No quedan más copias que probar.".to_string())?;
        self.library.note(
            id,
            "Sigo con las copias que quedaban.",
            "she asked for more",
        );
        let remaining = entry.remaining.clone();
        let seed = Entry {
            remaining: Vec::new(),
            gave_up: false,
            superseded_by: None,
            untried: 0,
            allowance: entry.attempt + CHASE_LIMIT,
            story: self.library.get(id).unwrap_or_default().story,
            ..entry.clone()
        };
        let total = entry.attempts_total.max(entry.attempt + remaining.len());
        let name = entry.filed_as.clone();
        let next = self.start(Attempt {
            seed,
            remaining,
            total,
            name,
        })?;
        self.library.update(id, |entry| {
            entry.superseded_by = Some(next);
            entry.remaining.clear();
            entry.untried = 0;
        });
        Ok(Grabbed {
            id: next,
            already: false,
        })
    }

    fn give_up(&self, id: i64, _attempt: &Attempt, said: &str) {
        self.library.update(id, |entry| {
            entry.gave_up = true;
            entry.remaining.clear();
        });
        let entry = self.library.get(id).unwrap_or_default();
        (self.notify)(&entry.title, said);
    }

    pub fn cancel(&self, id: i64) -> Result<(), String> {
        // dropped first: stopping means stopping, not falling through to the next copy
        self.attempts.lock().expect("not poisoned").remove(&id);
        self.library
            .note(id, messages::CANCELLED, "cancelled from the window");
        self.library.update(id, |entry| entry.remaining.clear());
        self.downloader
            .cancel(id)
            .map_err(|failure| messages::explain(&failure).said)
    }

    // --- progress ----------------------------------------------------------------

    pub fn progress(&self) -> Progress {
        let measured = self.disk.space(&self.destination);
        let free_bytes = measured.map(|space| space.free).unwrap_or(0);
        let total_bytes = measured.map(|space| space.total).unwrap_or(0);
        let shelf = self.shelf();

        let queue = match self.downloader.queue() {
            Ok(queue) => queue,
            Err(failure) => {
                let explained = messages::explain(&failure);
                self.log.line(&format!("progress: {}", explained.why));
                return Progress {
                    active: Vec::new(),
                    finished: Vec::new(),
                    shelf,
                    free_space: gigabytes(free_bytes),
                    free_bytes,
                    total_space: gigabytes(total_bytes),
                    total_bytes,
                    problem: Some(explained.said),
                };
            }
        };
        let history = match self.downloader.history() {
            Ok(history) => history,
            Err(failure) => {
                let explained = messages::explain(&failure);
                self.log.line(&format!("progress: {}", explained.why));
                return Progress {
                    active: Vec::new(),
                    finished: Vec::new(),
                    shelf,
                    free_space: gigabytes(free_bytes),
                    free_bytes,
                    total_space: gigabytes(total_bytes),
                    total_bytes,
                    problem: Some(explained.said),
                };
            }
        };
        let rate = self.downloader.download_rate().unwrap_or(0);
        if rate > 500_000 {
            let mut last = self.last_rate.lock().expect("not poisoned");
            *last = rate.max(*last);
        }

        let active = queue
            .iter()
            .map(|item| {
                let remembered = self.library.get(item.id).unwrap_or_default();
                Active {
                    cover_url: remembered.cover_url,
                    year: remembered.year,
                    id: item.id,
                    title: if remembered.title.is_empty() {
                        item.name.clone()
                    } else {
                        remembered.title
                    },
                    status: match item.status {
                        Status::Queued => "starting",
                        Status::Downloading => "downloading",
                        Status::Verifying => "verifying",
                        Status::Repairing => "repairing",
                        Status::Unpacking => "unpacking",
                        Status::Moving => "moving",
                        // its own word: "finishing" over a stalled bar read as a fault of its own
                        Status::Paused => "paused",
                        Status::Finishing | Status::Other(_) => "finishing",
                    },
                    percent: item.percent(),
                    beneath: remaining_in_words(item.remaining_mb, rate),
                    speed: speed_in_words(rate),
                    attempt: remembered.attempt,
                    attempts_total: remembered.attempts_total,
                    series: remembered.series,
                    story: remembered.story,
                }
            })
            .collect();

        let attempts = self.attempts.lock().expect("not poisoned");
        let known: std::collections::HashSet<i64> = queue
            .iter()
            .map(|item| item.id)
            .chain(history.iter().map(|item| item.id))
            .collect();
        let mut finished: Vec<Finished> = history
            .iter()
            .cloned()
            .map(|item| {
                let known_entry = self.library.get(item.id).is_some();
                let remembered = self.library.get(item.id).unwrap_or_default();
                Finished {
                    detail: if remembered.gave_up && remembered.superseded_by.is_none() {
                        messages::gave_up(remembered.series, remembered.attempt, remembered.untried)
                    } else {
                        String::new()
                    },
                    subtitle_note: remembered.subtitle_note,
                    title: if remembered.title.is_empty() {
                        item.name.clone()
                    } else {
                        remembered.title
                    },
                    cover_url: remembered.cover_url,
                    year: remembered.year,
                    languages: remembered.info,
                    series: remembered.series,
                    next_id: remembered.superseded_by,
                    // failed, but the chase has not answered yet: the screen keeps waiting rather
                    // than flashing a failure that is about to be untrue
                    // a dead copy is "failed" only once the app has decided to give up, and
                    // that decision lives in the library. Deriving this from the in-memory
                    // attempts map flashed "no he podido conseguirla" during the seconds the
                    // chase spent fetching the next copy.
                    retrying: known_entry
                        && !item.succeeded
                        && !abandoned_deliberately(&item.status)
                        && !remembered.gave_up
                        && remembered.superseded_by.is_none(),
                    attempt: remembered.attempt,
                    attempts_total: remembered.attempts_total,
                    untried: remembered.untried,
                    ok: item.succeeded && remembered.superseded_by.is_none(),
                    story: remembered.story,
                    id: item.id,
                }
            })
            .collect();

        // a download nzbget has lost still has a story and, once the chase acts, a successor: the
        // window follows the chain through here, so a vanished id must not break the chain and
        // freeze the screen on "empezando"
        for (id, entry) in self.library.all() {
            if entry.settled || known.contains(&id) {
                continue;
            }
            let part_of_a_chain =
                entry.superseded_by.is_some() || entry.gave_up || attempts.contains_key(&id);
            if !part_of_a_chain {
                continue;
            }
            finished.push(Finished {
                detail: if entry.gave_up && entry.superseded_by.is_none() {
                    messages::gave_up(entry.series, entry.attempt, entry.untried)
                } else {
                    String::new()
                },
                subtitle_note: entry.subtitle_note,
                title: entry.title,
                cover_url: entry.cover_url,
                year: entry.year,
                languages: entry.info,
                series: entry.series,
                next_id: entry.superseded_by,
                retrying: !entry.gave_up && entry.superseded_by.is_none(),
                attempt: entry.attempt,
                attempts_total: entry.attempts_total,
                untried: entry.untried,
                ok: false,
                story: entry.story,
                id,
            });
        }

        Progress {
            active,
            finished,
            shelf,
            free_space: gigabytes(free_bytes),
            free_bytes,
            total_space: gigabytes(total_bytes),
            total_bytes,
            problem: self.server_trouble.lock().expect("not poisoned").clone(),
        }
    }

    /// Her films, as they are on the disk right now. Works even when the downloader does not.
    fn shelf(&self) -> Vec<Shelved> {
        self.library
            .all()
            .into_iter()
            .filter(|(_, entry)| entry.present())
            .map(|(id, entry)| Shelved {
                id,
                title: entry.title,
                year: entry.year,
                cover_url: entry.cover_url,
                subtitle_note: entry.subtitle_note,
                languages: entry.info,
                series: entry.series,
            })
            .collect()
    }

    /// What the tray icon says on hover: whether anything is coming down, how far along it is
    /// and how fast, without opening the window.
    pub fn tray_report(&self) -> TrayReport {
        let Ok(queue) = self.downloader.queue() else {
            return TrayReport {
                tooltip: "Mamá Cine".to_string(),
                summary: "Mamá Cine".to_string(),
            };
        };
        if queue.is_empty() {
            return TrayReport {
                tooltip: "Mamá Cine · No se está descargando nada".to_string(),
                summary: "No se está descargando nada".to_string(),
            };
        }
        let rate = self.downloader.download_rate().unwrap_or(0);
        let speed = if queue.iter().any(|item| item.status == Status::Downloading) {
            speed_in_words(rate)
        } else {
            String::new()
        };
        let lines: Vec<String> = queue
            .iter()
            .map(|item| self.tray_item(item, rate))
            .collect();
        let header = if speed.is_empty() {
            "Mamá Cine".to_string()
        } else {
            format!("Mamá Cine · {speed}")
        };
        let summary = match lines.len() {
            1 => lines[0].clone(),
            several if speed.is_empty() => format!("{several} descargas en marcha"),
            several => format!("{several} descargas · {speed}"),
        };
        TrayReport {
            tooltip: tray_tooltip(&header, &lines),
            summary,
        }
    }

    fn tray_item(&self, item: &QueueItem, rate: u64) -> String {
        let title = self
            .library
            .get(item.id)
            .map(|entry| entry.title)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| item.name.clone());
        let title = shortened(&title, TRAY_TITLE_LIMIT);
        match item.status {
            Status::Downloading => {
                let left = remaining_in_words(item.remaining_mb, rate);
                let percent = format!("{:.0} %", item.percent());
                if left.is_empty() {
                    format!("{title} · {percent}")
                } else {
                    format!("{title} · {percent} · {left}")
                }
            }
            Status::Paused => format!("{title} · En pausa ({:.0} %)", item.percent()),
            Status::Queued => format!("{title} · Empezando la descarga"),
            Status::Verifying => format!("{title} · Comprobando que está completa"),
            Status::Repairing => format!("{title} · Arreglando lo que falta"),
            Status::Unpacking => format!("{title} · Casi lista"),
            Status::Moving => format!("{title} · Guardando"),
            Status::Finishing | Status::Other(_) => format!("{title} · Últimos detalles"),
        }
    }

    // --- her films ---------------------------------------------------------------

    /// To the recycle bin, and the shelf agrees with the disk on the next poll.
    pub fn remove(&self, id: i64) -> Result<(), String> {
        let entry = self
            .library
            .get(id)
            .ok_or_else(|| "Esa película ya no está en este ordenador.".to_string())?;
        let folder = entry
            .folder
            .filter(|folder| folder.exists())
            .ok_or_else(|| "Esa película ya no está en este ordenador.".to_string())?;
        self.remover.remove(&folder)?;
        self.library.note(
            id,
            "La has enviado a la papelera. Desde ahí todavía la puedes recuperar.",
            "sent to the recycle bin",
        );
        Ok(())
    }

    /// The library first: it remembers where a film is long after nzbget's history has rolled over.
    pub fn folder_of(&self, id: i64) -> Result<PathBuf, String> {
        if let Some(folder) = self
            .library
            .get(id)
            .and_then(|entry| entry.folder)
            .filter(|folder| folder.exists())
        {
            return Ok(folder);
        }
        let history = self
            .downloader
            .history()
            .map_err(|failure| messages::explain(&failure).said)?;
        let entry = history
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "Esa película ya no está en este ordenador.".to_string())?;
        entry
            .directory
            .map(PathBuf::from)
            .filter(|folder| folder.exists())
            .ok_or_else(|| "Esa película ya no está en este ordenador.".to_string())
    }

    pub fn film_file(&self, id: i64) -> Result<PathBuf, String> {
        let remembered = self
            .library
            .get(id)
            .and_then(|entry| entry.file)
            .filter(|file| file.exists());
        match remembered {
            Some(file) => Ok(file),
            None => crate::finishing::largest_video(&self.folder_of(id)?)
                .ok_or_else(|| "No se encuentra el archivo de la película.".to_string()),
        }
    }

    /// The episodes of a season, named as she counts them, so a folder of release-named files
    /// never has to be opened by hand. Each one says whether it has subtitles she can read: "en 2
    /// de 12 episodios" named no episode, and this screen is where naming one is worth something.
    pub fn episodes(&self, id: i64) -> Result<Vec<EpisodeRow>, String> {
        let folder = self.folder_of(id)?;
        let beside = crate::finishing::subtitles_in(&folder);
        let named = self.named_episodes(id);
        Ok(self
            .episode_files(id)?
            .into_iter()
            .map(|(label, numbered, path)| {
                let known = numbered.and_then(|(season, number)| {
                    named
                        .iter()
                        .find(|episode| episode.season == season && episode.number == number)
                });
                EpisodeRow {
                    subtitles: crate::finishing::has_subtitles(
                        &path,
                        &beside,
                        &self.subtitle_language,
                    ),
                    label,
                    title: known.and_then(|episode| episode.title.clone()),
                    overview: known.and_then(|episode| episode.overview.clone()),
                    season: numbered.map(|(season, _)| season),
                    number: numbered.map(|(_, number)| number),
                }
            })
            .collect())
    }

    /// The episodes of a season she already owns, as the show database names them. Everything is
    /// optional: a season from before the app kept the show's ids, a provider with nothing to say
    /// and a provider that cannot be reached all answer the same way, and the screen stands
    /// without names exactly as it stands without a synopsis.
    fn named_episodes(&self, id: i64) -> Vec<Episode> {
        let Some((show, first, last)) = self.identified(id) else {
            return Vec::new();
        };
        let remembered = format!("{show:?}:{first}-{last}");
        if let Some(known) = self.listed.lock().expect("not poisoned").get(&remembered) {
            return known.clone();
        }
        let episodes = match self.suggestions.episodes(&show, first, last) {
            Ok(episodes) => episodes,
            Err(failure) => {
                self.log
                    .line(&format!("episodes: {}", messages::explain(&failure).why));
                Vec::new()
            }
        };
        self.listed
            .lock()
            .expect("not poisoned")
            .insert(remembered, episodes.clone());
        episodes
    }

    /// Which show a season she owns is, and which seasons its folder holds.
    ///
    /// A season downloaded before the app kept the show's ids, and one whose search never
    /// identified the show, arrive here knowing neither, and a folder full of numbered episodes is
    /// all her screen can then say. Both questions have an answer that does not depend on a search
    /// that is long gone: the files on the disk state which seasons they are, and the show
    /// database answers to the name on the card. Asked once and written back, because a folder
    /// does not change its mind about which show it is.
    fn identified(&self, id: i64) -> Option<(ShowIds, u32, u32)> {
        let entry = self.library.get(id)?;
        let (first, last) = match entry.seasons {
            Some(seasons) => seasons,
            None => self.seasons_on_disk(id)?,
        };
        let show = match entry.show.any() {
            true => entry.show.clone(),
            false => self.identify_show(&mamacine_core::series::show_of(&entry.title))?,
        };
        if entry.seasons.is_none() || !entry.show.any() {
            let learned = show.clone();
            self.library.update(id, move |entry| {
                if entry.imdb.is_none() {
                    entry.imdb = learned.imdb.clone();
                }
                entry.show = learned;
                entry.seasons = Some((first, last));
            });
        }
        Some((show, first, last))
    }

    /// Which seasons the folder holds, from the names of the files in it. What is on the disk is
    /// the one answer about her own copy that cannot be out of date.
    fn seasons_on_disk(&self, id: i64) -> Option<(u32, u32)> {
        let numbered: Vec<u32> = self
            .episode_files(id)
            .ok()?
            .into_iter()
            .filter_map(|(_, numbered, _)| numbered.map(|(season, _)| season))
            .collect();
        Some((*numbered.iter().min()?, *numbered.iter().max()?))
    }

    /// The show behind the name on her card, asked of the database that names the episodes.
    ///
    /// Not the suggestion list she picks from as she types: that is ordered for someone who is
    /// there to choose, and nobody is here. The providers answer this one directly, and a name
    /// they cannot place leaves the season numbered rather than named after another programme.
    fn identify_show(&self, name: &str) -> Option<ShowIds> {
        if name.trim().is_empty() {
            return None;
        }
        let show = match self.suggestions.show_named(name) {
            Ok(show) => show,
            Err(failure) => {
                self.log.line(&format!(
                    "identifying {name}: {}",
                    messages::explain(&failure).why
                ));
                return None;
            }
        };
        if !show.any() {
            self.log
                .line(&format!("identifying {name}: no show by that name"));
            return None;
        }
        self.log.line(&format!("{name} identified as {show:?}"));
        Some(show)
    }

    pub fn episode_file(&self, id: i64, position: usize) -> Result<PathBuf, String> {
        self.episode_files(id)?
            .into_iter()
            .nth(position)
            .map(|(_, _, path)| path)
            .ok_or_else(|| "Ese episodio ya no está en la carpeta.".to_string())
    }

    /// Each video in the folder with the season and episode its own name states, which is what
    /// the show database's list can be matched against. A file whose name says neither keeps its
    /// place in the folder as its only name.
    fn episode_files(&self, id: i64) -> Result<Vec<EpisodeFile>, String> {
        use mamacine_core::series::episode_of;
        let folder = self.folder_of(id)?;
        let several = self
            .library
            .get(id)
            .and_then(|entry| entry.seasons)
            .map(|(first, last)| first != last)
            .unwrap_or(false);
        let videos = crate::finishing::all_videos(&folder);
        Ok(videos
            .into_iter()
            .enumerate()
            .map(|(position, path)| {
                let numbered = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(episode_of);
                let label = match numbered {
                    Some((season, episode)) if several => {
                        format!("Temporada {season} · Episodio {episode}")
                    }
                    Some((_, episode)) => format!("Episodio {episode}"),
                    None => format!("Vídeo {}", position + 1),
                };
                (label, numbered, path)
            })
            .collect())
    }

    /// The episodes a season holds, asked of the show database once and remembered. A search that
    /// identified no show, and a provider with nothing to say, answer with an empty list: the
    /// screen stands without it, exactly as it does without a synopsis.
    pub fn season_episodes(&self, index: usize) -> Result<Vec<Episode>, String> {
        let Some(season) = self
            .seasons
            .lock()
            .expect("not poisoned")
            .get(index)
            .map(|season| (season.first, season.last))
        else {
            return Ok(Vec::new());
        };
        let show = self.searched_show.lock().expect("not poisoned").clone();
        if !show.any() {
            return Ok(Vec::new());
        }
        let (first, last) = season;
        let remembered = format!("{show:?}:{first}-{last}");
        if let Some(known) = self.listed.lock().expect("not poisoned").get(&remembered) {
            return Ok(known.clone());
        }
        let episodes = match self.suggestions.episodes(&show, first, last) {
            Ok(episodes) => episodes,
            // the list is garnish: what she came for is the season, and it is still there
            Err(failure) => {
                self.log
                    .line(&format!("episodes: {}", messages::explain(&failure).why));
                Vec::new()
            }
        };
        self.listed
            .lock()
            .expect("not poisoned")
            .insert(remembered, episodes.clone());
        Ok(episodes)
    }

    /// Where IMDb keeps this season: its own page lists every episode, which is more than the app
    /// can say about one.
    pub fn imdb_season_of(&self, index: usize) -> Result<(String, u32), String> {
        let first = self
            .seasons
            .lock()
            .expect("not poisoned")
            .get(index)
            .map(|season| season.first)
            .ok_or_else(|| "Esa temporada ya no está en los resultados.".to_string())?;
        let imdb = self
            .searched_show
            .lock()
            .expect("not poisoned")
            .imdb
            .clone()
            .ok_or_else(|| "Esta serie no tiene ficha.".to_string())?;
        Ok((imdb, first))
    }

    pub fn imdb_of(&self, index: usize) -> Result<String, String> {
        self.found
            .lock()
            .expect("not poisoned")
            .get(index)
            .and_then(|film| film.imdb.clone())
            .ok_or_else(|| "Esta película no tiene ficha.".to_string())
    }

    /// What the film is about, asked of the film database once and remembered. A film without an
    /// IMDb id, or a provider without words, answers nothing rather than an error: the ficha is
    /// garnish and the screen stands without it.
    pub fn synopsis(&self, index: usize) -> Result<String, String> {
        let Ok(id) = self.imdb_of(index) else {
            return Ok(String::new());
        };
        self.synopsis_of(&id)
    }

    /// What something she already owns is about, for its own page: asked by the id kept with it,
    /// rather than by a place in a list of results that may be several searches old.
    pub fn library_synopsis(&self, id: i64) -> Result<String, String> {
        match self.library.get(id).and_then(|entry| entry.imdb) {
            Some(imdb) => self.synopsis_of(&imdb),
            None => Ok(String::new()),
        }
    }

    fn synopsis_of(&self, id: &str) -> Result<String, String> {
        // the same normalised id the IMDb page is opened with
        let imdb = format!(
            "tt{:0>7}",
            id.trim().trim_start_matches("tt").trim_start_matches('0')
        );
        if let Some(known) = self.synopses.lock().expect("not poisoned").get(&imdb) {
            return Ok(known.clone());
        }
        let words = self
            .suggestions
            .synopsis(&imdb)
            .map_err(|failure| {
                // the window treats a missing ficha as garnish and says nothing, so a key the
                // database rejects would otherwise be invisible from both sides
                let said = messages::explain(&failure);
                self.log.line(&format!("synopsis {imdb}: {}", said.why));
                said.said
            })?
            .unwrap_or_default();
        self.synopses
            .lock()
            .expect("not poisoned")
            .insert(imdb, words.clone());
        Ok(words)
    }

    // --- images ------------------------------------------------------------------

    /// The window is not allowed to reach the internet; images come back through here as data,
    /// fetched once and kept, because the grid asks on every render.
    pub fn image(&self, url: &str) -> Result<String, String> {
        if let Some(found) = self.covers.lock().expect("not poisoned").get(url) {
            return Ok(found.clone());
        }
        // a suggestion poster comes from the film database; cover art from the indexer it names
        let poster_host = host_of(url)
            .map(|host| host.ends_with("media-amazon.com") || host == "image.tmdb.org")
            .unwrap_or(false);
        let fetched = if poster_host {
            self.suggestions.poster(url)
        } else {
            self.indexer_for(url)
                .ok_or_else(|| {
                    mamacine_core::error::Error::Setup(
                        "Esa imagen viene de un buscador que ya no está en los ajustes.".into(),
                    )
                })
                .and_then(|indexer| indexer.cover(url))
        };
        let (kind, bytes) = fetched.map_err(|failure| messages::explain(&failure).said)?;
        let encoded = format!(
            "data:{kind};base64,{}",
            mamacine_core::nzbget::base64(&bytes)
        );
        self.covers
            .lock()
            .expect("not poisoned")
            .insert(url.to_string(), encoded.clone());
        Ok(encoded)
    }
}

// --- words and arithmetic --------------------------------------------------------

/// The words she reads, with the resolution they stand for beside them: the number is real
/// information, and hiding it behind a friendly word is deciding what she may know.
fn definition_of(release: &SearchResult) -> &'static str {
    let title = release.title.to_lowercase();
    if title.contains("2160p") || title.contains("4k") {
        "4K"
    } else if title.contains("1080p") {
        "Alta definición (1080p)"
    } else if title.contains("720p") {
        "Buena calidad (720p)"
    } else {
        "Calidad normal"
    }
}

fn size_of(release: &SearchResult) -> String {
    gigabytes(release.size_bytes)
}

/// The chip a copy belongs under. Dual counts as Spanish for the same bet `matches` makes.
fn voice_of(release: &SearchResult) -> &'static str {
    use mamacine_core::release::Tag;
    let has = |tag: Tag| release.tags.contains(&tag);
    if has(Tag::Spanish) || (has(Tag::Dual) && !has(Tag::Latino) && !has(Tag::OtherLanguage)) {
        "es"
    } else if has(Tag::Latino) {
        "latino"
    } else {
        "original"
    }
}

fn language_of(release: &SearchResult) -> String {
    use mamacine_core::release::Tag;
    let has = |tag: Tag| release.tags.contains(&tag);
    if has(Tag::Spanish) {
        "Español".into()
    } else if has(Tag::Latino) {
        "Español latino".into()
    } else if has(Tag::Dual) {
        "Dos idiomas".into()
    } else if has(Tag::Subbed) {
        "Original con subtítulos".into()
    } else {
        "Versión original".into()
    }
}

/// How much this entry looks like what she typed. The card's display title comes from the film
/// database and may be in another language ("Champions" for a "campeones" search), so the release
/// names, which carry the title the thing was actually filed under, get a vote too.
/// How much a card looks like the name the question actually asked for. Only a loose question is
/// judged this way: an indexer given words can free-associate, and one given an id cannot.
fn looks_like(asked: &str, name: &str, releases: &[SearchResult]) -> f64 {
    releases
        .iter()
        .take(5)
        .map(|release| mamacine_core::search::relevance(asked, &release.title))
        .fold(mamacine_core::search::relevance(asked, name), f64::max)
}

/// The same words, whatever she capitalized and whichever accents she reached for.
fn same_text(one: &str, other: &str) -> bool {
    let plain = |text: &str| mamacine_core::search::fold(text.trim()).to_lowercase();
    plain(one) == plain(other)
}

/// The same rule that will later refuse the download, asked while she is still deciding.
fn room_of(free: Option<u64>, size: u64) -> &'static str {
    use mamacine_core::space::{room_for, Room};
    let Some(free) = free else { return "fits" };
    if size == 0 {
        return "fits";
    }
    match room_for(free, size) {
        Room::Fits => "fits",
        Room::Tight => "tight",
        Room::NotEnough => "no",
    }
}

fn minutes_for(size_bytes: u64, rate: u64) -> Option<i64> {
    if rate == 0 || size_bytes == 0 {
        return None;
    }
    Some(((size_bytes as f64 / rate as f64) / 60.0).ceil() as i64)
}

/// The chosen copy first, then every other one, best first: the ones behind it are the plan for
/// when this one turns out to be dead.
fn candidates_from(releases: &[SearchResult], version: Option<usize>) -> Vec<SearchResult> {
    let chosen = version.unwrap_or(0);
    let mut ordered = Vec::with_capacity(releases.len());
    let voice = releases.get(chosen).map(voice_of);
    if let Some(first) = releases.get(chosen) {
        ordered.push(first.clone());
    }
    // her choice of copy is also a choice of language: when it dies, the next copy in the same
    // voice comes first, and a silent switch to another language comes last
    let rest = releases
        .iter()
        .enumerate()
        .filter(|(position, _)| *position != chosen);
    let (same_voice, other): (Vec<_>, Vec<_>) =
        rest.partition(|(_, release)| Some(voice_of(release)) == voice);
    ordered.extend(same_voice.into_iter().map(|(_, release)| release.clone()));
    ordered.extend(other.into_iter().map(|(_, release)| release.clone()));
    ordered
}

/// Whether this download ended because somebody meant it to, rather than because the copy was bad.
/// nzbget's own words: `DELETED/MANUAL` is her pressing cancel, `DELETED/COPY` is its duplicate
/// check, which the app turns off but which an older configuration on her machine may still have.
fn abandoned_deliberately(status: &str) -> bool {
    status.contains("MANUAL") || status.contains("COPY")
}

/// The same moment, for whoever is fixing the app rather than watching a film.
fn explain_failure(item: &mamacine_core::nzbget::HistoryItem) -> String {
    if item.failed_articles > 0 {
        format!(
            "{}: faltaban {} de {} partes, salud {:.1}%",
            item.status, item.failed_articles, item.total_articles, item.health_percent
        )
    } else {
        format!("{}: no se pudo preparar", item.status)
    }
}

fn gigabytes(bytes: u64) -> String {
    let value = bytes as f64 / 1_073_741_824.0;
    if value >= 100.0 {
        format!("{value:.0} GB")
    } else {
        // the interface speaks es-ES, where the decimal separator is a comma
        format!("{value:.1} GB").replace('.', ",")
    }
}

/// Minutes she can plan around, not bytes a second.
fn remaining_in_words(remaining_mb: i64, rate_bytes: u64) -> String {
    if rate_bytes == 0 || remaining_mb <= 0 {
        return String::new();
    }
    let seconds = (remaining_mb as f64 * 1_048_576.0) / rate_bytes as f64;
    if seconds < 90.0 {
        "Menos de un minuto".to_string()
    } else {
        format!("Unos {} minutos", (seconds / 60.0).round() as i64)
    }
}

/// Bytes a second, in the units every download in the world shows. Only for a running download:
/// a number that sits at zero next to a stalled bar reads as a fault of its own.
fn speed_in_words(rate_bytes: u64) -> String {
    match rate_bytes {
        0 => String::new(),
        rate if rate >= 1_048_576 => format!("{:.0} MB/s", rate as f64 / 1_048_576.0),
        rate => format!("{:.0} KB/s", rate as f64 / 1024.0),
    }
}

/// Windows truncates a tray tooltip past 127 characters, so it is built to fit: the header, then
/// as many films as are left room for, then a count of the ones that were not.
const TRAY_TOOLTIP_LIMIT: usize = 127;
const TRAY_TITLE_LIMIT: usize = 34;

fn tray_tooltip(header: &str, items: &[String]) -> String {
    let mut shown = items.len();
    loop {
        let mut lines = vec![header.to_string()];
        lines.extend(items.iter().take(shown).cloned());
        if shown < items.len() {
            lines.push(format!("y {} más", items.len() - shown));
        }
        let tooltip = lines.join("\n");
        if shown == 0 || tooltip.chars().count() <= TRAY_TOOLTIP_LIMIT {
            return tooltip;
        }
        shown -= 1;
    }
}

fn shortened(title: &str, limit: usize) -> String {
    if title.chars().count() <= limit {
        return title.to_string();
    }
    let kept: String = title.chars().take(limit.saturating_sub(1)).collect();
    format!("{}\u{2026}", kept.trim_end())
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    Some(rest.split('/').next()?.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mamacine_core::error::{Error, Result};
    use mamacine_core::nzbget::HistoryItem;
    use std::sync::atomic::{AtomicI64, Ordering};

    // --- the fakes ---------------------------------------------------------------

    struct FakeIndexer {
        results: Vec<SearchResult>,
        fails: bool,
        nzb_requests: Mutex<Vec<String>>,
        asked: Arc<Mutex<Vec<Query>>>,
    }

    impl Indexer for FakeIndexer {
        fn search(&self, query: &Query, category: Option<Category>) -> Result<Vec<SearchResult>> {
            self.asked.lock().expect("not poisoned").push(query.clone());
            if self.fails {
                return Err(Error::Refused {
                    what: "the indexer".into(),
                    status: 429,
                    message: "too many requests".into(),
                });
            }
            // television answers with packs, film with films; the fake keeps it simple and
            // answers everything, letting the grouping sort it out
            let _ = category;
            Ok(self.results.clone())
        }
        fn capabilities(&self) -> Result<String> {
            Ok("ready".into())
        }
        fn fetch_nzb(&self, url: &str) -> Result<Vec<u8>> {
            self.nzb_requests
                .lock()
                .expect("not poisoned")
                .push(url.to_string());
            // realistic enough to probe: a hundred data articles and a sliver of par2
            let segments: String = (0..100)
                .map(|n| format!(r#"<segment bytes="1000" number="{n}">id-{n}@x</segment>"#))
                .collect();
            Ok(format!(
                r#"<nzb><file subject="film.rar"><segments>{segments}</segments></file>
                   <file subject="film.vol0+1.PAR2"><segments>
                   <segment bytes="6000" number="1">par@x</segment></segments></file></nzb>"#
            )
            .into_bytes())
        }
        fn cover(&self, _url: &str) -> Result<(String, Vec<u8>)> {
            Ok(("image/jpeg".into(), vec![1, 2, 3]))
        }
        fn host(&self) -> Option<String> {
            Some("indexer.test".into())
        }
    }

    struct FakeDownloader {
        queue: Mutex<Vec<QueueItem>>,
        history: Mutex<Vec<HistoryItem>>,
        appended: Mutex<Vec<String>>,
        cancelled: Mutex<Vec<i64>>,
        next_id: AtomicI64,
        check: Mutex<ServerCheck>,
        unreachable: Mutex<bool>,
        append_fails: Mutex<bool>,
        forgotten: Mutex<Vec<i64>>,
        rate: Mutex<u64>,
    }

    impl Default for FakeDownloader {
        fn default() -> FakeDownloader {
            FakeDownloader {
                queue: Mutex::new(Vec::new()),
                history: Mutex::new(Vec::new()),
                appended: Mutex::new(Vec::new()),
                cancelled: Mutex::new(Vec::new()),
                next_id: AtomicI64::new(0),
                check: Mutex::new(ServerCheck::Working),
                unreachable: Mutex::new(false),
                append_fails: Mutex::new(false),
                forgotten: Mutex::new(Vec::new()),
                rate: Mutex::new(0),
            }
        }
    }

    impl Downloader for FakeDownloader {
        fn append(&self, name: &str, _nzb: &[u8]) -> Result<i64> {
            if *self.append_fails.lock().expect("not poisoned") {
                return Err(Error::Unreachable {
                    what: "nzbget".into(),
                    detail: "connection refused".into(),
                });
            }
            self.appended
                .lock()
                .expect("not poisoned")
                .push(name.to_string());
            Ok(self.next_id.fetch_add(1, Ordering::SeqCst) + 1)
        }
        fn queue(&self) -> Result<Vec<QueueItem>> {
            if *self.unreachable.lock().expect("not poisoned") {
                return Err(Error::Unreachable {
                    what: "nzbget".into(),
                    detail: "connection refused".into(),
                });
            }
            Ok(self.queue.lock().expect("not poisoned").clone())
        }
        fn history(&self) -> Result<Vec<HistoryItem>> {
            if *self.unreachable.lock().expect("not poisoned") {
                return Err(Error::Unreachable {
                    what: "nzbget".into(),
                    detail: "connection refused".into(),
                });
            }
            Ok(self.history.lock().expect("not poisoned").clone())
        }
        fn download_rate(&self) -> Result<u64> {
            Ok(*self.rate.lock().expect("not poisoned"))
        }
        fn cancel(&self, id: i64) -> Result<()> {
            self.cancelled.lock().expect("not poisoned").push(id);
            Ok(())
        }
        fn forget(&self, id: i64) -> Result<()> {
            self.forgotten.lock().expect("not poisoned").push(id);
            self.history
                .lock()
                .expect("not poisoned")
                .retain(|item| item.id != id);
            Ok(())
        }
        fn check_server(&self, _news: &NewsServer) -> ServerCheck {
            self.check.lock().expect("not poisoned").clone()
        }
    }

    struct FakeDisk(u64);
    impl Disk for FakeDisk {
        fn space(&self, _path: &Path) -> Option<crate::disk::Space> {
            Some(crate::disk::Space {
                free: self.0,
                total: 500 * GIGABYTE,
            })
        }
    }

    #[derive(Default)]
    struct FakeRemover {
        removed: Mutex<Vec<PathBuf>>,
    }
    impl Remover for FakeRemover {
        fn remove(&self, folder: &Path) -> std::result::Result<(), String> {
            self.removed
                .lock()
                .expect("not poisoned")
                .push(folder.to_path_buf());
            std::fs::remove_dir_all(folder).map_err(|failure| failure.to_string())
        }
    }

    struct FakeProber {
        answers: std::sync::Arc<Mutex<Vec<f64>>>,
        bodies: std::sync::Arc<Mutex<Vec<Vec<u8>>>>,
    }
    impl mamacine_core::nntp::Prober for FakeProber {
        fn statuses(&self, _news: &NewsServer, ids: &[&str]) -> Result<Vec<bool>> {
            let mut answers = self.answers.lock().expect("not poisoned");
            let ratio = if answers.is_empty() {
                0.0
            } else {
                answers.remove(0)
            };
            let gone = (ids.len() as f64 * ratio).round() as usize;
            Ok((0..ids.len()).map(|n| n < gone).collect())
        }
        fn fetch_body(&self, _news: &NewsServer, _id: &str) -> Result<Vec<u8>> {
            let mut bodies = self.bodies.lock().expect("not poisoned");
            if bodies.is_empty() {
                Err(Error::Unreachable {
                    what: "the fake".into(),
                    detail: "no body scripted".into(),
                })
            } else {
                Ok(bodies.remove(0))
            }
        }
    }

    /// A title provider that behaves as the real ones do: a name in any language matches the one
    /// thing it names, and it answers under the name that thing is released with.
    struct NoSuggestions;

    impl NoSuggestions {
        fn catalogue() -> Vec<(Vec<&'static str>, Suggestion)> {
            let title =
                |id: &str, name: &str, original: Option<&str>, year: &str, series| Suggestion {
                    id: id.into(),
                    title: name.into(),
                    original: original.map(str::to_string),
                    year: Some(year.into()),
                    series,
                    poster_url: None,
                };
            vec![
                (
                    vec!["el hoyo", "the platform"],
                    title("8228288", "El hoyo", Some("The Platform"), "2019", false),
                ),
                (
                    vec!["gomorra", "gomorrah"],
                    title("2049116", "Gomorrah", None, "2014", true),
                ),
                // the film of the same name, offered after the series: "gomorra" means the show
                (
                    vec!["gomorra", "gomorrah"],
                    title("0929425", "Gomorrah", None, "2008", false),
                ),
                (
                    vec!["juego de tronos", "game of thrones"],
                    title("0944947", "Game of Thrones", None, "2011", true),
                ),
                (
                    vec!["el castillo ambulante", "howl's moving castle"],
                    title(
                        "0347149",
                        "El castillo ambulante",
                        Some("ハウルの動く城"),
                        "2004",
                        false,
                    ),
                ),
            ]
        }
    }

    impl Suggest for NoSuggestions {
        fn suggest(&self, text: &str) -> Result<Vec<Suggestion>> {
            Ok(NoSuggestions::catalogue()
                .into_iter()
                .filter(|(known, _)| known.iter().any(|name| same_text(name, text)))
                .map(|(_, suggestion)| suggestion)
                .collect())
        }
        /// A real provider asks a show database which show this is; the fake knows.
        fn resolve(&self, suggestion: &Suggestion) -> Result<Picked> {
            let picked = mamacine_core::lookup::resolve(suggestion);
            if !picked.series {
                return Ok(picked);
            }
            Ok(Picked {
                show: mamacine_core::indexer::ShowIds {
                    tvdb: Some("281342".into()),
                    ..picked.show
                },
                ..picked
            })
        }
        /// The real providers answer this outright, and so does the fake: one name, one show.
        fn show_named(&self, name: &str) -> Result<ShowIds> {
            let found = NoSuggestions::catalogue()
                .into_iter()
                .find(|(known, entry)| {
                    entry.series && known.iter().any(|known| same_text(known, name))
                });
            match found {
                Some((_, entry)) => Ok(mamacine_core::indexer::ShowIds {
                    tvdb: Some("281342".into()),
                    imdb: Some(format!("tt{}", entry.id)),
                    ..ShowIds::default()
                }),
                None => Ok(ShowIds::default()),
            }
        }
        fn episodes(&self, _show: &ShowIds, first: u32, last: u32) -> Result<Vec<Episode>> {
            Ok((first..=last)
                .flat_map(|season| {
                    (1..=2).map(move |number| Episode {
                        season,
                        number,
                        title: Some(format!("Episodio {number} de la {season}")),
                        overview: Some(format!("Lo que pasa en el {number}.")),
                    })
                })
                .collect())
        }
        fn poster(&self, _url: &str) -> Result<(String, Vec<u8>)> {
            Err(Error::Setup("no posters in tests".into()))
        }
    }

    struct CountingFicha {
        asked: Arc<Mutex<Vec<String>>>,
    }
    impl Suggest for CountingFicha {
        fn suggest(&self, _text: &str) -> Result<Vec<Suggestion>> {
            Ok(Vec::new())
        }
        fn resolve(&self, suggestion: &Suggestion) -> Result<Picked> {
            Ok(mamacine_core::lookup::resolve(suggestion))
        }
        fn poster(&self, _url: &str) -> Result<(String, Vec<u8>)> {
            Err(Error::Setup("no posters in tests".into()))
        }
        fn synopsis(&self, imdb: &str) -> Result<Option<String>> {
            self.asked
                .lock()
                .expect("not poisoned")
                .push(imdb.to_string());
            Ok(Some("Una historia.".into()))
        }
    }

    // --- scaffolding -------------------------------------------------------------

    const GIGABYTE: u64 = 1_073_741_824;

    fn release(title: &str, size_gb: f64, grabs: u64) -> SearchResult {
        SearchResult {
            tags: mamacine_core::release::tags(title),
            title: title.to_string(),
            nzb_url: format!("https://indexer.test/{}", title.replace(' ', ".")),
            size_bytes: (size_gb * GIGABYTE as f64) as u64,
            age_days: Some(300.0),
            grabs,
            cover_url: None,
            imdb: Some("0082096".into()),
            about: "Das Boot · 1981 · ★8.4 · Drama, War · 149 min".into(),
            thumbs_up: 0,
            thumbs_down: 0,
        }
    }

    fn failed(id: i64, name: &str) -> HistoryItem {
        HistoryItem {
            id,
            name: name.into(),
            succeeded: false,
            status: "FAILURE/HEALTH".into(),
            directory: None,
            size_mb: 2000,
            total_articles: 6000,
            failed_articles: 300,
            health_percent: 93.8,
        }
    }

    struct World {
        orchestrator: Orchestrator,
        downloader: Arc<FakeDownloader>,
        notified: Arc<Mutex<Vec<(String, String)>>>,
        prober_answers: Arc<Mutex<Vec<f64>>>,
        prober_bodies: Arc<Mutex<Vec<Vec<u8>>>>,
        directory: PathBuf,
        /// Every question the indexer was asked, so a test can check what was asked and not only
        /// what came back.
        asked: Arc<Mutex<Vec<Query>>>,
    }

    fn world_with(releases: Vec<SearchResult>, free: u64) -> World {
        let directory = std::env::temp_dir().join(format!(
            "mama-cine-orchestrator-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a scratch folder");
        let log = Arc::new(Log::open(&directory));
        let downloader = Arc::new(FakeDownloader::default());
        let notified = Arc::new(Mutex::new(Vec::new()));
        let prober_answers: Arc<Mutex<Vec<f64>>> = Arc::new(Mutex::new(Vec::new()));
        let prober_bodies: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&notified);

        struct SharedDownloader(Arc<FakeDownloader>);
        impl Downloader for SharedDownloader {
            fn append(&self, name: &str, nzb: &[u8]) -> Result<i64> {
                self.0.append(name, nzb)
            }
            fn queue(&self) -> Result<Vec<QueueItem>> {
                self.0.queue()
            }
            fn history(&self) -> Result<Vec<HistoryItem>> {
                self.0.history()
            }
            fn download_rate(&self) -> Result<u64> {
                self.0.download_rate()
            }
            fn cancel(&self, id: i64) -> Result<()> {
                self.0.cancel(id)
            }
            fn forget(&self, id: i64) -> Result<()> {
                self.0.forget(id)
            }
            fn check_server(&self, news: &NewsServer) -> ServerCheck {
                self.0.check_server(news)
            }
        }

        let asked: Arc<Mutex<Vec<Query>>> = Arc::new(Mutex::new(Vec::new()));
        let orchestrator = Orchestrator::new(Pieces {
            indexers: vec![(
                "Test".to_string(),
                Box::new(FakeIndexer {
                    results: releases,
                    fails: false,
                    nzb_requests: Mutex::new(Vec::new()),
                    asked: Arc::clone(&asked),
                }) as Box<dyn Indexer>,
            )],
            downloader: Box::new(SharedDownloader(Arc::clone(&downloader))),
            library: Arc::new(Library::open(&directory, Arc::clone(&log))),
            log,
            destination: directory.join("films"),
            news: NewsServer {
                host: "news.test".into(),
                port: 563,
                username: "reader".into(),
                password: "secret".into(),
                encrypted: true,
                connections: 8,
                retention_days: 0,
            },
            preference: Preference::Any,
            subtitle_language: "es".into(),
            disk: Box::new(FakeDisk(free)),
            remover: Box::new(FakeRemover::default()),
            suggestions: Box::new(NoSuggestions),
            prober: Box::new(FakeProber {
                answers: std::sync::Arc::clone(&prober_answers),
                bodies: std::sync::Arc::clone(&prober_bodies),
            }),
            notify: Box::new(move |title, body| {
                sink.lock()
                    .expect("not poisoned")
                    .push((title.to_string(), body.to_string()));
            }),
        });
        World {
            orchestrator,
            downloader,
            notified,
            prober_answers,
            prober_bodies,
            directory,
            asked,
        }
    }

    fn grab_first(world: &World) -> i64 {
        world
            .orchestrator
            .search("das boot", None, None)
            .expect("results")
            .films
            .first()
            .expect("a film");
        world
            .orchestrator
            .grab(0, None, false)
            .expect("a download")
            .id
    }

    // --- the behaviour -----------------------------------------------------------

    #[test]
    fn a_synopsis_is_asked_for_once_and_remembered() {
        let mut world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            200 * GIGABYTE,
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        world.orchestrator.suggestions = Box::new(CountingFicha {
            asked: Arc::clone(&asked),
        });
        world
            .orchestrator
            .search("das boot", None, None)
            .expect("results");

        assert_eq!(
            world.orchestrator.synopsis(0).expect("words"),
            "Una historia."
        );
        assert_eq!(
            world.orchestrator.synopsis(0).expect("words"),
            "Una historia."
        );
        assert_eq!(
            asked.lock().expect("not poisoned").as_slice(),
            ["tt0082096"],
            "asked once, by the normalised id"
        );
    }

    #[test]
    fn a_film_without_a_page_has_no_synopsis_and_no_error() {
        let mut world = world_with(
            vec![SearchResult {
                imdb: None,
                ..release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)
            }],
            200 * GIGABYTE,
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        world.orchestrator.suggestions = Box::new(CountingFicha {
            asked: Arc::clone(&asked),
        });
        world
            .orchestrator
            .search("das boot", None, None)
            .expect("results");

        assert_eq!(world.orchestrator.synopsis(0).expect("no words"), "");
        assert!(
            asked.lock().expect("not poisoned").is_empty(),
            "without an id there is nothing to ask"
        );
    }

    #[test]
    fn a_dead_copy_is_replaced_and_the_screen_is_never_told_it_failed() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        let first = grab_first(&world);

        world
            .downloader
            .history
            .lock()
            .expect("not poisoned")
            .push(failed(first, "Das Boot"));

        // before the chase has answered, the failure must read as "still working on it"
        let progress = world.orchestrator.progress();
        let landed = progress
            .finished
            .iter()
            .find(|film| film.id == first)
            .expect("in history");
        assert!(landed.retrying, "the flash of false failure is the old bug");
        assert!(!landed.ok);
        assert!(landed.detail.is_empty(), "{}", landed.detail);

        world.orchestrator.chase();
        let progress = world.orchestrator.progress();
        let landed = progress
            .finished
            .iter()
            .find(|film| film.id == first)
            .expect("in history");
        assert_eq!(landed.next_id, Some(first + 1), "the next copy took over");
        // the screen follows next_id, and the successor is owed the whole story
        let story = world
            .orchestrator
            .library
            .get(first + 1)
            .expect("the successor")
            .story;
        assert!(
            story.iter().any(|note| note.said.contains("descartado")),
            "the discard is a line of the story"
        );
        assert!(
            story.iter().any(|note| note.said.contains("otra copia")),
            "and so is the replacement"
        );
    }

    // A server problem is transient: it must never consume the film, never blame the copies,
    // and never end in an instruction with no next step. The chase waits, says so once, and
    // resumes on its own the moment the server answers — seen live when a false "server broken"
    // verdict abandoned a season over an RPC arity error.
    #[test]
    fn a_server_problem_pauses_the_chase_and_it_resumes_by_itself() {
        let mut world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        world.orchestrator.server_recheck = std::time::Duration::ZERO;
        let first = grab_first(&world);
        *world.downloader.check.lock().expect("not poisoned") =
            ServerCheck::Refused("502 Authentication Failed".into());
        world
            .downloader
            .history
            .lock()
            .expect("not poisoned")
            .push(failed(first, "Das Boot"));

        world.orchestrator.chase();
        world.orchestrator.chase(); // a second tick must not repeat the story line

        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            1,
            "no copy may be spent against a refusing account"
        );
        let entry = world.orchestrator.library.get(first).expect("remembered");
        assert!(!entry.gave_up, "a server problem never consumes the film");
        let refusals = entry
            .story
            .iter()
            .filter(|note| note.said == messages::SERVER_REFUSED)
            .count();
        assert_eq!(refusals, 1, "said once per outage, not once per tick");
        let progress = world.orchestrator.progress();
        assert_eq!(
            progress.problem.as_deref(),
            Some(messages::SERVER_REFUSED),
            "the banner names the real problem and what happens next"
        );
        assert_eq!(
            world.notified.lock().expect("not poisoned").len(),
            1,
            "she is told even if she walked away"
        );

        // the account is fixed: the chase notices on its own and carries on
        *world.downloader.check.lock().expect("not poisoned") = ServerCheck::Working;
        world.orchestrator.chase();
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            2,
            "the next copy starts without her doing anything"
        );
        let progress = world.orchestrator.progress();
        assert!(progress.problem.is_none(), "the banner clears itself");
        let entry = world.orchestrator.library.get(first).expect("remembered");
        assert!(
            entry
                .story
                .iter()
                .any(|note| note.said.contains("Ya puedo conectarme otra vez")),
            "the recovery is a line of the story too"
        );
    }

    #[test]
    fn the_chase_is_capped_and_the_message_does_not_overclaim() {
        let releases: Vec<SearchResult> = (0..7)
            .map(|n| {
                release(
                    &format!("Das Boot 1981 1080p BluRay x264-{n}"),
                    2.0,
                    900 - n,
                )
            })
            .collect();
        let world = world_with(releases, 200 * GIGABYTE);
        let mut id = grab_first(&world);

        for _ in 0..CHASE_LIMIT {
            world
                .downloader
                .history
                .lock()
                .expect("not poisoned")
                .push(failed(id, "Das Boot"));
            world.orchestrator.chase();
            id += 1;
        }

        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            CHASE_LIMIT,
            "three tries, then it stops"
        );
        let last = world
            .orchestrator
            .library
            .get(CHASE_LIMIT as i64)
            .expect("the last attempt");
        assert!(last.gave_up);
        let progress = world.orchestrator.progress();
        let landed = progress
            .finished
            .iter()
            .find(|film| film.id == CHASE_LIMIT as i64)
            .expect("in history");
        assert!(
            landed.detail.contains("he probado 3 copias"),
            "{}: the real count, hers to judge",
            landed.detail
        );
        assert!(
            landed.detail.contains("quedan 4 sin probar"),
            "{}: and what was left untried is not hidden",
            landed.detail
        );
    }

    #[test]
    fn when_no_copy_can_start_the_story_says_the_real_reason() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        let first = grab_first(&world);
        *world.downloader.append_fails.lock().expect("not poisoned") = true;
        world
            .downloader
            .history
            .lock()
            .expect("not poisoned")
            .push(failed(first, "Das Boot"));

        world.orchestrator.chase();

        let entry = world.orchestrator.library.get(first).expect("remembered");
        assert!(entry.gave_up);
        assert!(
            entry.story.iter().any(|note| note
                .said
                .contains("El descargador de la aplicación no responde")),
            "the story carries the reason, not a claim about damage: {:?}",
            entry
                .story
                .iter()
                .map(|note| &note.said)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_copy_that_does_not_fit_is_skipped_with_a_line_she_can_read() {
        let world = world_with(
            vec![
                release("Das Boot 1981 REMUX-HUGE", 60.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            20 * GIGABYTE,
        );
        // the huge one wins the version pick on purpose here
        let id = world
            .orchestrator
            .search("das boot", None, None)
            .ok()
            .and_then(|_| world.orchestrator.grab(0, None, false).ok())
            .expect("a download")
            .id;

        let entry = world.orchestrator.library.get(id).expect("remembered");
        let skipped = entry
            .story
            .iter()
            .any(|note| note.said.contains("no cabe en el disco"));
        // whichever release ranked first, a skip must never be silent
        if world
            .downloader
            .appended
            .lock()
            .expect("not poisoned")
            .len()
            == 1
            && entry.attempt > 1
        {
            assert!(skipped, "{:?}", entry.story);
        }
    }

    #[test]
    fn a_downloader_that_stopped_answering_is_said_plainly_and_the_shelf_survives() {
        let world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            200 * GIGABYTE,
        );
        let films = world.directory.join("films-here");
        std::fs::create_dir_all(&films).expect("a folder");
        world.orchestrator.library.put(
            9,
            Entry {
                title: "El Sur".into(),
                key: "imdb:86010".into(),
                settled: true,
                folder: Some(films),
                ..Entry::default()
            },
        );
        *world.downloader.unreachable.lock().expect("not poisoned") = true;

        let progress = world.orchestrator.progress();
        let problem = progress.problem.expect("said plainly");
        assert!(problem.contains("descargador"), "{problem}");
        assert!(!problem.contains("cannot"), "no english: {problem}");
        assert_eq!(
            progress.shelf.len(),
            1,
            "her films are not hostage to nzbget"
        );
    }

    // The detail screen offered "Descargar" for a season that was downloading at that moment.
    #[test]
    fn the_detail_screen_is_told_about_a_download_already_on_its_way() {
        let world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            200 * GIGABYTE,
        );
        let id = grab_first(&world);
        let owned = world.orchestrator.have(0, false);
        assert_eq!(owned.have, None, "not on the shelf yet");
        assert_eq!(owned.downloading, Some(id), "but very much on its way");
    }

    fn downloading(id: i64, name: &str, done_mb: i64, total_mb: i64) -> QueueItem {
        QueueItem {
            id,
            name: name.into(),
            status: mamacine_core::nzbget::Status::Downloading,
            downloaded_mb: done_mb,
            total_mb,
            remaining_mb: total_mb - done_mb,
        }
    }

    #[test]
    fn the_tray_says_what_is_coming_down_without_opening_the_window() {
        let world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            200 * GIGABYTE,
        );
        assert_eq!(
            world.orchestrator.tray_report().tooltip,
            "Mamá Cine · No se está descargando nada"
        );
        let id = grab_first(&world);
        *world.downloader.rate.lock().expect("not poisoned") = 5 * 1_048_576;
        world
            .downloader
            .queue
            .lock()
            .expect("not poisoned")
            .push(downloading(id, "Das Boot", 500, 1000));
        let report = world.orchestrator.tray_report();
        assert!(report.tooltip.contains("Das Boot"), "{}", report.tooltip);
        assert!(report.tooltip.contains("50 %"), "{}", report.tooltip);
        assert!(report.tooltip.contains("5 MB/s"), "{}", report.tooltip);
        assert!(
            report.tooltip.contains("Unos 2 minutos"),
            "{}",
            report.tooltip
        );
        assert_eq!(report.summary, "Das Boot · 50 % · Unos 2 minutos");
    }

    #[test]
    fn the_tray_says_how_far_along_each_of_several_downloads_is() {
        let world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            200 * GIGABYTE,
        );
        let id = grab_first(&world);
        *world.downloader.rate.lock().expect("not poisoned") = 2 * 1_048_576;
        let mut queue = world.downloader.queue.lock().expect("not poisoned");
        queue.push(downloading(id, "Das Boot", 250, 1000));
        queue.push(downloading(id + 1, "Sopa de ganso", 900, 1000));
        drop(queue);
        let report = world.orchestrator.tray_report();
        assert!(
            report.tooltip.contains("Das Boot · 25 %"),
            "{}",
            report.tooltip
        );
        assert!(
            report.tooltip.contains("Sopa de ganso · 90 %"),
            "{}",
            report.tooltip
        );
        assert_eq!(report.summary, "2 descargas · 2 MB/s");
    }

    #[test]
    fn the_tray_says_what_a_download_is_doing_when_it_is_not_downloading() {
        let world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            200 * GIGABYTE,
        );
        let id = grab_first(&world);
        world
            .downloader
            .queue
            .lock()
            .expect("not poisoned")
            .push(QueueItem {
                status: mamacine_core::nzbget::Status::Unpacking,
                ..downloading(id, "Das Boot", 1000, 1000)
            });
        let report = world.orchestrator.tray_report();
        assert_eq!(report.summary, "Das Boot · Casi lista");
        assert!(!report.tooltip.contains("MB/s"), "{}", report.tooltip);
    }

    #[test]
    fn a_tooltip_windows_would_cut_off_lists_fewer_films_and_counts_the_rest() {
        let items: Vec<String> = (1..=6)
            .map(|number| format!("Una película con un título larguísimo {number} · 50 %"))
            .collect();
        let tooltip = tray_tooltip("Mamá Cine · 5 MB/s", &items);
        assert!(tooltip.chars().count() <= TRAY_TOOLTIP_LIMIT, "{tooltip}");
        assert!(tooltip.contains("Una película"), "{tooltip}");
        assert!(tooltip.contains("más"), "{tooltip}");
    }

    #[test]
    fn a_title_too_long_for_the_tray_is_cut_rather_than_pushing_the_rest_out() {
        assert_eq!(shortened("Das Boot", 34), "Das Boot");
        let cut = shortened("El discreto encanto de la burguesía y algo más", 20);
        assert_eq!(cut.chars().count(), 20);
        assert!(cut.ends_with('…'), "{cut}");
    }

    #[test]
    fn pressing_download_twice_follows_the_running_download_instead_of_twinning_it() {
        let world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            200 * GIGABYTE,
        );
        let first = grab_first(&world);
        let second = world
            .orchestrator
            .grab(0, None, false)
            .expect("the same download");
        assert_eq!(second.id, first);
        assert!(!second.already);
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            1,
            "one film, one download"
        );
    }

    #[test]
    fn one_search_answers_with_films_and_seasons_together() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot S01 1080p WEB-DL-PACK", 8.0, 300),
            ],
            200 * GIGABYTE,
        );
        let found = world
            .orchestrator
            .search("das boot", None, None)
            .expect("results");
        assert_eq!(found.films.len(), 1);
        assert_eq!(found.seasons.len(), 1);
        assert_eq!(found.seasons[0].label, "Temporada 1");
        assert!(found.notice.is_none());
    }

    // She picked "serie" in the suggestions, and was shown a wall of films first: the films
    // category was searched anyway, and its parodies and episode reviews came back above the
    // seasons she chose.
    #[test]
    fn a_name_she_already_called_a_series_is_never_searched_as_a_film() {
        let world = world_with(
            vec![
                release("Game of Thrones The Last Watch 1080p", 2.0, 900),
                release("Game.Of.Thrones.S01.1080p.WEB-DL-PACK", 8.0, 300),
            ],
            200 * GIGABYTE,
        );
        let found = world
            .orchestrator
            .search("game of thrones", Some("series"), None)
            .expect("results");
        assert!(
            found.films.is_empty(),
            "no films for a name she called a series"
        );
        assert_eq!(found.seasons.len(), 1);
    }

    // Picking the Gomorrah series and being told it does not exist: the season search asked for
    // the name followed by "complete", which matches the release name literally, and season packs
    // are not named that. Asked by id, the same indexer answers with every season.
    #[test]
    fn a_show_she_picked_is_asked_for_by_id_and_named_after_her_pick() {
        let world = world_with(
            vec![
                release("Gomorra.S01.1080p.BluRay.x264-ITA", 8.0, 100),
                release("Gomorrah.S01.1080p.HMAX.WEB-DL.DUAL", 9.0, 300),
            ],
            200 * GIGABYTE,
        );
        world.orchestrator.suggest("gomorrah").expect("suggestions");
        let picked = world.orchestrator.pick(0).expect("the show she tapped");
        let found = world
            .orchestrator
            .search(&picked.query, Some("series"), Some(&picked.title))
            .expect("results");

        let asked = world.asked.lock().expect("not poisoned");
        assert!(
            asked.iter().any(|question| matches!(
                question,
                Query::Show { ids, .. } if ids.tvdb.as_deref() == Some("281342")
            )),
            "the show she picked is a show, not a spelling: {asked:?}"
        );
        assert_eq!(
            found.seasons.len(),
            1,
            "one card for the season, whichever name its packs carry"
        );
        assert_eq!(found.seasons[0].show, "Gomorrah");
    }

    // "Son varios episodios" is what the screen could say, and it is less than the app knows:
    // the show database can name them, and how many there are is what a season card is missing.
    #[test]
    fn the_episodes_of_a_picked_season_are_named_and_counted() {
        let world = world_with(
            vec![release("Gomorrah.S01.1080p.HMAX.WEB-DL.DUAL", 9.0, 300)],
            200 * GIGABYTE,
        );
        world.orchestrator.suggest("gomorrah").expect("suggestions");
        let picked = world.orchestrator.pick(0).expect("the show she tapped");
        world
            .orchestrator
            .search(&picked.query, Some("series"), Some(&picked.title))
            .expect("results");

        let episodes = world.orchestrator.season_episodes(0).expect("episodes");
        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[0].title.as_deref(), Some("Episodio 1 de la 1"));
        let (imdb, season) = world.orchestrator.imdb_season_of(0).expect("a ficha");
        assert_eq!((imdb.as_str(), season), ("tt2049116", 1));
    }

    #[test]
    fn a_season_found_by_name_alone_says_nothing_about_its_episodes() {
        let world = world_with(
            vec![release("Gomorrah.S01.1080p.HMAX.WEB-DL.DUAL", 9.0, 300)],
            200 * GIGABYTE,
        );
        world
            .orchestrator
            .search("gomorrah", Some("series"), None)
            .expect("results");
        assert!(
            world
                .orchestrator
                .season_episodes(0)
                .expect("no episodes, no error")
                .is_empty(),
            "a search that identified no show must not name another show's episodes"
        );
    }

    #[test]
    fn a_name_she_typed_is_never_taken_for_the_show_she_last_picked() {
        let world = world_with(
            vec![release("Gomorrah.S01.1080p.HMAX.WEB-DL.DUAL", 9.0, 300)],
            200 * GIGABYTE,
        );
        world.orchestrator.suggest("gomorrah").expect("suggestions");
        world.orchestrator.pick(0).expect("the show she tapped");
        world
            .orchestrator
            .search("otra serie", Some("series"), None)
            .expect("results");
        let asked = world.asked.lock().expect("not poisoned");
        assert!(
            asked.iter().all(|question| matches!(
                question,
                Query::Show { ids, .. } if !ids.any()
            )),
            "her own words are not the last show she tapped: {asked:?}"
        );
    }

    // She typed the name she knows and got an empty screen: "juego de tronos" answered with
    // nothing at every indexer, while "game of thrones" answered with the whole show.
    #[test]
    fn a_name_in_her_language_finds_what_releases_are_named_in_another() {
        let world = world_with(
            vec![release("Game.Of.Thrones.S01.1080p.WEB-DL-PACK", 8.0, 300)],
            200 * GIGABYTE,
        );
        let found = world
            .orchestrator
            .search("juego de tronos", None, None)
            .expect("results");

        assert_eq!(
            found.seasons.len(),
            1,
            "the show she asked for, under the name its releases carry"
        );
        assert_eq!(found.seasons[0].show, "Game of Thrones");
        let asked = world.asked.lock().expect("not poisoned");
        assert!(
            asked.iter().any(|question| matches!(
                question,
                Query::Show { ids, .. } if ids.tvdb.as_deref() == Some("281342")
            )),
            "her words name a show the provider could identify: {asked:?}"
        );
    }

    // She typed the name she knows and the releases are named in another language entirely.
    // Nothing here reads Japanese, and nothing has to: the show was identified, and the indexer
    // was asked for the thing rather than for her words.
    #[test]
    fn a_film_whose_releases_are_named_in_another_language_is_still_hers() {
        let world = world_with(
            vec![release("Howls.Moving.Castle.2004.1080p.BluRay-X", 8.0, 300)],
            200 * GIGABYTE,
        );
        let found = world
            .orchestrator
            .search("el castillo ambulante", None, None)
            .expect("results");

        assert_eq!(found.films.len(), 1, "the film she asked for");
        assert!(
            found.exact,
            "a title that was identified is not a misspelling"
        );
        let asked = world.asked.lock().expect("not poisoned");
        assert!(
            asked.contains(&Query::Imdb("0347149".into())),
            "asked for the film itself, not for the words she reached for it with: {asked:?}"
        );
    }

    // Her words go to an indexer only when nothing recognised them. Sending them anyway asks a
    // second question that answers with the same releases or with noise, and either way it is a
    // search hit somebody else is paying for.
    #[test]
    fn an_identified_title_is_asked_for_instead_of_her_words_not_beside_them() {
        let world = world_with(
            vec![release("Game.Of.Thrones.S01.1080p.WEB-DL-PACK", 8.0, 300)],
            200 * GIGABYTE,
        );
        world
            .orchestrator
            .search("juego de tronos", None, None)
            .expect("results");
        let asked = world.asked.lock().expect("not poisoned");
        assert!(
            !asked.iter().any(|question| matches!(
                question,
                Query::Title(text) | Query::Show { name: text, .. } if text.contains("juego")
            )),
            "her words were identified, so they are not also asked: {asked:?}"
        );
        assert_eq!(
            asked.len(),
            1,
            "the provider said it is a series and no film, so film was not asked either: {asked:?}"
        );
    }

    // Nothing recognised what she typed, and an empty screen would be the wrong answer while an
    // indexer can still be asked. Her words are the question of last resort, and what comes back
    // is judged against them, because a keyword search is free to have misunderstood.
    #[test]
    fn words_nothing_recognises_are_still_asked_and_still_judged() {
        let world = world_with(
            vec![
                release("Something.Else.Entirely.2020.1080p-X", 4.0, 10),
                release("Pelicula.Rarisima.2020.1080p-X", 4.0, 10),
            ],
            200 * GIGABYTE,
        );
        let found = world
            .orchestrator
            .search("pelicula rarisima", None, None)
            .expect("results");
        let asked = world.asked.lock().expect("not poisoned");
        assert!(
            asked.contains(&Query::Title("pelicula rarisima".into())),
            "her words are the question when nothing else is: {asked:?}"
        );
        assert!(
            !found.exact,
            "nothing identified it, so an empty answer may well be her spelling"
        );
        assert_eq!(
            found.films.len(),
            1,
            "the indexer was free to free-associate and one of these is not hers"
        );
    }

    // "Gomorra" is a film and a series. Both are hers to choose from, and both were asked for by
    // id, so neither can be the indexer having misunderstood. Which of the two the name means is
    // still a question, and the provider was asked it: its own order is the answer, and the screen
    // does not toss a coin over it a second time.
    #[test]
    fn the_kind_the_provider_offered_first_is_what_the_name_means() {
        let world = world_with(
            vec![
                release("Gomorrah.S01.1080p.HMAX.WEB-DL.DUAL", 9.0, 300),
                release("Gomorrah.2008.1080p.BluRay.x264-CiNEFiLE", 9.0, 300),
            ],
            200 * GIGABYTE,
        );
        let found = world
            .orchestrator
            .search("gomorra", None, None)
            .expect("results");
        let season = found.seasons.first().expect("the series");
        let film = found.films.first().expect("the film of the same name");
        assert_eq!(
            season.relevance, 3.0,
            "the series is what the provider offered first"
        );
        assert!(
            film.relevance < season.relevance,
            "the film of the same name is hers to reach, under the show ({} vs {})",
            film.relevance,
            season.relevance
        );
    }

    // The window asks for suggestions on every keystroke, then she presses Buscar. Asking the
    // same provider the same question again is a call somebody else is paying for.
    #[test]
    fn submitting_right_after_typing_reuses_the_suggestions_already_fetched() {
        struct CountingSuggest {
            asked: Arc<Mutex<Vec<String>>>,
        }
        impl Suggest for CountingSuggest {
            fn suggest(&self, text: &str) -> Result<Vec<Suggestion>> {
                self.asked.lock().expect("not poisoned").push(text.into());
                NoSuggestions.suggest(text)
            }
            fn resolve(&self, suggestion: &Suggestion) -> Result<Picked> {
                NoSuggestions.resolve(suggestion)
            }
            fn poster(&self, url: &str) -> Result<(String, Vec<u8>)> {
                NoSuggestions.poster(url)
            }
        }

        let mut world = world_with(
            vec![release("Game.Of.Thrones.S01.1080p.WEB-DL-PACK", 8.0, 300)],
            200 * GIGABYTE,
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        world.orchestrator.suggestions = Box::new(CountingSuggest {
            asked: Arc::clone(&asked),
        });

        world
            .orchestrator
            .suggest("juego de tronos")
            .expect("suggestions");
        let found = world
            .orchestrator
            .search("juego de tronos", None, None)
            .expect("results");
        assert_eq!(found.seasons.len(), 1, "and it still translated her words");
        assert_eq!(
            asked.lock().expect("not poisoned").len(),
            1,
            "the answer to that question was already on hand"
        );
    }

    // She tapped a title: it arrived already named the way the indexer knows it, and translating
    // it again would ask a provider to second-guess a choice she made.
    #[test]
    fn a_title_she_tapped_is_never_put_through_a_translation() {
        struct NeverAsked;
        impl Suggest for NeverAsked {
            fn suggest(&self, _text: &str) -> Result<Vec<Suggestion>> {
                panic!("a picked title must not be looked up again")
            }
            fn resolve(&self, suggestion: &Suggestion) -> Result<Picked> {
                NoSuggestions.resolve(suggestion)
            }
            fn poster(&self, url: &str) -> Result<(String, Vec<u8>)> {
                NoSuggestions.poster(url)
            }
        }

        let mut world = world_with(
            vec![release("Game.Of.Thrones.S01.1080p.WEB-DL-PACK", 8.0, 300)],
            200 * GIGABYTE,
        );
        world.orchestrator.suggestions = Box::new(NeverAsked);
        world
            .orchestrator
            .search("Game of Thrones", Some("series"), Some("Game of Thrones"))
            .expect("results");
    }

    // Nothing recognised her words, so an indexer was asked for them and was free to have
    // misunderstood: what she named must never sit below what merely mentions it.
    #[test]
    fn what_she_named_outranks_what_merely_mentions_it() {
        let world = world_with(
            vec![
                release("Das Boot The Making Of 1981 1080p", 2.0, 900),
                release("Das.Boot.1981.1080p.BluRay-X", 8.0, 300),
            ],
            200 * GIGABYTE,
        );
        let found = world
            .orchestrator
            .search("das boot", None, None)
            .expect("results");
        let best = found.films.first().expect("the film she named");
        assert_eq!(best.title, "Das Boot");
        for other in found.films.iter().skip(1) {
            assert!(
                best.relevance > other.relevance,
                "the film she named ({}) must outrank {} ({})",
                best.relevance,
                other.title,
                other.relevance
            );
        }
    }

    // The chase spends seconds fetching the next copy; a poll landing in that window used to
    // see "failed, no successor" and flash the death sentence before retracting it. The screen
    // may only say "failed" once the library records the decision — and an undecided failure
    // nobody is handling must be adopted by the chase, so "working on it" is provably temporary.
    #[test]
    fn an_undecided_failure_never_reads_as_failed_and_is_always_picked_up() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        let first = grab_first(&world);
        world
            .downloader
            .history
            .lock()
            .expect("not poisoned")
            .push(failed(first, "Das Boot"));
        // the mid-decision window: the attempt is out of the map, nothing written yet
        let attempt = world
            .orchestrator
            .attempts
            .lock()
            .expect("not poisoned")
            .remove(&first)
            .expect("tracked");

        let progress = world.orchestrator.progress();
        let landed = progress
            .finished
            .iter()
            .find(|film| film.id == first)
            .expect("in history");
        assert!(
            landed.retrying,
            "mid-decision must read as working, never as failed"
        );
        drop(attempt); // the handler died here: the failure is now an orphan

        world.orchestrator.chase();
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            2,
            "the orphan was adopted and the next copy started"
        );
        let entry = world.orchestrator.library.get(first).expect("remembered");
        assert_eq!(entry.superseded_by, Some(first + 1));
    }

    // nzbget can only prove a rotten copy by downloading gigabytes until the failure count is
    // beyond repair — nine gigabytes and twenty minutes per copy, in the field. A sampled STAT
    // conversation knows in seconds, and the skip is a line of the story like any other skip.
    #[test]
    fn a_copy_the_server_no_longer_has_is_skipped_without_downloading_it() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        // the first copy is 8% gone against 6% par2; the second is intact
        world
            .prober_answers
            .lock()
            .expect("not poisoned")
            .extend([0.08, 0.0]);

        let id = grab_first(&world);
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            1,
            "only the intact copy was handed to nzbget"
        );
        let entry = world.orchestrator.library.get(id).expect("remembered");
        assert_eq!(
            entry.attempt, 1,
            "a probe skip costs nothing, so it spends none of her chase allowance"
        );
        assert!(
            !entry.story.iter().any(|note| {
                note.said.contains("descartado")
                    || note.said.contains("siguiente")
                    || note.said.contains("servidor")
            }),
            "seamless: nothing visible changed, so the story says nothing: {:?}",
            entry
                .story
                .iter()
                .map(|note| &note.said)
                .collect::<Vec<_>>()
        );
    }

    // 48 GB of dead seasons sat in nzbget's hidden work folder: a failed item keeps its
    // partial download until its history entry is deleted, and nothing ever deleted it. The
    // chase now buries what it discards, and sweeps corpses older builds left behind; the
    // library carries everything the screen needs, so nothing she sees changes.
    #[test]
    fn a_discarded_copy_is_buried_so_its_gigabytes_do_not_haunt_the_disk() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        let first = grab_first(&world);
        world
            .downloader
            .history
            .lock()
            .expect("not poisoned")
            .push(failed(first, "Das Boot"));
        world.orchestrator.chase();
        assert_eq!(
            world
                .downloader
                .forgotten
                .lock()
                .expect("not poisoned")
                .as_slice(),
            &[first],
            "the dead copy's files are deleted the moment it is replaced"
        );
        // the chain still reads whole from the library alone
        let progress = world.orchestrator.progress();
        let ghost = progress
            .finished
            .iter()
            .find(|film| film.id == first)
            .expect("still tellable");
        assert_eq!(ghost.next_id, Some(first + 1));

        // a corpse from an older build: failed in history, long since decided, tracked by nobody
        world
            .downloader
            .history
            .lock()
            .expect("not poisoned")
            .push(failed(77, "Old Season"));
        world.orchestrator.library.put(
            77,
            Entry {
                title: "Old Season".into(),
                gave_up: true,
                ..Entry::default()
            },
        );
        world.orchestrator.chase();
        assert!(
            world
                .downloader
                .forgotten
                .lock()
                .expect("not poisoned")
                .contains(&77),
            "leftovers from the past are swept by the same loop"
        );
    }

    // The Joy copy: par2 present on the server yet covering none of the damage — no remote
    // probe can see that, only nzbget's own attempt proves it. A copy proven dead is burned,
    // and a later grab of the same thing goes straight past it instead of spending four more
    // gigabytes on a known outcome.
    #[test]
    fn a_copy_the_downloader_refused_is_never_spent_again() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        let mut id = grab_first(&world);
        for _ in 0..2 {
            world
                .downloader
                .history
                .lock()
                .expect("not poisoned")
                .push(failed(id, "Das Boot"));
            world.orchestrator.chase();
            id += 1;
        }
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            2,
            "both copies were tried once"
        );

        // she searches the same film again and presses Descargar again
        world
            .orchestrator
            .search("das boot", None, None)
            .expect("results");
        let refused = world.orchestrator.grab(0, None, false);
        assert!(
            refused
                .expect_err("nothing worth starting")
                .contains("Ninguna de las copias"),
            "told plainly, instead of re-burning gigabytes on proven corpses"
        );
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            2,
            "not a byte spent twice on the same dead copies"
        );
    }

    // The Joy season: 1.7% damage, ten "par2" volumes that decode to something that is not
    // par2 at all — disguised data, a scanner-dodging trick. nzbget finds nothing to repair
    // with and any damage is fatal; the probe now unmasks the fakes for the price of one
    // article, before a byte of the release is spent.
    #[test]
    fn a_copy_with_fake_repair_files_is_skipped_without_downloading_it() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        // both copies: slight damage, par articles present — but the first copy's "par2" is
        // not par2, and the second's is real
        world
            .prober_answers
            .lock()
            .expect("not poisoned")
            .extend([0.017, 0.0, 0.017, 0.0]);
        world.prober_bodies.lock().expect("not poisoned").extend([
            b"data wearing a .par2 name".to_vec(),
            b"PAR2\0PKTthe real thing".to_vec(),
        ]);

        world
            .orchestrator
            .search("das boot", None, None)
            .expect("results");
        let id = world
            .orchestrator
            .grab(0, None, false)
            .expect("a download")
            .id;
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            1,
            "the disguised copy was never handed to nzbget"
        );
        let entry = world.orchestrator.library.get(id).expect("remembered");
        assert_eq!(entry.attempt, 1, "and it cost none of her allowance");
    }

    // The give-up screen used to be an instruction with no door. "Probar más copias" is the
    // door: the copies beyond the chase limit were kept, and the button spends them.
    #[test]
    fn trying_more_copies_continues_with_the_kept_ones_under_a_fresh_allowance() {
        let releases: Vec<SearchResult> = (0..7)
            .map(|n| {
                release(
                    &format!("Das Boot 1981 1080p BluRay x264-{n}"),
                    2.0,
                    900 - n,
                )
            })
            .collect();
        let world = world_with(releases, 200 * GIGABYTE);
        let mut id = grab_first(&world);
        for _ in 0..CHASE_LIMIT {
            world
                .downloader
                .history
                .lock()
                .expect("not poisoned")
                .push(failed(id, "Das Boot"));
            world.orchestrator.chase();
            id += 1;
        }
        let given_up = CHASE_LIMIT as i64;
        let entry = world
            .orchestrator
            .library
            .get(given_up)
            .expect("remembered");
        assert!(entry.gave_up);
        assert_eq!(
            entry.remaining.len(),
            4,
            "the untried copies were kept, not thrown away"
        );

        let grabbed = world
            .orchestrator
            .try_more(given_up)
            .expect("the button works");
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            CHASE_LIMIT + 1,
            "the fourth copy starts"
        );
        let fresh = world
            .orchestrator
            .library
            .get(grabbed.id)
            .expect("the new attempt");
        assert_eq!(
            fresh.attempt, 4,
            "the numbering carries on where it stopped"
        );
        assert_eq!(
            fresh.allowance,
            CHASE_LIMIT + CHASE_LIMIT,
            "and the chase may spend a fresh budget before asking again"
        );
        let old = world
            .orchestrator
            .library
            .get(given_up)
            .expect("the old attempt");
        assert_eq!(old.superseded_by, Some(grabbed.id), "the screen can follow");
        assert!(
            fresh
                .story
                .iter()
                .any(|note| note.said.contains("Sigo con las copias")),
            "her decision is a line of the story"
        );

        // pressing it with nothing left is answered, not obeyed
        let empty = world.orchestrator.try_more(999);
        assert!(empty.is_err());
    }

    // Choosing a Spanish copy is choosing Spanish: when it dies, the chase falls to the next
    // Spanish copy, and only after those are gone to another language.
    #[test]
    fn the_fall_through_keeps_her_language_before_it_keeps_the_ranking() {
        let releases = [
            release("Film.2016.1080p.BluRay-VO-Best", 2.0, 900),
            release("Film.2016.SPANISH.1080p.WEB-A", 2.0, 500),
            release("Film.2016.720p.WEB-VO-B", 1.8, 400),
            release("Film.2016.CASTELLANO.720p.WEB-B", 1.8, 300),
        ];
        let plan = candidates_from(&releases, Some(1));
        let titles: Vec<&str> = plan.iter().map(|release| release.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Film.2016.SPANISH.1080p.WEB-A",
                "Film.2016.CASTELLANO.720p.WEB-B",
                "Film.2016.1080p.BluRay-VO-Best",
                "Film.2016.720p.WEB-VO-B",
            ],
        );
    }

    // "el sur" answered with Tinker Bell, Pumpkinhead and True Romance: the indexer
    // free-associates, and a card that shares no word with the question makes the app look broken.
    #[test]
    fn what_shares_no_word_with_the_question_is_not_an_answer() {
        let mut junk = release("Tinker Bell and the Lost Treasure 1080p", 2.0, 700);
        junk.imdb = Some("1216475".into());
        junk.about = "Tinker Bell and the Lost Treasure · 2009".into();
        let mut wanted = release("El.Sur.1983.1080p.BluRay", 2.0, 100);
        wanted.imdb = Some("0086010".into());
        wanted.about = "El Sur · 1983".into();
        let world = world_with(vec![junk, wanted], 200 * GIGABYTE);

        let found = world
            .orchestrator
            .search("el sur", None, None)
            .expect("results");
        assert_eq!(found.films.len(), 1, "only what resembles the question");
        assert_eq!(found.films[0].title, "El Sur");
    }

    // Seen live with "campeones": the film database titles it "Champions", so the card shared no
    // word with her query, while every release is filed as Campeones. The releases get a vote.
    #[test]
    fn a_film_titled_in_another_language_is_still_found_by_its_releases() {
        let mut film = release("Campeones.2018.MULTi.1080p.WEB.H264-SAVER", 2.0, 300);
        film.imdb = Some("7600372".into());
        film.about = "Champions · 2018 · ★7.2 · Comedy · 124 min".into();
        let world = world_with(vec![film], 200 * GIGABYTE);

        let found = world
            .orchestrator
            .search("campeones", None, None)
            .expect("results");
        assert_eq!(found.films.len(), 1);
        assert!(found.films[0].relevance > 0.0);
    }

    // The film database titles things in other languages; the name she picked, localized with
    // the original beside it, must be the name the card, the library and the shelf then use.
    #[test]
    fn the_picked_name_follows_the_film_all_the_way_to_the_library() {
        let world = world_with(
            vec![release("The.Platform.2019.1080p.WEB-DL", 2.0, 900)],
            200 * GIGABYTE,
        );
        let suggested = world.orchestrator.suggest("el hoyo").expect("suggestions");
        assert_eq!(suggested[0].original.as_deref(), Some("The Platform"));

        let picked = world.orchestrator.pick(0).expect("resolved");
        assert_eq!(picked.query, "tt8228288");

        let found = world
            .orchestrator
            .search(&picked.query, Some("film"), Some("El hoyo (The Platform)"))
            .expect("results");
        assert_eq!(found.films[0].title, "El hoyo (The Platform)");

        let id = world
            .orchestrator
            .grab(0, None, false)
            .expect("a download")
            .id;
        let entry = world.orchestrator.library.get(id).expect("remembered");
        assert_eq!(
            entry.title, "El hoyo (The Platform)",
            "the shelf and the notifications will use her name for it"
        );
    }

    #[test]
    fn picking_a_title_that_is_gone_says_so_instead_of_guessing() {
        let world = world_with(Vec::new(), 200 * GIGABYTE);
        let refused = world.orchestrator.pick(7);
        assert!(refused.is_err());
    }

    // Seen live: the library believed copy 5 of 7 was downloading; nzbget had never heard of the
    // id (a rejected nzb, or a lost queue). The screen said "empezando la descarga" forever.
    #[test]
    fn a_download_nzbget_has_lost_is_chased_like_a_dead_copy_not_waited_on_forever() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        let first = grab_first(&world);
        // nzbget loses the id entirely: not in the queue, never reaches history
        world.downloader.queue.lock().expect("not poisoned").clear();

        // before the chase acts, the screen must at least see the film as still being worked on
        let progress = world.orchestrator.progress();
        let ghost = progress
            .finished
            .iter()
            .find(|film| film.id == first)
            .expect("the lost download is visible, not a frozen screen");
        assert!(ghost.retrying);

        world.orchestrator.chase();
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            2,
            "the next copy was started"
        );
        let progress = world.orchestrator.progress();
        let ghost = progress
            .finished
            .iter()
            .find(|film| film.id == first)
            .expect("still visible");
        assert_eq!(
            ghost.next_id,
            Some(first + 1),
            "the screen can follow the chain"
        );
        let entry = world.orchestrator.library.get(first).expect("remembered");
        assert!(
            entry
                .story
                .iter()
                .any(|note| note.said.contains("se ha perdido")),
            "{:?}",
            entry
                .story
                .iter()
                .map(|note| &note.said)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cancelling_writes_the_line_and_stops_the_chase() {
        let world = world_with(
            vec![
                release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900),
                release("Das Boot 1981 720p WEB-DL-B", 1.8, 500),
            ],
            200 * GIGABYTE,
        );
        let id = grab_first(&world);
        world.orchestrator.cancel(id).expect("cancelled");

        assert_eq!(
            world
                .downloader
                .cancelled
                .lock()
                .expect("not poisoned")
                .as_slice(),
            &[id]
        );
        let entry = world.orchestrator.library.get(id).expect("remembered");
        assert!(entry
            .story
            .iter()
            .any(|note| note.said == messages::CANCELLED));

        // the copy dying later must not resurrect the download she stopped
        world
            .downloader
            .history
            .lock()
            .expect("not poisoned")
            .push(failed(id, "Das Boot"));
        world.orchestrator.chase();
        assert_eq!(
            world
                .downloader
                .appended
                .lock()
                .expect("not poisoned")
                .len(),
            1
        );
    }

    #[test]
    fn removing_a_film_sends_its_folder_to_the_bin_and_off_the_shelf() {
        let world = world_with(Vec::new(), 200 * GIGABYTE);
        let films = world.directory.join("the-film");
        std::fs::create_dir_all(&films).expect("a folder");
        world.orchestrator.library.put(
            4,
            Entry {
                title: "El Sur".into(),
                key: "imdb:86010".into(),
                settled: true,
                folder: Some(films.clone()),
                ..Entry::default()
            },
        );
        assert_eq!(world.orchestrator.progress().shelf.len(), 1);

        world.orchestrator.remove(4).expect("removed");
        assert!(!films.exists());
        assert_eq!(world.orchestrator.progress().shelf.len(), 0);
    }

    #[test]
    fn the_room_verdict_on_a_version_matches_the_rule_that_refuses_it() {
        let world = world_with(
            vec![release("Das Boot 1981 1080p BluRay x264-A", 2.0, 900)],
            // between the old interface warning (1.4x) and the refusal (2.2x + reserve):
            // exactly where she used to be told nothing and then refused
            4 * GIGABYTE,
        );
        world
            .orchestrator
            .search("das boot", None, None)
            .expect("results");
        let versions = world.orchestrator.versions(0, false).expect("versions");
        assert_ne!(
            versions[0].room, "fits",
            "the warning and the refusal agree"
        );
    }

    #[test]
    fn a_season_folder_becomes_episodes_she_can_count() {
        let world = world_with(Vec::new(), 200 * GIGABYTE);
        let folder = world.directory.join("season");
        std::fs::create_dir_all(&folder).expect("a folder");
        for name in ["Show.S01E02.mkv", "Show.S01E01.mkv", "extras.mkv"] {
            std::fs::write(folder.join(name), b"x").expect("a file");
        }
        world.orchestrator.library.put(
            6,
            Entry {
                series: true,
                settled: true,
                folder: Some(folder.clone()),
                ..Entry::default()
            },
        );

        let episodes = world.orchestrator.episodes(6).expect("episodes");
        assert_eq!(episodes[0].label, "Episodio 1");
        assert_eq!(episodes[1].label, "Episodio 2");
        assert_eq!(episodes.len(), 3, "an unplaceable file is still reachable");
        let file = world.orchestrator.episode_file(6, 1).expect("a file");
        assert!(file.ends_with("Show.S01E02.mkv"));
    }

    // A folder of files numbered by a scene release is not an evening she can choose from. The
    // show she downloaded is remembered with the season, so its episodes can be named on her own
    // shelf, months after the search that found them is gone.
    #[test]
    fn a_season_she_owns_names_its_episodes_from_the_show_database() {
        let world = world_with(Vec::new(), 200 * GIGABYTE);
        let folder = world.directory.join("named");
        std::fs::create_dir_all(&folder).expect("a folder");
        for name in ["Show.S01E01.mkv", "Show.S01E02.mkv"] {
            std::fs::write(folder.join(name), b"x").expect("a file");
        }
        world.orchestrator.library.put(
            8,
            Entry {
                series: true,
                settled: true,
                folder: Some(folder.clone()),
                seasons: Some((1, 1)),
                show: ShowIds {
                    tvmaze: Some("2228".into()),
                    ..ShowIds::default()
                },
                ..Entry::default()
            },
        );

        let episodes = world.orchestrator.episodes(8).expect("episodes");
        assert_eq!(episodes[0].title.as_deref(), Some("Episodio 1 de la 1"));
        assert_eq!(
            episodes[0].overview.as_deref(),
            Some("Lo que pasa en el 1."),
            "and says what happens in it, where the database does"
        );
        assert_eq!(episodes[1].number, Some(2));

        // a season the database has no name for is still a list she can play
        world.orchestrator.library.put(
            9,
            Entry {
                series: true,
                settled: true,
                folder: Some(folder),
                ..Entry::default()
            },
        );
        let older = world.orchestrator.episodes(9).expect("episodes");
        assert_eq!(older[0].label, "Episodio 1");
        assert_eq!(
            older[0].title, None,
            "and is never given a name nobody said"
        );
    }

    // The season she downloaded before the app kept the show's ids knows neither which show it is
    // nor which seasons it holds, and the search that knew both is long gone. Neither question
    // needs it: the files state the season, and the show database answers to the name on her card.
    #[test]
    fn a_season_that_never_knew_its_show_identifies_itself_from_the_name_on_the_card() {
        let world = world_with(Vec::new(), 200 * GIGABYTE);
        let folder = world.directory.join("gomorra");
        std::fs::create_dir_all(&folder).expect("a folder");
        for name in ["Gomorrah.S01E01.mkv", "Gomorrah.S01E02.mkv"] {
            std::fs::write(folder.join(name), b"x").expect("a file");
        }
        world.orchestrator.library.put(
            11,
            Entry {
                title: "Gomorrah · Temporada 1".into(),
                series: true,
                settled: true,
                folder: Some(folder),
                ..Entry::default()
            },
        );

        let episodes = world.orchestrator.episodes(11).expect("episodes");
        assert_eq!(episodes[0].title.as_deref(), Some("Episodio 1 de la 1"));
        assert_eq!(
            episodes[0].overview.as_deref(),
            Some("Lo que pasa en el 1.")
        );

        let kept = world.orchestrator.library.get(11).expect("the entry");
        assert_eq!(
            kept.seasons,
            Some((1, 1)),
            "the files said which season this is"
        );
        assert!(kept.show.any(), "and what it was identified as is kept");
        assert_eq!(
            kept.imdb.as_deref(),
            Some("tt2049116"),
            "so her page can also say what the show is about"
        );
    }

    // A wrong name would be worse than no name: she would be reading another programme's episodes
    // while this one plays. Which show a name is belongs to the provider, which has the ordering
    // and the ids to decide it; what is decided here is that no answer leaves the season numbered.
    #[test]
    fn a_name_the_database_cannot_place_leaves_the_episodes_numbered() {
        struct NoIdea;
        impl Suggest for NoIdea {
            fn suggest(&self, _text: &str) -> Result<Vec<Suggestion>> {
                Ok(Vec::new())
            }
            fn resolve(&self, suggestion: &Suggestion) -> Result<Picked> {
                Ok(mamacine_core::lookup::resolve(suggestion))
            }
            fn poster(&self, _url: &str) -> Result<(String, Vec<u8>)> {
                Err(Error::Setup("no posters in tests".into()))
            }
            fn episodes(&self, _show: &ShowIds, _first: u32, _last: u32) -> Result<Vec<Episode>> {
                Ok(vec![Episode {
                    season: 1,
                    number: 1,
                    title: Some("Un nombre de otra serie".into()),
                    overview: None,
                }])
            }
        }

        let mut world = world_with(Vec::new(), 200 * GIGABYTE);
        world.orchestrator.suggestions = Box::new(NoIdea);
        let folder = world.directory.join("otra");
        std::fs::create_dir_all(&folder).expect("a folder");
        std::fs::write(folder.join("Serie.S01E01.mkv"), b"x").expect("a file");
        world.orchestrator.library.put(
            13,
            Entry {
                title: "Una serie que nadie conoce · Temporada 1".into(),
                series: true,
                settled: true,
                folder: Some(folder),
                ..Entry::default()
            },
        );

        let episodes = world.orchestrator.episodes(13).expect("episodes");
        assert_eq!(episodes[0].label, "Episodio 1");
        assert_eq!(
            episodes[0].title, None,
            "and no episode is named after a guess"
        );
        assert!(
            !world
                .orchestrator
                .library
                .get(13)
                .expect("the entry")
                .show
                .any(),
            "and a show it is not is never written down"
        );
    }

    // Her own page for something she owns has the same words the search screen had, and asks for
    // them by the id kept with the film rather than by a place in results long since replaced.
    #[test]
    fn what_she_owns_says_what_it_is_about_after_the_search_is_gone() {
        let mut world = world_with(Vec::new(), 200 * GIGABYTE);
        let asked = Arc::new(Mutex::new(Vec::new()));
        world.orchestrator.suggestions = Box::new(CountingFicha {
            asked: Arc::clone(&asked),
        });
        world.orchestrator.library.put(
            4,
            Entry {
                settled: true,
                imdb: Some("0082096".into()),
                ..Entry::default()
            },
        );
        assert_eq!(
            world.orchestrator.library_synopsis(4).expect("words"),
            "Una historia."
        );
        assert_eq!(
            asked.lock().expect("not poisoned").as_slice(),
            ["tt0082096"]
        );

        // a season keeps the show's id with the tt the indexer files it under
        world.orchestrator.library.put(
            5,
            Entry {
                settled: true,
                series: true,
                imdb: Some("tt1234567".into()),
                ..Entry::default()
            },
        );
        world.orchestrator.library_synopsis(5).expect("words");
        assert_eq!(
            asked.lock().expect("not poisoned").as_slice(),
            ["tt0082096", "tt1234567"],
            "one tt, never two"
        );

        world.orchestrator.library.put(6, Entry::default());
        assert_eq!(
            world.orchestrator.library_synopsis(6).expect("no words"),
            "",
            "nothing to ask about is not an error"
        );
    }

    // "Sin subtítulos en español en 2 de 12 episodios" named no episode, and this is the screen
    // where naming one is worth something: she is choosing what to watch tonight.
    #[test]
    fn an_episode_says_whether_it_has_subtitles_she_can_read() {
        let world = world_with(Vec::new(), 200 * GIGABYTE);
        let folder = world.directory.join("subtitled");
        std::fs::create_dir_all(&folder).expect("a folder");
        for name in [
            "Show.S01E01.mkv",
            "Show.S01E01.es.srt",
            "Show.S01E02.mkv",
            "Show.S01E03.mkv",
            "Show.S01E03.spa.srt",
            "Show.S01E04.mkv",
            "Show.S01E04.en.srt",
        ] {
            std::fs::write(folder.join(name), b"x").expect("a file");
        }
        world.orchestrator.library.put(
            7,
            Entry {
                series: true,
                settled: true,
                folder: Some(folder.clone()),
                ..Entry::default()
            },
        );

        let episodes = world.orchestrator.episodes(7).expect("episodes");
        let has: Vec<bool> = episodes.iter().map(|episode| episode.subtitles).collect();
        assert_eq!(has, vec![true, false, true, false]);
    }
}
