//! Every sentence the window is allowed to show, built here from what actually happened.
//!
//! The boundary decides what the person sees: deeper layers return an `Error` and this module
//! phrases it. English never crosses this line; the technical detail rides along separately so it
//! can go to the story's `why` and the log instead of her screen.

use mamacine_core::error::Error;

/// A failure as the two audiences need it: `said` for her, `why` for whoever is fixing the app.
pub struct Explained {
    pub said: String,
    pub why: String,
}

/// The services she knows by role, not by hostname.
fn role_of(what: &str) -> &str {
    match what {
        "nzbget" => "el descargador",
        "the indexer" | "the title lookup" => "el buscador",
        "opensubtitles" | "the subtitle file host" => "el servicio de subtítulos",
        "the film database" => "el buscador de fichas",
        // anything else is a hostname: name the connection, not the machine
        _ => "internet",
    }
}

pub fn explain(error: &Error) -> Explained {
    let why = error.to_string();
    let said = match error {
        // the downloader runs inside this very app: telling her to check the internet for it
        // was wrong advice about a local fact
        Error::Unreachable { what, .. } if what == "nzbget" => {
            "El descargador de la aplicación no responde. Cierra la aplicación del todo y vuelve a abrirla."
                .to_string()
        }
        Error::Unreachable { what, .. } => match role_of(what) {
            "internet" => {
                "No hay conexión. Comprueba que internet funciona y vuelve a probar.".to_string()
            }
            role => format!("No consigo conectarme con {role}. Comprueba que internet funciona."),
        },
        Error::Refused { what, status, .. } => match status {
            401 | 403 | 100..=199 => format!(
                "{} ha rechazado la clave. Hay que revisar los ajustes.",
                capitalised(role_of(what))
            ),
            429 => format!(
                "{} dice que hemos pedido demasiadas cosas por hoy. Prueba mañana.",
                capitalised(role_of(what))
            ),
            _ => format!(
                "{} no ha aceptado la petición. Vuelve a probarlo en un rato.",
                capitalised(role_of(what))
            ),
        },
        Error::Unreadable { what, .. } => format!(
            "{} ha contestado algo que no he entendido. Vuelve a probarlo en un rato.",
            capitalised(role_of(what))
        ),
        // already phrased for a person by whoever raised it
        Error::Setup(message) => message.clone(),
        Error::Io(_) => "Algo ha fallado en este ordenador. Vuelve a probarlo.".to_string(),
    };
    Explained { said, why }
}

fn capitalised(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}

/// Giving up, with the real numbers. "Todo lo que había venía dañado" hid how much was tried,
/// and claimed everything was tried when it was not: she can hold a count, and it is hers.
pub fn gave_up(series: bool, tried: usize, untried: usize) -> String {
    let thing = if series {
        "esta temporada"
    } else {
        "esta película"
    };
    let what_happened = match (tried, untried) {
        (0 | 1, 0) => "la única copia que había venía dañada".to_string(),
        (tried, 0) => format!("he probado las {tried} copias que había y todas venían dañadas"),
        (tried, untried) => {
            format!("he probado {tried} copias y todas venían dañadas; quedan {untried} sin probar")
        }
    };
    format!("No he podido conseguir {thing}: {what_happened}. Vuelve a probar dentro de unos días.")
}

/// The server rejecting the account and the server being unreachable are different facts with
/// different remedies, and neither is terminal: the chase waits and keeps trying on its own, and
/// both sentences say so, because an instruction with no named next step is a dead end.
pub const SERVER_REFUSED: &str = "El servidor de descargas ha rechazado el usuario o la contraseña.      Hay que revisar los ajustes; en cuanto estén bien, sigo yo solo.";

pub const SERVER_UNREACHABLE: &str = "No consigo conectarme al servidor de descargas. Puede que      ahora mismo no haya internet. Lo sigo intentando yo solo.";

pub const GAVE_UP_ON_THIS_COPY: &str = "Esa descarga venía dañada, así que la he descartado.";

pub const CANCELLED: &str = "Has cancelado la descarga.";

/// Every remaining copy is either proven dead or visibly incomplete on the server: starting any
/// of them would only spend her bandwidth on a known outcome.
pub const NO_WORKING_COPY: &str =
    "Ninguna de las copias que quedan funciona ahora mismo. Vuelve a probar dentro de unos días.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_key_is_blamed_on_the_settings_not_on_the_copies() {
        let explained = explain(&Error::Refused {
            what: "the indexer".into(),
            status: 100,
            message: "Incorrect user credentials".into(),
        });
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
            let said = explain(&error).said;
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
    fn the_technical_reason_is_kept_beside_the_sentence_not_inside_it() {
        let explained = explain(&Error::Unreachable {
            what: "news.eweka.nl".into(),
            detail: "dns error".into(),
        });
        assert!(!explained.said.contains("dns"), "{}", explained.said);
        assert!(explained.why.contains("dns error"));
    }

    // "Todo lo que había venía dañado" hid the count and overclaimed. She can hold a number.
    #[test]
    fn giving_up_says_how_many_were_tried_and_how_many_were_not() {
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
