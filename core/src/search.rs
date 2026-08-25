//! Several indexers, one list; failures reported.

use crate::indexer::{Category, Indexer, Query, SearchResult};

pub struct Gathered {
    pub results: Vec<SearchResult>,
    /// Per failed indexer, the error kept whole.
    pub problems: Vec<(String, crate::error::Error)>,
}

impl Gathered {
    /// Every indexer refused and nothing was found.
    pub fn is_total_failure(&self) -> bool {
        self.results.is_empty() && !self.problems.is_empty()
    }
}

pub fn gather<'a>(
    indexers: impl IntoIterator<Item = (&'a str, &'a dyn Indexer)>,
    query: &Query,
    category: Option<Category>,
) -> Gathered {
    let mut results: Vec<SearchResult> = Vec::new();
    let mut problems = Vec::new();

    for (name, indexer) in indexers {
        match indexer.search(query, category) {
            Ok(found) => {
                for release in found {
                    merge(&mut results, release);
                }
            }
            Err(failure) => problems.push((name.to_string(), failure)),
        }
    }

    Gathered { results, problems }
}

fn merge(results: &mut Vec<SearchResult>, release: SearchResult) {
    let key = identity(&release);
    match results
        .iter_mut()
        .find(|existing| identity(existing) == key)
    {
        Some(existing) => {
            if release.grabs > existing.grabs {
                existing.grabs = release.grabs;
            }
            if existing.cover_url.is_none() {
                existing.cover_url = release.cover_url;
            }
            if existing.imdb.is_none() {
                existing.imdb = release.imdb;
            }
            if existing.about.is_empty() {
                existing.about = release.about;
            }
        }
        None => results.push(release),
    }
}

/// How much a title matches the question.
pub fn relevance(query: &str, title: &str) -> f64 {
    let asked = words(query);
    let found = words(title);
    if asked.is_empty() || found.is_empty() {
        return 0.0;
    }
    if asked == found {
        return 3.0;
    }
    let present = asked
        .iter()
        .filter(|word| found.iter().any(|other| same_word(word, other)))
        .count();
    if present == asked.len() {
        (2.0 - 0.05 * (found.len() - asked.len()) as f64).max(1.05)
    } else {
        present as f64 / asked.len() as f64
    }
}

fn same_word(asked: &str, found: &str) -> bool {
    if asked == found {
        return true;
    }
    let (shorter, longer) = if asked.len() < found.len() {
        (asked, found)
    } else {
        (found, asked)
    };
    shorter.len() >= 4 && longer.len() - shorter.len() <= 2 && longer.starts_with(shorter)
}

fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Diacritics flattened to scene ASCII.
pub fn fold(text: &str) -> String {
    text.chars()
        .map(|letter| match letter {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'O',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ç' => 'c',
            'Ç' => 'C',
            other => other,
        })
        .collect()
}

fn identity(release: &SearchResult) -> (String, u64) {
    let name: String = release
        .title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    (name, release.size_bytes / 1_048_576)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Error, Result};
    use crate::release::tags;

    struct Answers {
        results: Vec<SearchResult>,
        fails: bool,
    }

    impl Indexer for Answers {
        fn search(&self, _query: &Query, _category: Option<Category>) -> Result<Vec<SearchResult>> {
            if self.fails {
                return Err(Error::Refused {
                    what: "the indexer".into(),
                    status: 429,
                    message: "too many requests".into(),
                });
            }
            Ok(self.results.clone())
        }
        fn capabilities(&self) -> Result<String> {
            Ok(String::new())
        }
        fn fetch_nzb(&self, _url: &str) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
        fn cover(&self, _url: &str) -> Result<(String, Vec<u8>)> {
            Ok((String::new(), Vec::new()))
        }
        fn host(&self) -> Option<String> {
            None
        }
    }

    fn release(title: &str, size_mb: u64, grabs: u64) -> SearchResult {
        SearchResult {
            tags: tags(title),
            title: title.to_string(),
            nzb_url: format!("https://one.test/{title}"),
            size_bytes: size_mb * 1_048_576,
            age_days: Some(10.0),
            grabs,
            cover_url: None,
            imdb: None,
            about: String::new(),
            thumbs_up: 0,
            thumbs_down: 0,
        }
    }

    fn answering(results: Vec<SearchResult>) -> Answers {
        Answers {
            results,
            fails: false,
        }
    }

    fn refusing() -> Answers {
        Answers {
            results: Vec::new(),
            fails: true,
        }
    }

    #[test]
    fn puts_what_several_indexers_found_into_one_list() {
        let first = answering(vec![release("A.Film.2020.1080p-X", 2000, 10)]);
        let second = answering(vec![release("Another.Film.2019.720p-Y", 1500, 4)]);
        let gathered = gather(
            [
                ("one", &first as &dyn Indexer),
                ("two", &second as &dyn Indexer),
            ],
            &Query::Title("film".into()),
            None,
        );
        assert_eq!(gathered.results.len(), 2);
        assert!(gathered.problems.is_empty());
    }

    #[test]
    fn the_same_release_from_two_indexers_is_one_release() {
        let first = answering(vec![release("A.Film.2020.1080p-X", 2000, 10)]);
        let second = answering(vec![release("a.film.2020.1080p-x", 2000, 250)]);
        let gathered = gather(
            [
                ("one", &first as &dyn Indexer),
                ("two", &second as &dyn Indexer),
            ],
            &Query::Title("film".into()),
            None,
        );
        assert_eq!(gathered.results.len(), 1);
        assert_eq!(gathered.results[0].grabs, 250, "the better evidence wins");
    }

    #[test]
    fn one_indexer_failing_does_not_take_the_search_with_it() {
        let working = answering(vec![release("A.Film.2020.1080p-X", 2000, 10)]);
        let broken = refusing();
        let gathered = gather(
            [
                ("good", &working as &dyn Indexer),
                ("bad", &broken as &dyn Indexer),
            ],
            &Query::Title("film".into()),
            None,
        );
        assert_eq!(gathered.results.len(), 1);
        assert_eq!(gathered.problems.len(), 1);
        assert_eq!(gathered.problems[0].0, "bad", "named, so it can be fixed");
        assert!(
            matches!(gathered.problems[0].1, Error::Refused { status: 429, .. }),
            "the error arrives whole, so the boundary can say what actually happened"
        );
        assert!(!gathered.is_total_failure());
    }

    #[test]
    fn every_indexer_failing_is_a_failure_rather_than_an_empty_shelf() {
        let broken = refusing();
        let gathered = gather(
            [("bad", &broken as &dyn Indexer)],
            &Query::Title("film".into()),
            None,
        );
        assert!(gathered.is_total_failure());
    }

    #[test]
    fn what_she_asked_for_exactly_outranks_everything_that_merely_contains_it() {
        let exact = relevance("game of thrones", "Game Of Thrones");
        let documentary = relevance("game of thrones", "Game of Thrones The Last Watch");
        let longer = relevance(
            "game of thrones",
            "Game of Thrones Conquest and Rebellion An Animated History of the Seven Kingdoms",
        );
        let loose = relevance("game of thrones", "Purge of Kingdoms");
        let unrelated = relevance("game of thrones", "Coma");
        assert!(exact > documentary, "{exact} vs {documentary}");
        assert!(documentary > longer, "{documentary} vs {longer}");
        assert!(longer > loose, "{longer} vs {loose}");
        assert!(loose > unrelated, "{loose} vs {unrelated}");
        assert_eq!(unrelated, 0.0);
    }

    #[test]
    fn punctuation_and_case_do_not_keep_a_title_from_matching() {
        assert_eq!(relevance("das boot", "Das.Boot"), 3.0);
        assert_eq!(relevance("Cuéntame cómo pasó", "cuéntame cómo pasó"), 3.0);
    }

    #[test]
    fn accents_do_not_keep_her_words_from_matching_scene_names() {
        assert_eq!(relevance("Cuéntame cómo pasó", "Cuentame.Como.Paso"), 3.0);
        assert_eq!(
            relevance("El espíritu de la colmena", "El.Espiritu.De.La.Colmena"),
            3.0
        );
    }

    #[test]
    fn a_letter_a_language_adds_or_drops_is_the_same_word() {
        assert!(relevance("gomorra", "Gomorrah.S03.Bluray.1080p") > 0.0);
        assert!(relevance("gomorrah", "Gomorra.S01.720p.BluRay") > 0.0);
        assert!(
            relevance("gomorra", "Gomorra") > relevance("gomorra", "Gomorrah"),
            "her spelling exactly still comes first"
        );
    }

    #[test]
    fn a_word_that_merely_starts_the_same_is_a_different_word() {
        assert_eq!(relevance("star", "Stargate SG-1"), 0.0);
        assert_eq!(relevance("el sur", "El Suricato Pumpkinhead"), 0.5);
    }

    #[test]
    fn a_mountain_of_extra_words_still_beats_a_partial_match() {
        let buried = relevance(
            "up",
            "Up And Away The Complete Making Of Documentary Extended Edition With Extra Interviews And More",
        );
        assert!(buried > relevance("up and coming", "up"), "{buried}");
    }

    #[test]
    fn metadata_is_taken_from_whichever_indexer_had_it() {
        let bare = answering(vec![release("A.Film.2020.1080p-X", 2000, 10)]);
        let mut richer = release("A.Film.2020.1080p-X", 2000, 5);
        richer.imdb = Some("0082096".into());
        richer.cover_url = Some("https://two.test/cover.jpg".into());
        richer.about = "A Film · 2020".into();
        let detailed = answering(vec![richer]);

        let gathered = gather(
            [
                ("bare", &bare as &dyn Indexer),
                ("detailed", &detailed as &dyn Indexer),
            ],
            &Query::Title("film".into()),
            None,
        );
        assert_eq!(gathered.results.len(), 1);
        assert_eq!(gathered.results[0].imdb.as_deref(), Some("0082096"));
        assert_eq!(gathered.results[0].about, "A Film · 2020");
    }
}
