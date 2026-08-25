use mamacine_core::clock::SystemClock;
use mamacine_core::error::Result;
use mamacine_core::http::{HttpClient, Request, Response};
use mamacine_core::indexer::{Category, Indexer, Newznab, Query};
use mamacine_core::lookup::{Lookup, Picked};
use mamacine_core::net::Network;
use mamacine_core::release::Preference;
use mamacine_core::search::{gather, relevance};
use mamacine_core::settings::IndexerSettings;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

struct CachedHttp {
    inner: Network,
    directory: PathBuf,
}

impl CachedHttp {
    fn new() -> CachedHttp {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/probe-cache");
        std::fs::create_dir_all(&directory).expect("a cache folder");
        CachedHttp {
            inner: Network::new(),
            directory,
        }
    }

    fn slot(&self, request: &Request) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        request.url.hash(&mut hasher);
        self.directory.join(format!("{:016x}", hasher.finish()))
    }
}

impl HttpClient for CachedHttp {
    fn send(&self, request: Request) -> Result<Response> {
        let slot = self.slot(&request);
        if let Ok(bytes) = std::fs::read(&slot) {
            let split = bytes.iter().position(|byte| *byte == b'\n').unwrap_or(0);
            let head = String::from_utf8_lossy(&bytes[..split]).to_string();
            let (status, content_type) = head.split_once(' ').unwrap_or(("200", ""));
            return Ok(Response {
                status: status.parse().unwrap_or(200),
                content_type: content_type.to_string(),
                body: bytes[split + 1..].to_vec(),
            });
        }
        eprintln!("[live] {}", public_part(&request.url));
        let response = self.inner.send(request)?;
        if response.status == 200 {
            let mut kept = format!("{} {}\n", response.status, response.content_type).into_bytes();
            kept.extend(&response.body);
            let _ = std::fs::write(&slot, kept);
        }
        Ok(response)
    }
}

fn public_part(url: &str) -> String {
    url.split("apikey=")
        .next()
        .unwrap_or(url)
        .trim_end_matches(['?', '&'])
        .to_string()
}

fn real_indexer() -> Newznab<CachedHttp, SystemClock> {
    let home = std::env::var("HOME").expect("a home");
    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(format!("{home}/.config/com.fnune.mamacine/settings.json"))
            .expect("the app settings"),
    )
    .expect("readable settings");
    let text = |field: &str| {
        stored
            .get(field)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let (url, key) = match stored.get("indexers").and_then(|value| value.as_array()) {
        Some(rows) if !rows.is_empty() => (
            rows[0]["url"].as_str().unwrap_or_default().to_string(),
            rows[0]["key"].as_str().unwrap_or_default().to_string(),
        ),
        _ => (text("indexer_url"), text("indexer_key")),
    };
    assert!(!key.is_empty(), "no indexer key in the settings");
    Newznab::new(
        IndexerSettings {
            name: "real".into(),
            base_url: if url.is_empty() {
                "https://api.nzbgeek.info".into()
            } else {
                url
            },
            api_key: key,
            enabled: true,
        },
        CachedHttp::new(),
        SystemClock,
    )
}

fn show_of(query: &str) -> (String, mamacine_core::indexer::ShowIds) {
    let lookup = Lookup::new(CachedHttp::new());
    let named = |ids| (query.to_string(), ids);
    let Ok(found) = lookup.suggest(query) else {
        return named(Default::default());
    };
    let Some(series) = found.into_iter().find(|title| title.series) else {
        return named(Default::default());
    };
    let picked = mamacine_core::lookup::resolve(&series);
    let Some(imdb) = picked.show.imdb.as_deref() else {
        return named(Default::default());
    };
    match mamacine_core::tvmaze::TvMaze::new(CachedHttp::new()).ids_for(imdb) {
        Ok(ids) => {
            eprintln!("[show] {} → {ids:?}", picked.title);
            (picked.title, ids)
        }
        Err(failure) => {
            eprintln!("[show] {failure}");
            named(Default::default())
        }
    }
}

fn identify(query: &str) -> Vec<Picked> {
    let lookup = Lookup::new(CachedHttp::new());
    let Ok(found) = lookup.suggest(query) else {
        return Vec::new();
    };
    let mut resolved: Vec<Picked> = Vec::new();
    for suggestion in &found {
        if resolved.iter().any(|done| done.series == suggestion.series) {
            continue;
        }
        let picked = mamacine_core::lookup::resolve(suggestion);
        eprintln!(
            "[identified] {query} → {} ({})",
            picked.title,
            if picked.series { "serie" } else { "film" }
        );
        let ids = picked
            .show
            .imdb
            .as_deref()
            .filter(|_| picked.series)
            .map(|imdb| mamacine_core::tvmaze::TvMaze::new(CachedHttp::new()).ids_for(imdb));
        match ids {
            Some(Ok(ids)) => {
                eprintln!("[show] {} → {ids:?}", picked.title);
                resolved.push(Picked {
                    show: ids,
                    ..picked
                });
            }
            Some(Err(failure)) => {
                eprintln!("[show] {failure}");
                resolved.push(picked);
            }
            None => resolved.push(picked),
        }
    }
    resolved
}

fn show_search(query: &str, kind: Option<&str>) {
    let preference = match std::env::var("PROBE_LANG").as_deref() {
        Ok("original") => Preference::Original,
        Ok(code) => mamacine_core::release::known_language(code)
            .map(Preference::Language)
            .unwrap_or(Preference::Any),
        _ => Preference::Any,
    };
    let indexer = real_indexer();
    let indexers = [("real", &indexer as &dyn Indexer)];
    let parsed = Query::parse(query).expect("a query");

    let identified = match (&parsed, kind) {
        (Query::Imdb(_), _) | (_, Some(_)) => Vec::new(),
        _ => identify(query),
    };
    let identified_as = |series: bool| identified.iter().find(|found| found.series == series);
    let (film, series) = (identified_as(false), identified_as(true));
    let recognised = !identified.is_empty();
    let certain = |series: bool| match identified.first() {
        Some(first) if first.series != series => 2.0,
        _ => 3.0,
    };

    let film_question = match (film, recognised) {
        (Some(picked), _) => Query::parse(&picked.query),
        (None, false) => Some(parsed.clone()),
        (None, true) => None,
    };
    let films_found = match (kind, &film_question) {
        (Some("series"), _) | (_, None) => Vec::new(),
        (_, Some(question)) => {
            let gathered = gather(indexers, question, Some(Category::Movies));
            for (name, error) in &gathered.problems {
                eprintln!("problem: {name}: {error}");
            }
            gathered.results
        }
    };
    let judge_films_by = match &film_question {
        None | Some(Query::Imdb(_)) => None,
        Some(_) => Some(film.map(|picked| picked.query.as_str()).unwrap_or(query)),
    };

    let (name, show) = match (series, kind) {
        (Some(picked), _) => (picked.title.clone(), picked.show.clone()),
        (None, Some("series")) => show_of(query),
        _ => (query.to_string(), Default::default()),
    };
    let show_named = series.map(|picked| picked.query.as_str()).unwrap_or(query);
    let television = kind.is_some() || !recognised || series.is_some();
    let seasons_found = if kind == Some("film") || matches!(parsed, Query::Imdb(_)) || !television {
        Vec::new()
    } else {
        let gathered = gather(
            indexers,
            &Query::Show {
                name: mamacine_core::search::fold(show_named.trim()),
                ids: show.clone(),
            },
            Some(Category::Television),
        );
        for (name, error) in &gathered.problems {
            eprintln!("problem: {name}: {error}");
        }
        gathered.results
    };

    let named = show.any().then_some(name.as_str());
    let films = mamacine_core::films::group(films_found, preference);
    let seasons = mamacine_core::series::group_seasons(seasons_found, preference, named);

    let looks_like =
        |asked: &str, name: &str, releases: &[mamacine_core::indexer::SearchResult]| {
            releases
                .iter()
                .take(5)
                .map(|release| relevance(asked, &release.title))
                .fold(relevance(asked, name), f64::max)
        };

    let mut cards: Vec<(f64, String, Vec<String>)> = Vec::new();
    for film in &films {
        let best = film.best();
        cards.push((
            match judge_films_by {
                Some(asked) => looks_like(asked, &film.title, &film.releases),
                None => certain(false),
            },
            format!(
                "[film]   {:40} {}  · {:.1} GB · {} grabs · {} releases",
                film.title,
                film.year.clone().unwrap_or_default(),
                best.map(|release| release.size_bytes).unwrap_or(0) as f64 / 1_073_741_824.0,
                best.map(|release| release.grabs).unwrap_or(0),
                film.releases.len(),
            ),
            film.releases
                .iter()
                .take(3)
                .map(|release| release.title.clone())
                .collect(),
        ));
    }
    for season in &seasons {
        let best = season.best();
        cards.push((
            match named {
                Some(_) => certain(true),
                None => looks_like(show_named, &season.show, &season.releases),
            },
            format!(
                "[season] {:40} {}  {:.1} GB · {} grabs · {} releases",
                season.show,
                season_label(season),
                best.map(|release| release.size_bytes).unwrap_or(0) as f64 / 1_073_741_824.0,
                best.map(|release| release.grabs).unwrap_or(0),
                season.releases.len(),
            ),
            season
                .releases
                .iter()
                .take(3)
                .map(|release| release.title.clone())
                .collect(),
        ));
    }
    cards.retain(|(score, _, _)| *score > 0.0);
    cards.sort_by(|(left, _, _), (right, _, _)| right.partial_cmp(left).expect("comparable"));

    println!(
        "— {query}{} —",
        kind.map(|kind| format!(" ({kind})")).unwrap_or_default()
    );
    for (score, line, _) in cards.iter().take(14) {
        println!("  {score:>5.2}  {line}");
    }
    if cards.len() > 14 {
        println!("  … and {} more", cards.len() - 14);
    }
    if let (Some(tvmaze), Some(season)) = (&show.tvmaze, seasons.first()) {
        match mamacine_core::tvmaze::TvMaze::new(CachedHttp::new()).episodes(
            tvmaze,
            season.first,
            season.last,
        ) {
            Ok(episodes) => {
                println!("  {} · {} episodios:", season_label(season), episodes.len());
                for episode in episodes.iter().take(3) {
                    println!(
                        "    {}. {}",
                        episode.number,
                        episode.title.clone().unwrap_or_else(|| "?".into())
                    );
                }
            }
            Err(failure) => println!("  (no episode list: {failure})"),
        }
    }
    if let Some((_, _, releases)) = cards.first() {
        println!("  the pick, then the plan:");
        for release in releases {
            println!("    · {release}");
        }
    }
    println!();
}

fn show_suggestions(text: &str) {
    let lookup = Lookup::new(CachedHttp::new());
    match lookup.suggest(text) {
        Ok(found) => {
            println!("— suggest: {text} —");
            for title in found {
                println!(
                    "  {} {:36} {}  {}",
                    if title.series { "[serie]" } else { "[film] " },
                    title.title,
                    title.year.unwrap_or_default(),
                    if title.poster_url.is_some() {
                        "· poster"
                    } else {
                        "· no poster"
                    },
                );
            }
        }
        Err(failure) => println!("suggest failed: {failure}"),
    }
    println!();
}

fn show_stat(query: &str) {
    use mamacine_core::nntp::Prober;
    let indexer = real_indexer();
    let (name, show) = show_of(query);
    let gathered = gather(
        [("real", &indexer as &dyn Indexer)],
        &Query::Show {
            name: mamacine_core::search::fold(query.trim()),
            ids: show.clone(),
        },
        Some(Category::Television),
    );
    let seasons = mamacine_core::series::group_seasons(
        gathered.results,
        Preference::Any,
        show.any().then_some(name.as_str()),
    );
    let Some(season) = seasons.first() else {
        println!("no seasons found");
        return;
    };
    let home = std::env::var("HOME").expect("a home");
    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(format!("{home}/.config/com.fnune.mamacine/settings.json"))
            .expect("the app settings"),
    )
    .expect("readable settings");
    let text = |field: &str| stored[field].as_str().unwrap_or_default().to_string();
    let news = mamacine_core::settings::NewsServer {
        host: text("news_host"),
        port: stored["news_port"].as_u64().unwrap_or(563) as u16,
        username: text("news_user"),
        password: text("news_password"),
        encrypted: true,
        connections: 8,
        retention_days: 0,
    };

    println!("— {} · {} —", season.show, season_label(season));
    for release in season.releases.iter().take(4) {
        let Ok(nzb) = indexer.fetch_nzb(&release.nzb_url) else {
            println!("  (could not fetch nzb for {})", release.title);
            continue;
        };
        let Ok(contents) = mamacine_core::nzb::read(&nzb) else {
            println!("  (unreadable nzb: {})", release.title);
            continue;
        };
        let sample = contents.sample_with_files(300);
        let ids: Vec<&str> = sample.iter().map(|(_, id)| *id).collect();
        let par_ids = contents.sample_par(100);
        let started = std::time::Instant::now();
        let ratio = |statuses: &[bool]| {
            if statuses.is_empty() {
                0.0
            } else {
                statuses.iter().filter(|gone| **gone).count() as f64 / statuses.len() as f64
            }
        };
        let data_statuses = mamacine_core::nntp::NntpProbe.statuses(&news, &ids);
        let par_statuses = mamacine_core::nntp::NntpProbe.statuses(&news, &par_ids);
        match (data_statuses, par_statuses) {
            (Ok(data), Ok(par)) => {
                let missing = ratio(&data);
                let par_gone = ratio(&par);
                let effective = contents.effective_par(par_gone);
                println!(
                    "  data {:5.1}% gone · par {:4.1}% on paper, {:4.1}% of it gone → {:4.1}% usable · {} → {}  ({:?})",
                    missing * 100.0,
                    contents.par_ratio() * 100.0,
                    par_gone * 100.0,
                    effective * 100.0,
                    release.title,
                    if mamacine_core::nzb::beyond_repair(missing, effective) {
                        "SKIP"
                    } else {
                        "worth downloading"
                    },
                    started.elapsed(),
                );
                if missing > 0.0 {
                    let mut authentic = None;
                    for id in contents.par_index_segments().into_iter().take(2) {
                        match mamacine_core::nntp::NntpProbe.fetch_body(&news, id) {
                            Ok(bytes) => {
                                authentic = Some(mamacine_core::par2::contains_packets(&bytes));
                                if authentic == Some(true) {
                                    break;
                                }
                            }
                            Err(failure) => println!("    (par fetch failed: {failure})"),
                        }
                    }
                    match authentic {
                        Some(true) => println!("    par2 is real"),
                        Some(false) => println!("    par2 is FAKE (disguised data) → SKIP"),
                        None => println!("    par2 authenticity unknown"),
                    }
                }
            }
            (data, par) => println!(
                "  probe failed for {}: {:?} / {:?}",
                release.title,
                data.err().map(|e| e.to_string()),
                par.err().map(|e| e.to_string())
            ),
        }
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("--suggest") => show_suggestions(&arguments[1..].join(" ")),
        Some("--stat") => show_stat(&arguments[1..].join(" ")),
        Some("--series") => show_search(&arguments[1..].join(" "), Some("series")),
        Some("--film") => show_search(&arguments[1..].join(" "), Some("film")),
        _ => show_search(&arguments.join(" "), None),
    }
}

fn season_label(season: &mamacine_core::series::Season) -> String {
    if season.first == season.last {
        format!("Temporada {}", season.first)
    } else {
        format!("Temporadas {} a {}", season.first, season.last)
    }
}
