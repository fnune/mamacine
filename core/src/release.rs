//! Reading a release name. Scene naming is all the indexer gives us about language.

use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tag {
    /// SPANISH or CASTELLANO: the dub from Spain, which is the one she wants.
    Spanish,
    /// LATINO: a different dub entirely.
    Latino,
    /// DUAL or MULTI: the original plus some dub, not necessarily Spanish.
    Dual,
    /// VOSE: original audio with Spanish subtitles.
    Subbed,
    /// Names another language outright, such as ITA or TRUEFRENCH.
    OtherLanguage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    Any,
    Spanish,
    Original,
}

fn pattern(cell: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(source).expect("pattern compiles"))
}

pub fn tags(title: &str) -> Vec<Tag> {
    static SPANISH: OnceLock<Regex> = OnceLock::new();
    static LATINO: OnceLock<Regex> = OnceLock::new();
    static DUAL: OnceLock<Regex> = OnceLock::new();
    static SUBBED: OnceLock<Regex> = OnceLock::new();
    static OTHER: OnceLock<Regex> = OnceLock::new();

    let title = title.to_lowercase();
    let mut found = Vec::new();
    if pattern(&SPANISH, r"\b(spanish|castellano|espanol|español|cast)\b").is_match(&title) {
        found.push(Tag::Spanish);
    }
    if pattern(&LATINO, r"\b(latino|latin|latam|mexican)\b").is_match(&title) {
        found.push(Tag::Latino);
    }
    if pattern(&DUAL, r"\b(dual|multi|multi\d+)\b").is_match(&title) {
        found.push(Tag::Dual);
    }
    if pattern(&SUBBED, r"\b(vose|vos|subtitulado|subs|subbed)\b").is_match(&title) {
        found.push(Tag::Subbed);
    }
    let others = r"\b(ita|italian|fre|french|truefrench|vostfr|ger|german|deutsch|dutch|pol|polish|\
cze|czech|hun|hungarian|rus|russian|tur|turkish|hindi|tamil|telugu|kor|korean|jap|japanese|\
nordic|swedish|danish|finnish|norwegian|brazilian|portuguese)\b";
    if pattern(&OTHER, others).is_match(&title) {
        found.push(Tag::OtherLanguage);
    }
    found
}

pub fn matches(tags: &[Tag], preference: Preference) -> bool {
    let has = |tag: Tag| tags.contains(&tag);
    match preference {
        Preference::Any => true,
        Preference::Spanish => {
            has(Tag::Spanish) || (has(Tag::Dual) && !has(Tag::Latino) && !has(Tag::OtherLanguage))
        }
        Preference::Original => !(has(Tag::Spanish) || has(Tag::Latino) || has(Tag::Dual)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spanish(title: &str) -> bool {
        matches(&tags(title), Preference::Spanish)
    }

    fn original(title: &str) -> bool {
        matches(&tags(title), Preference::Original)
    }

    #[test]
    fn recognises_the_dub_from_spain() {
        for title in [
            "Das.Boot.1981.SPANISH.1080p",
            "Pelicula.2020.CASTELLANO.720p",
            "Pelicula.2021.Espanol.1080p",
        ] {
            assert!(tags(title).contains(&Tag::Spanish), "{title}");
            assert!(spanish(title), "{title}");
        }
    }

    #[test]
    fn keeps_latino_apart_from_castellano() {
        let found = tags("Pelicula.2020.LATINO.1080p");
        assert!(found.contains(&Tag::Latino));
        assert!(!found.contains(&Tag::Spanish));
        assert!(!spanish("Pelicula.2020.LATINO.1080p"));
    }

    #[test]
    fn dual_audio_counts_unless_it_names_another_language() {
        assert!(spanish("Film.2020.DUAL.1080p"));
        assert!(!spanish("Film.2020.DUAL.LATINO.1080p"));
        assert!(
            !spanish("Film.2024.720p.ITA-ENG.MULTI.x264"),
            "an Italian-English dual is not a Castellano release"
        );
    }

    #[test]
    fn explicit_spanish_survives_other_language_markers() {
        assert!(spanish("Film.2024.SPANISH.ITA.MULTI.1080p"));
    }

    #[test]
    fn original_excludes_every_dub() {
        assert!(original("Film.2020.1080p.BluRay.x264"));
        for dubbed in [
            "Film.2020.SPANISH.1080p",
            "Film.2020.LATINO.1080p",
            "Film.2020.DUAL.1080p",
        ] {
            assert!(!original(dubbed), "{dubbed}");
        }
    }

    #[test]
    fn subtitled_original_stays_in_original() {
        let found = tags("Film.2020.VOSE.1080p.BluRay");
        assert!(found.contains(&Tag::Subbed));
        assert!(matches(&found, Preference::Original));
    }
}
