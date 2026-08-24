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

/// Every language a release name claims, in her words, in the order they appear in the name.
///
/// The tag says only that a copy is in some language other than the ones the app sorts by, and
/// "En otro idioma" is not something anybody can choose with: told that, she still has to
/// download it to find out. A name that carries two is a copy that carries two, and both are
/// worth reading before she spends an hour on it.
pub fn languages_named(title: &str) -> Vec<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        (r"\b(spanish|castellano|espanol|español|cast)\b", "español"),
        (r"\b(latino|latin|latam|mexican)\b", "español latino"),
        (r"\b(eng|english|vo)\b", "inglés"),
        (r"\b(ita|italian)\b", "italiano"),
        (r"\b(fre|french|truefrench|vff|vfq)\b", "francés"),
        (r"\b(ger|german|deutsch)\b", "alemán"),
        (r"\b(dutch|nld|nl)\b", "neerlandés"),
        (r"\b(pol|polish)\b", "polaco"),
        (r"\b(cze|czech)\b", "checo"),
        (r"\b(hun|hungarian)\b", "húngaro"),
        (r"\b(rus|russian)\b", "ruso"),
        (r"\b(tur|turkish)\b", "turco"),
        (r"\b(hindi|tamil|telugu)\b", "hindi"),
        (r"\b(kor|korean)\b", "coreano"),
        (r"\b(jap|japanese|jpn)\b", "japonés"),
        (r"\b(brazilian|portuguese|por)\b", "portugués"),
        (r"\b(swedish|danish|finnish|norwegian|nordic)\b", "nórdico"),
    ];
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        NAMES
            .iter()
            .map(|(source, _)| Regex::new(source).expect("pattern compiles"))
            .collect()
    });
    let title = title.to_lowercase();
    let mut found: Vec<(usize, &'static str)> = Vec::new();
    for (pattern, (_, name)) in patterns.iter().zip(NAMES) {
        if let Some(at) = pattern.find(&title) {
            if !found.iter().any(|(_, already)| already == name) {
                found.push((at.start(), name));
            }
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, name)| name).collect()
}

pub fn matches(tags: &[Tag], preference: Preference) -> bool {
    let has = |tag: Tag| tags.contains(&tag);
    match preference {
        Preference::Any => true,
        Preference::Spanish => {
            has(Tag::Spanish) || (has(Tag::Dual) && !has(Tag::Latino) && !has(Tag::OtherLanguage))
        }
        // A copy that names a language is that language's dub. Leaving `OtherLanguage` out of
        // this offered an Italian copy of a Spanish film as the original, called it "Versión
        // original" on the screen, and the next copy it fell through to was French.
        Preference::Original => {
            !(has(Tag::Spanish) || has(Tag::Latino) || has(Tag::Dual) || has(Tag::OtherLanguage))
        }
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

    // She asked for the original and got twenty minutes into an Italian dub of a Spanish film,
    // under a screen that said "Versión original". The copy after it was French.
    #[test]
    fn a_copy_that_names_another_language_is_that_dub_and_not_the_original() {
        for dubbed in [
            "La.Virgen.Roja.2024.ITA.1080p.WEB-DL",
            "La.Virgen.Roja.2024.TRUEFRENCH.1080p",
            "Film.2024.German.DL.1080p.BluRay",
        ] {
            assert!(tags(dubbed).contains(&Tag::OtherLanguage), "{dubbed}");
            assert!(!original(dubbed), "{dubbed}");
        }
        assert!(
            original("La.Virgen.Roja.2024.1080p.WEB-DL.x264"),
            "a copy that names no language at all is still the honest guess at the original"
        );
    }

    // "En otro idioma" told her the copy was wrong without telling her what it was, so the only
    // way to find out was still to download it and watch it.
    #[test]
    fn a_copy_is_named_in_the_languages_its_own_name_carries() {
        assert_eq!(
            languages_named("La.Virgen.Roja.2024.ITA.1080p"),
            ["italiano"]
        );
        assert_eq!(
            languages_named("La.Virgen.Roja.2024.TRUEFRENCH.1080p"),
            ["francés"]
        );
        assert_eq!(
            languages_named("Film.2024.720p.ITA-ENG.MULTI.x264"),
            ["italiano", "inglés"],
            "a copy that carries two is read as two, in the order its name says them"
        );
        assert_eq!(languages_named("Das.Boot.1981.SPANISH.1080p"), ["español"]);
        assert!(
            languages_named("La.Virgen.Roja.2024.1080p.WEB-DL.x264").is_empty(),
            "a name that claims no language claims none"
        );
    }

    #[test]
    fn subtitled_original_stays_in_original() {
        let found = tags("Film.2020.VOSE.1080p.BluRay");
        assert!(found.contains(&Tag::Subbed));
        assert!(matches(&found, Preference::Original));
    }
}
