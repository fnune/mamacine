use crate::text::Lang;
use mamacine_core::error::Error;

pub struct Explained {
    pub said: String,
    pub why: String,
}

pub fn explain(error: &Error, lang: Lang) -> Explained {
    let why = error.to_string();
    let said = match error {
        Error::Unreachable { what, .. } if what == "nzbget" => {
            lang.downloader_not_answering().to_string()
        }
        Error::Unreachable { what, .. } => match lang.role(what) {
            "internet" | "the internet" => lang.no_connection().to_string(),
            role => lang.cannot_reach(role),
        },
        Error::Refused { what, status, .. } => match status {
            401 | 403 | 100..=199 => lang.rejected_the_key(&subject_of(what, lang)),
            429 => lang.too_many_requests(&subject_of(what, lang)),
            _ => lang.refused_the_request(&subject_of(what, lang)),
        },
        Error::Unreadable { what, .. } => lang.answered_nonsense(&subject_of(what, lang)),
        Error::Setup(message) => message.clone(),
        Error::Io(_) => lang.this_computer_failed().to_string(),
    };
    Explained { said, why }
}

fn subject_of(what: &str, lang: Lang) -> String {
    match lang.role(what) {
        "internet" | "the internet" => lang.some_site().to_string(),
        role => capitalised(role),
    }
}

fn capitalised(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_key_is_blamed_on_the_settings_not_on_the_copies() {
        let explained = explain(
            &Error::Refused {
                what: "the indexer".into(),
                status: 100,
                message: "Incorrect user credentials".into(),
            },
            Lang::Es,
        );
        assert!(
            explained.said.contains("rechazado la clave"),
            "{}",
            explained.said
        );
        assert!(explained.said.contains("ajustes"), "{}", explained.said);
        assert!(explained.why.contains("Incorrect user credentials"));
    }

    #[test]
    fn nothing_english_survives_into_what_she_reads() {
        let errors = [
            Error::Unreachable {
                what: "api.nzbgeek.info".into(),
                detail: "connection refused".into(),
            },
            Error::Refused {
                what: "nzbget".into(),
                status: 429,
                message: "too many requests".into(),
            },
            Error::Unreadable {
                what: "the indexer".into(),
                detail: "expected value at line 1".into(),
            },
        ];
        for error in errors {
            let said = explain(&error, Lang::Es).said;
            for english in [
                "cannot",
                "refused",
                "request",
                "unreadable",
                "sent something",
            ] {
                assert!(!said.contains(english), "{said}");
            }
        }
    }

    #[test]
    fn the_same_refusal_reads_in_english_for_an_english_interface() {
        let said = explain(
            &Error::Refused {
                what: "the indexer".into(),
                status: 401,
                message: "bad key".into(),
            },
            Lang::En,
        )
        .said;
        assert!(said.contains("rejected the key"), "{said}");
    }

    #[test]
    fn the_technical_reason_is_kept_beside_the_sentence_not_inside_it() {
        let explained = explain(
            &Error::Unreachable {
                what: "news.eweka.nl".into(),
                detail: "dns error".into(),
            },
            Lang::Es,
        );
        assert!(!explained.said.contains("dns"), "{}", explained.said);
        assert!(explained.why.contains("dns error"));
    }

    #[test]
    fn giving_up_says_how_many_were_tried_and_how_many_were_not() {
        let gave_up = |series, tried, untried| Lang::Es.gave_up(series, tried, untried);
        assert!(gave_up(false, 1, 0).contains("la única copia"));
        assert!(gave_up(false, 3, 0).contains("las 3 copias que había"));
        let capped = gave_up(false, 3, 4);
        assert!(capped.contains("he probado 3 copias"), "{capped}");
        assert!(capped.contains("quedan 4 sin probar"), "{capped}");
        assert!(!capped.contains("todo lo que había"), "{capped}");
        assert!(gave_up(true, 2, 0).contains("esta temporada"));
        assert!(!gave_up(true, 2, 0).contains("película"));
    }
}
