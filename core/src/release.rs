//! Reading a release name for language claims.

use regex::Regex;
use std::sync::OnceLock;

/// Absolute claims; preference-free until interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// SPANISH is `Dub("es")`, TRUEFRENCH `Dub("fr")`.
    Dub(&'static str),
    /// A market's other dub: LATINO, VFQ.
    Variant(&'static str),
    /// DUAL or MULTI: original plus some dub.
    Dual,
    /// VOSE: original audio with subtitles.
    Subbed,
    /// A language the profiles do not cover.
    OtherLanguage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    Any,
    /// A dub in this language, by code.
    Language(&'static str),
    Original,
}

struct Profile {
    code: &'static str,
    dub: &'static str,
    variant: Option<&'static str>,
}

const PROFILES: &[Profile] = &[
    Profile {
        code: "es",
        dub: r"\b(spanish|castellano|espanol|español|cast)\b",
        variant: Some(r"\b(latino|latin|latam|mexican)\b"),
    },
    Profile {
        code: "fr",
        dub: r"\b(fre|french|truefrench|vff)\b",
        variant: Some(r"\b(vfq)\b"),
    },
    Profile {
        code: "de",
        dub: r"\b(ger|german|deutsch)\b",
        variant: None,
    },
    Profile {
        code: "it",
        dub: r"\b(ita|italian)\b",
        variant: None,
    },
    Profile {
        code: "pt",
        dub: r"\b(portuguese|portugues|português)\b",
        variant: Some(r"\b(brazilian)\b"),
    },
    Profile {
        code: "nl",
        dub: r"\b(dutch)\b",
        variant: None,
    },
    Profile {
        code: "pl",
        dub: r"\b(pol|polish)\b",
        variant: None,
    },
    Profile {
        code: "cs",
        dub: r"\b(cze|czech)\b",
        variant: None,
    },
    Profile {
        code: "hu",
        dub: r"\b(hun|hungarian)\b",
        variant: None,
    },
    Profile {
        code: "ru",
        dub: r"\b(rus|russian)\b",
        variant: None,
    },
    Profile {
        code: "tr",
        dub: r"\b(tur|turkish)\b",
        variant: None,
    },
    Profile {
        code: "hi",
        dub: r"\b(hindi)\b",
        variant: None,
    },
    Profile {
        code: "ta",
        dub: r"\b(tamil)\b",
        variant: None,
    },
    Profile {
        code: "te",
        dub: r"\b(telugu)\b",
        variant: None,
    },
    Profile {
        code: "ko",
        dub: r"\b(kor|korean)\b",
        variant: None,
    },
    Profile {
        code: "ja",
        dub: r"\b(jap|japanese|jpn)\b",
        variant: None,
    },
    Profile {
        code: "sv",
        dub: r"\b(swedish)\b",
        variant: None,
    },
    Profile {
        code: "da",
        dub: r"\b(danish)\b",
        variant: None,
    },
    Profile {
        code: "fi",
        dub: r"\b(finnish)\b",
        variant: None,
    },
    Profile {
        code: "no",
        dub: r"\b(norwegian)\b",
        variant: None,
    },
];

/// The canonical code, when the profiles know it.
pub fn known_language(code: &str) -> Option<&'static str> {
    PROFILES
        .iter()
        .map(|profile| profile.code)
        .find(|known| *known == code)
}

fn pattern(cell: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(source).expect("pattern compiles"))
}

struct Compiled {
    code: &'static str,
    dub: Regex,
    variant: Option<Regex>,
}

fn compiled() -> &'static [Compiled] {
    static COMPILED: OnceLock<Vec<Compiled>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        PROFILES
            .iter()
            .map(|profile| Compiled {
                code: profile.code,
                dub: Regex::new(profile.dub).expect("pattern compiles"),
                variant: profile
                    .variant
                    .map(|source| Regex::new(source).expect("pattern compiles")),
            })
            .collect()
    })
}

pub fn tags(title: &str) -> Vec<Tag> {
    static DUAL: OnceLock<Regex> = OnceLock::new();
    static SUBBED: OnceLock<Regex> = OnceLock::new();
    static OTHER: OnceLock<Regex> = OnceLock::new();

    let title = title.to_lowercase();
    let mut found = Vec::new();
    for profile in compiled() {
        if profile.dub.is_match(&title) {
            found.push(Tag::Dub(profile.code));
        }
        if let Some(variant) = &profile.variant {
            if variant.is_match(&title) {
                found.push(Tag::Variant(profile.code));
            }
        }
    }
    if pattern(&DUAL, r"\b(dual|multi|multi\d+)\b").is_match(&title) {
        found.push(Tag::Dual);
    }
    if pattern(&SUBBED, r"\b(vose|vos|subtitulado|subs|subbed)\b").is_match(&title) {
        found.push(Tag::Subbed);
    }
    if pattern(&OTHER, r"\b(vostfr|nordic)\b").is_match(&title) {
        found.push(Tag::OtherLanguage);
    }
    found
}

/// Every language the name claims, as codes; the boundary owns the nouns.
pub fn languages_claimed(title: &str) -> Vec<&'static str> {
    // never bare "por": it is an everyday Spanish word, and film titles carry it
    const NAMES: &[(&str, &str)] = &[
        (r"\b(spanish|castellano|espanol|español|cast)\b", "es"),
        (r"\b(latino|latin|latam|mexican)\b", "es-419"),
        (r"\b(eng|english|vo)\b", "en"),
        (r"\b(ita|italian)\b", "it"),
        (r"\b(fre|french|truefrench|vff|vfq)\b", "fr"),
        (r"\b(ger|german|deutsch)\b", "de"),
        (r"\b(dutch|nld|nl)\b", "nl"),
        (r"\b(pol|polish)\b", "pl"),
        (r"\b(cze|czech)\b", "cs"),
        (r"\b(hun|hungarian)\b", "hu"),
        (r"\b(rus|russian)\b", "ru"),
        (r"\b(tur|turkish)\b", "tr"),
        (r"\b(hindi|tamil|telugu)\b", "hi"),
        (r"\b(kor|korean)\b", "ko"),
        (r"\b(jap|japanese|jpn)\b", "ja"),
        (r"\b(brazilian|portuguese|portugues|português)\b", "pt"),
        (r"\b(swedish|danish|finnish|norwegian|nordic)\b", "nordic"),
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

/// Claims a dub in some other language.
pub fn names_another_language(tags: &[Tag], code: &str) -> bool {
    tags.iter().any(|tag| match tag {
        Tag::Dub(named) | Tag::Variant(named) => *named != code,
        Tag::OtherLanguage => true,
        _ => false,
    })
}

pub fn matches(tags: &[Tag], preference: Preference) -> bool {
    let has = |tag: Tag| tags.contains(&tag);
    match preference {
        Preference::Any => true,
        Preference::Language(code) => {
            has(Tag::Dub(code))
                || has(Tag::Variant(code))
                || (has(Tag::Dual) && !names_another_language(tags, code))
        }
        Preference::Original => !tags.iter().any(|tag| {
            matches!(
                tag,
                Tag::Dub(_) | Tag::Variant(_) | Tag::Dual | Tag::OtherLanguage
            )
        }),
    }
}

/// Keeps the words older library records wrote.
impl serde::Serialize for Tag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Tag::Dub("es") => serializer.serialize_str("spanish"),
            Tag::Variant("es") => serializer.serialize_str("latino"),
            Tag::Dub(code) => serializer.serialize_str(&format!("dub:{code}")),
            Tag::Variant(code) => serializer.serialize_str(&format!("variant:{code}")),
            Tag::Dual => serializer.serialize_str("dual"),
            Tag::Subbed => serializer.serialize_str("subbed"),
            Tag::OtherLanguage => serializer.serialize_str("other_language"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Tag {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let said = String::deserialize(deserializer)?;
        Ok(match said.as_str() {
            "spanish" => Tag::Dub("es"),
            "latino" => Tag::Variant("es"),
            "dual" => Tag::Dual,
            "subbed" => Tag::Subbed,
            other => {
                let named = other
                    .strip_prefix("dub:")
                    .map(|code| (code, false))
                    .or_else(|| other.strip_prefix("variant:").map(|code| (code, true)));
                match named
                    .and_then(|(code, variant)| known_language(code).map(|code| (code, variant)))
                {
                    Some((code, false)) => Tag::Dub(code),
                    Some((code, true)) => Tag::Variant(code),
                    None => Tag::OtherLanguage,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spanish(title: &str) -> bool {
        matches(&tags(title), Preference::Language("es"))
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
            assert!(tags(title).contains(&Tag::Dub("es")), "{title}");
            assert!(spanish(title), "{title}");
        }
    }

    #[test]
    fn latino_is_told_apart_from_castellano_and_both_count_as_spanish() {
        let found = tags("Pelicula.2020.LATINO.1080p");
        assert!(found.contains(&Tag::Variant("es")));
        assert!(!found.contains(&Tag::Dub("es")));
        assert!(spanish("Pelicula.2020.LATINO.1080p"));
    }

    #[test]
    fn dual_audio_counts_unless_it_names_another_language() {
        assert!(spanish("Film.2020.DUAL.1080p"));
        assert!(spanish("Film.2020.DUAL.LATINO.1080p"));
        assert!(
            !spanish("Film.2024.720p.ITA-ENG.MULTI.x264"),
            "an Italian-English dual is not a Spanish release"
        );
    }

    #[test]
    fn explicit_spanish_survives_other_language_markers() {
        assert!(spanish("Film.2024.SPANISH.ITA.MULTI.1080p"));
    }

    #[test]
    fn another_households_language_is_matched_by_the_same_rules() {
        let french = |title: &str| matches(&tags(title), Preference::Language("fr"));
        assert!(french("Film.2024.TRUEFRENCH.1080p"));
        assert!(french("Film.2024.MULTI.1080p"));
        assert!(
            french("Film.2024.MULTI.VFQ.1080p"),
            "the Québec dub is French audio, exactly as latino is Spanish audio"
        );
        assert!(!french("Film.2024.SPANISH.1080p"));
        assert!(
            !spanish("Film.2024.TRUEFRENCH.1080p"),
            "and a French dub is another language to a Spanish household"
        );
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
    fn a_copy_that_names_another_language_is_that_dub_and_not_the_original() {
        for dubbed in [
            "La.Virgen.Roja.2024.ITA.1080p.WEB-DL",
            "La.Virgen.Roja.2024.TRUEFRENCH.1080p",
            "Film.2024.German.DL.1080p.BluRay",
        ] {
            assert!(names_another_language(&tags(dubbed), "es"), "{dubbed}");
            assert!(!original(dubbed), "{dubbed}");
        }
        assert!(
            original("La.Virgen.Roja.2024.1080p.WEB-DL.x264"),
            "a copy that names no language at all is still the honest guess at the original"
        );
    }

    #[test]
    fn an_english_marker_never_reads_as_a_foreign_dub() {
        assert!(original("Film.2020.ENG.1080p.BluRay.x264"));
        assert!(!names_another_language(&tags("Film.2020.ENG.1080p"), "es"));
    }

    #[test]
    fn a_copy_is_named_in_the_languages_its_own_name_carries() {
        assert_eq!(languages_claimed("La.Virgen.Roja.2024.ITA.1080p"), ["it"]);
        assert_eq!(
            languages_claimed("La.Virgen.Roja.2024.TRUEFRENCH.1080p"),
            ["fr"]
        );
        assert_eq!(
            languages_claimed("Film.2024.720p.ITA-ENG.MULTI.x264"),
            ["it", "en"],
            "a copy that carries two is read as two, in the order its name says them"
        );
        assert_eq!(languages_claimed("Das.Boot.1981.SPANISH.1080p"), ["es"]);
        assert_eq!(languages_claimed("Film.2020.LATINO.1080p"), ["es-419"]);
        assert!(
            languages_claimed("La.Virgen.Roja.2024.1080p.WEB-DL.x264").is_empty(),
            "a name that claims no language claims none"
        );
    }

    #[test]
    fn subtitled_original_stays_in_original() {
        let found = tags("Film.2020.VOSE.1080p.BluRay");
        assert!(found.contains(&Tag::Subbed));
        assert!(matches(&found, Preference::Original));
    }

    #[test]
    fn a_tag_reads_back_as_itself_and_an_older_records_words_still_read() {
        for tag in [
            Tag::Dub("es"),
            Tag::Variant("es"),
            Tag::Dub("fr"),
            Tag::Variant("fr"),
            Tag::Dual,
            Tag::Subbed,
            Tag::OtherLanguage,
        ] {
            let written = serde_json::to_string(&tag).expect("serializable");
            let read: Tag = serde_json::from_str(&written).expect("readable");
            assert_eq!(read, tag, "{written}");
        }
        assert_eq!(
            serde_json::to_string(&Tag::Dub("es")).expect("serializable"),
            "\"spanish\"",
            "the word already in her records"
        );
        let old: Vec<Tag> = serde_json::from_str(r#"["spanish","latino","dual","other_language"]"#)
            .expect("an older record reads");
        assert_eq!(
            old,
            [
                Tag::Dub("es"),
                Tag::Variant("es"),
                Tag::Dual,
                Tag::OtherLanguage
            ]
        );
        let unknown: Tag = serde_json::from_str("\"dub:xx\"").expect("readable");
        assert_eq!(
            unknown,
            Tag::OtherLanguage,
            "a word this version does not know must not refuse the record it sits in"
        );
    }
}
