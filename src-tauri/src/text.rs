//! Every sentence the backend says, in every language it speaks.
//!
//! Adding a language is adding a variant to `Lang`: the compiler then names every match that
//! needs the new translation, and `just check` passes when the language is complete.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    /// The language the interface speaks, from the setting or this computer, English when the
    /// computer speaks something the interface cannot.
    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "es" => Some(Lang::Es),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }
}

/// The noun a language code answers to on screen.
pub fn language_noun(lang: Lang, code: &str) -> Option<&'static str> {
    let plain = code.to_lowercase();
    let table: &[(&str, &str)] = match lang {
        Lang::Es => &[
            ("es", "español"),
            ("es-419", "español latinoamericano"),
            ("en", "inglés"),
            ("it", "italiano"),
            ("fr", "francés"),
            ("de", "alemán"),
            ("nl", "neerlandés"),
            ("pl", "polaco"),
            ("cs", "checo"),
            ("hu", "húngaro"),
            ("ru", "ruso"),
            ("tr", "turco"),
            ("hi", "hindi"),
            ("ko", "coreano"),
            ("ja", "japonés"),
            ("pt", "portugués"),
            ("pt-br", "portugués brasileño"),
            ("nordic", "nórdico"),
        ],
        Lang::En => &[
            ("es", "Spanish"),
            ("es-419", "Latin American Spanish"),
            ("en", "English"),
            ("it", "Italian"),
            ("fr", "French"),
            ("de", "German"),
            ("nl", "Dutch"),
            ("pl", "Polish"),
            ("cs", "Czech"),
            ("hu", "Hungarian"),
            ("ru", "Russian"),
            ("tr", "Turkish"),
            ("hi", "Hindi"),
            ("ko", "Korean"),
            ("ja", "Japanese"),
            ("pt", "Portuguese"),
            ("pt-br", "Brazilian Portuguese"),
            ("nordic", "Nordic"),
        ],
    };
    table
        .iter()
        .find(|(known, _)| *known == plain)
        .map(|(_, noun)| *noun)
}

impl Lang {
    // --- errors, from messages::explain -------------------------------------------

    pub fn downloader_not_answering(self) -> &'static str {
        match self {
            Lang::Es => "El descargador de la aplicación no responde. Cierra la aplicación del todo y vuelve a abrirla.",
            Lang::En => "The app's downloader is not answering. Close the app completely and open it again.",
        }
    }

    pub fn no_connection(self) -> &'static str {
        match self {
            Lang::Es => "No hay conexión. Comprueba que internet funciona y vuelve a probar.",
            Lang::En => "No connection. Check that the internet works and try again.",
        }
    }

    pub fn cannot_reach(self, role: &str) -> String {
        match self {
            Lang::Es => {
                format!("No consigo conectarme con {role}. Comprueba que internet funciona.")
            }
            Lang::En => format!("I cannot reach {role}. Check that the internet works."),
        }
    }

    pub fn rejected_the_key(self, subject: &str) -> String {
        match self {
            Lang::Es => format!("{subject} ha rechazado la clave. Hay que revisar los ajustes."),
            Lang::En => format!("{subject} rejected the key. The settings need looking at."),
        }
    }

    pub fn too_many_requests(self, subject: &str) -> String {
        match self {
            Lang::Es => format!(
                "{subject} dice que hemos pedido demasiadas cosas por hoy. Vuelve a probar mañana."
            ),
            Lang::En => {
                format!("{subject} says we have asked for too much today. Try again tomorrow.")
            }
        }
    }

    pub fn refused_the_request(self, subject: &str) -> String {
        match self {
            Lang::Es => format!(
                "{subject} no ha aceptado lo que le he pedido. Vuelve a probar dentro de un rato."
            ),
            Lang::En => format!("{subject} did not accept what I asked for. Try again in a while."),
        }
    }

    pub fn answered_nonsense(self, subject: &str) -> String {
        match self {
            Lang::Es => format!(
                "{subject} ha contestado algo que no he entendido. Vuelve a probar dentro de un rato."
            ),
            Lang::En => {
                format!("{subject} answered something I did not understand. Try again in a while.")
            }
        }
    }

    pub fn this_computer_failed(self) -> &'static str {
        match self {
            Lang::Es => "Algo ha fallado en este ordenador. Vuelve a probar.",
            Lang::En => "Something failed on this computer. Try again.",
        }
    }

    pub fn role(self, what: &str) -> &'static str {
        match (self, what) {
            (Lang::Es, "nzbget") => "el descargador",
            (Lang::Es, "the indexer" | "the title lookup") => "el buscador",
            (Lang::Es, "opensubtitles" | "the subtitle file host") => "el servicio de subtítulos",
            (Lang::Es, "the film database") => "el buscador de fichas",
            (Lang::Es, _) => "internet",
            (Lang::En, "nzbget") => "the downloader",
            (Lang::En, "the indexer" | "the title lookup") => "the indexer",
            (Lang::En, "opensubtitles" | "the subtitle file host") => "the subtitle service",
            (Lang::En, "the film database") => "the film database",
            (Lang::En, _) => "the internet",
        }
    }

    pub fn some_site(self) -> &'static str {
        match self {
            Lang::Es => "Un sitio de internet",
            Lang::En => "A site on the internet",
        }
    }

    // --- giving up and carrying on -------------------------------------------------

    pub fn gave_up(self, series: bool, tried: usize, untried: usize) -> String {
        match self {
            Lang::Es => {
                let thing = if series {
                    "esta temporada"
                } else {
                    "esta película"
                };
                let what_happened = match (tried, untried) {
                    (0 | 1, 0) => "la única copia que había estaba estropeada".to_string(),
                    (tried, 0) => {
                        format!("he probado las {tried} copias que había y todas estaban estropeadas")
                    }
                    (tried, untried) => format!(
                        "he probado {tried} copias y todas estaban estropeadas; quedan {untried} sin probar"
                    ),
                };
                format!(
                    "No he podido conseguir {thing}: {what_happened}. Vuelve a probar dentro de unos días."
                )
            }
            Lang::En => {
                let thing = if series { "this season" } else { "this film" };
                let what_happened = match (tried, untried) {
                    (0 | 1, 0) => "the only copy there was arrived broken".to_string(),
                    (tried, 0) => {
                        format!("I tried all {tried} copies there were and every one was broken")
                    }
                    (tried, untried) => format!(
                        "I tried {tried} copies and every one was broken; {untried} remain untried"
                    ),
                };
                format!("I could not get {thing}: {what_happened}. Try again in a few days.")
            }
        }
    }

    pub fn server_refused(self) -> &'static str {
        match self {
            Lang::Es => {
                "El servidor de descargas ha rechazado el usuario o la contraseña. \
    Hay que revisar los ajustes; en cuanto estén bien, sigo yo solo."
            }
            Lang::En => {
                "The download server rejected the user or the password. \
    The settings need looking at; as soon as they are right, I carry on by myself."
            }
        }
    }

    pub fn server_unreachable(self) -> &'static str {
        match self {
            Lang::Es => {
                "No consigo conectarme al servidor de descargas. \
    Puede que ahora mismo no haya internet. Lo sigo intentando yo solo."
            }
            Lang::En => {
                "I cannot reach the download server. \
    There may be no internet right now. I keep trying by myself."
            }
        }
    }

    pub fn gave_up_on_this_copy(self) -> &'static str {
        match self {
            Lang::Es => "Esa copia estaba estropeada, así que la he descartado.",
            Lang::En => "That copy was broken, so I set it aside.",
        }
    }

    pub fn cancelled(self) -> &'static str {
        match self {
            Lang::Es => "Has cancelado la descarga.",
            Lang::En => "You cancelled the download.",
        }
    }

    pub fn no_working_copy(self) -> &'static str {
        match self {
            Lang::Es => "Ninguna de las copias que quedan funciona ahora mismo. Vuelve a probar dentro de unos días.",
            Lang::En => "None of the remaining copies works right now. Try again in a few days.",
        }
    }

    // --- the orchestrator ----------------------------------------------------------

    pub fn no_indexer_configured(self) -> &'static str {
        match self {
            Lang::Es => "No hay ningún buscador configurado. Hay que rellenar los ajustes primero.",
            Lang::En => "No indexer is configured. The settings need filling in first.",
        }
    }

    pub fn title_gone_from_list(self) -> &'static str {
        match self {
            Lang::Es => "Ese título ya no está en la lista.",
            Lang::En => "That title is no longer on the list.",
        }
    }

    pub fn season_gone_from_results(self) -> &'static str {
        match self {
            Lang::Es => "Esa temporada ya no está en los resultados.",
            Lang::En => "That season is no longer in the results.",
        }
    }

    pub fn film_gone_from_results(self) -> &'static str {
        match self {
            Lang::Es => "Esa película ya no está en los resultados.",
            Lang::En => "That film is no longer in the results.",
        }
    }

    pub fn film_gone_from_computer(self) -> &'static str {
        match self {
            Lang::Es => "Esa película ya no está en este ordenador.",
            Lang::En => "That film is no longer on this computer.",
        }
    }

    pub fn too_little_known_for_copies(self) -> &'static str {
        match self {
            Lang::Es => "No sé lo suficiente sobre esta película para buscar otras copias.",
            Lang::En => "I do not know enough about this film to look for other copies.",
        }
    }

    pub fn no_other_copy_found(self) -> &'static str {
        match self {
            Lang::Es => "No he encontrado ninguna otra copia de esta película.",
            Lang::En => "I have not found any other copy of this film.",
        }
    }

    pub fn no_copy_to_download(self) -> &'static str {
        match self {
            Lang::Es => "No hay ninguna copia para descargar.",
            Lang::En => "There is no copy to download.",
        }
    }

    pub fn copy_discarded_for_space(self, size: &str) -> String {
        match self {
            Lang::Es => {
                format!("Una copia de {size} no cabe en el disco, así que la he descartado.")
            }
            Lang::En => format!("A {size} copy does not fit on the disk, so I set it aside."),
        }
    }

    pub fn skipped_to_the_next_copy(self) -> &'static str {
        match self {
            Lang::Es => "No he podido pedir una de las copias, así que he pasado a la siguiente.",
            Lang::En => "I could not ask for one of the copies, so I moved on to the next.",
        }
    }

    pub fn indexer_gone_from_settings(self) -> &'static str {
        match self {
            Lang::Es => "Ese buscador ya no está en los ajustes.",
            Lang::En => "That indexer is no longer in the settings.",
        }
    }

    pub fn download_starts(self, first: bool, size: &str) -> String {
        match (self, first) {
            (Lang::Es, true) => format!("Empieza la descarga ({size})."),
            (Lang::Es, false) => format!("Empieza la descarga de otra copia ({size})."),
            (Lang::En, true) => format!("The download starts ({size})."),
            (Lang::En, false) => format!("The download of another copy starts ({size})."),
        }
    }

    pub fn no_room_on_disk(self, needed: &str, free: &str) -> String {
        match self {
            Lang::Es => format!(
                "No hay sitio en el disco. Hace falta un hueco de unos {needed} y quedan {free}. \
                 Quita alguna película que ya hayas visto y vuelve a intentarlo."
            ),
            Lang::En => format!(
                "There is no room on the disk. About {needed} is needed and {free} remains. \
                 Remove a film you have already watched and try again."
            ),
        }
    }

    pub fn download_could_not_start(self) -> &'static str {
        match self {
            Lang::Es => "No se ha podido empezar la descarga.",
            Lang::En => "The download could not be started.",
        }
    }

    pub fn server_back_trying_copies(self) -> &'static str {
        match self {
            Lang::Es => "Ya puedo conectarme otra vez; sigo probando copias.",
            Lang::En => "I can connect again; I keep trying copies.",
        }
    }

    pub fn download_vanished(self) -> &'static str {
        match self {
            Lang::Es => "Esa descarga se ha perdido por el camino, así que pruebo con otra copia.",
            Lang::En => "That download got lost along the way, so I am trying another copy.",
        }
    }

    pub fn carrying_on_with_the_rest(self) -> &'static str {
        match self {
            Lang::Es => "Sigo con las copias que quedaban.",
            Lang::En => "Carrying on with the copies that remained.",
        }
    }

    pub fn no_copies_left(self) -> &'static str {
        match self {
            Lang::Es => "No quedan más copias que probar.",
            Lang::En => "There are no more copies left to try.",
        }
    }

    pub fn sent_to_the_bin(self) -> &'static str {
        match self {
            Lang::Es => "La has enviado a la papelera. Desde ahí todavía la puedes recuperar.",
            Lang::En => "You sent it to the recycle bin. It can still be recovered from there.",
        }
    }

    pub fn episode_gone(self) -> &'static str {
        match self {
            Lang::Es => "Ese episodio ya no está en la carpeta.",
            Lang::En => "That episode is no longer in the folder.",
        }
    }

    pub fn series_has_no_page(self) -> &'static str {
        match self {
            Lang::Es => "Esta serie no tiene ficha.",
            Lang::En => "This series has no page.",
        }
    }

    pub fn film_has_no_page(self) -> &'static str {
        match self {
            Lang::Es => "Esta película no tiene ficha.",
            Lang::En => "This film has no page.",
        }
    }

    pub fn image_from_forgotten_indexer(self) -> &'static str {
        match self {
            Lang::Es => "Esa imagen viene de un buscador que ya no está en los ajustes.",
            Lang::En => "That image comes from an indexer no longer in the settings.",
        }
    }

    pub fn season_label(self, first: u32, last: u32) -> String {
        match (self, first == last) {
            (Lang::Es, true) => format!("Temporada {first}"),
            (Lang::Es, false) => format!("Temporadas {first} a {last}"),
            (Lang::En, true) => format!("Season {first}"),
            (Lang::En, false) => format!("Seasons {first} to {last}"),
        }
    }

    pub fn season_episode_label(self, season: u32, episode: u32) -> String {
        match self {
            Lang::Es => format!("Temporada {season} · Episodio {episode}"),
            Lang::En => format!("Season {season} · Episode {episode}"),
        }
    }

    pub fn episode_label(self, episode: u32) -> String {
        match self {
            Lang::Es => format!("Episodio {episode}"),
            Lang::En => format!("Episode {episode}"),
        }
    }

    pub fn video_label(self, position: usize) -> String {
        match self {
            Lang::Es => format!("Vídeo {position}"),
            Lang::En => format!("Video {position}"),
        }
    }

    // --- what a copy is, on the versions screen ------------------------------------

    pub fn definition(self, definition: &'static str) -> &'static str {
        match (self, definition) {
            (_, "4K") => "4K",
            (Lang::Es, "1080p") => "Alta definición (1080p)",
            (Lang::Es, "720p") => "Buena calidad (720p)",
            (Lang::Es, _) => "Calidad normal",
            (Lang::En, "1080p") => "High definition (1080p)",
            (Lang::En, "720p") => "Good quality (720p)",
            (Lang::En, _) => "Normal quality",
        }
    }

    pub fn original_with_subtitles(self) -> &'static str {
        match self {
            Lang::Es => "Original con subtítulos",
            Lang::En => "Original with subtitles",
        }
    }

    pub fn with_subtitles(self, languages: &str) -> String {
        match self {
            Lang::Es => format!("{languages}, con subtítulos"),
            Lang::En => format!("{languages}, with subtitles"),
        }
    }

    pub fn language_unknown(self) -> &'static str {
        match self {
            Lang::Es => "Idioma desconocido",
            Lang::En => "Language unknown",
        }
    }

    pub fn two_languages(self) -> &'static str {
        match self {
            Lang::Es => "Dos idiomas",
            Lang::En => "Two languages",
        }
    }

    pub fn original_version(self) -> &'static str {
        match self {
            Lang::Es => "Versión original",
            Lang::En => "Original version",
        }
    }

    pub fn and_the_next(self) -> &'static str {
        match self {
            Lang::Es => " y ",
            Lang::En => " and ",
        }
    }

    // --- time and the tray ---------------------------------------------------------

    pub fn under_a_minute(self) -> &'static str {
        match self {
            Lang::Es => "Menos de un minuto",
            Lang::En => "Under a minute",
        }
    }

    pub fn about_minutes(self, minutes: i64) -> String {
        match self {
            Lang::Es => format!("Unos {minutes} minutos"),
            Lang::En => format!("About {minutes} minutes"),
        }
    }

    pub fn nothing_downloading(self) -> &'static str {
        match self {
            Lang::Es => "No se está descargando nada",
            Lang::En => "Nothing is downloading",
        }
    }

    pub fn downloads_running(self, several: usize) -> String {
        match self {
            Lang::Es => format!("{several} descargas en marcha"),
            Lang::En => format!("{several} downloads running"),
        }
    }

    pub fn downloads_at(self, several: usize, speed: &str) -> String {
        match self {
            Lang::Es => format!("{several} descargas · {speed}"),
            Lang::En => format!("{several} downloads · {speed}"),
        }
    }

    pub fn and_more(self, more: usize) -> String {
        match self {
            Lang::Es => format!("y {more} más"),
            Lang::En => format!("and {more} more"),
        }
    }

    pub fn tray_status(self, status: &'static str, percent: f64) -> String {
        let words = match (self, status) {
            (Lang::Es, "paused") => return format!("En pausa ({percent:.0} %)"),
            (Lang::En, "paused") => return format!("Paused ({percent:.0} %)"),
            (Lang::Es, "queued") => "Empezando la descarga",
            (Lang::Es, "verifying") => "Comprobando que está completa",
            (Lang::Es, "repairing") => "Arreglando lo que falta",
            (Lang::Es, "unpacking") => "Casi lista",
            (Lang::Es, "moving") => "Guardando",
            (Lang::Es, _) => "Últimos detalles",
            (Lang::En, "queued") => "Starting the download",
            (Lang::En, "verifying") => "Checking it is complete",
            (Lang::En, "repairing") => "Repairing what is missing",
            (Lang::En, "unpacking") => "Nearly ready",
            (Lang::En, "moving") => "Saving",
            (Lang::En, _) => "Last details",
        };
        words.to_string()
    }

    pub fn keeps_downloading_title(self) -> &'static str {
        match self {
            Lang::Es => "Mamá Cine sigue descargando",
            Lang::En => "Mamá Cine keeps downloading",
        }
    }

    pub fn keeps_downloading_body(self) -> &'static str {
        match self {
            Lang::Es => "Se queda en el icono pequeño junto al reloj y avisará cuando la película esté lista.",
            Lang::En => "It stays in the small icon by the clock and will say when the film is ready.",
        }
    }

    pub fn open_the_app(self) -> &'static str {
        match self {
            Lang::Es => "Abrir Mamá Cine",
            Lang::En => "Open Mamá Cine",
        }
    }

    pub fn quit_entirely(self) -> &'static str {
        match self {
            Lang::Es => "Salir del todo",
            Lang::En => "Quit entirely",
        }
    }

    // --- finishing: subtitles ------------------------------------------------------

    pub fn subtitles_added(self, saved: usize) -> String {
        match (self, saved) {
            (Lang::Es, 1) => "Subtítulos en español añadidos".to_string(),
            (Lang::Es, saved) => format!("Subtítulos en español añadidos a {saved} episodios"),
            (Lang::En, 1) => "Subtitles added".to_string(),
            (Lang::En, saved) => format!("Subtitles added to {saved} episodes"),
        }
    }

    pub fn allowance_gone(self) -> &'static str {
        match self {
            Lang::Es => "El servicio de subtítulos no deja descargar más por hoy",
            Lang::En => "The subtitle service allows no more downloads today",
        }
    }

    pub fn subtitles_refused(self) -> &'static str {
        match self {
            Lang::Es => "Hay subtítulos, pero ahora mismo no se han podido descargar",
            Lang::En => "Subtitles exist, but they could not be downloaded right now",
        }
    }

    pub fn subtitles_mistimed(self) -> &'static str {
        match self {
            Lang::Es => "Los subtítulos que hay no son de esta copia",
            Lang::En => "The subtitles that exist are not for this copy",
        }
    }

    pub fn no_subtitles_for_this_copy(self) -> &'static str {
        match self {
            Lang::Es => "No hay subtítulos en español para esta copia",
            Lang::En => "There are no subtitles for this copy",
        }
    }

    pub fn subtitles_ready(self) -> &'static str {
        match self {
            Lang::Es => "Subtítulos en español listos",
            Lang::En => "Subtitles ready",
        }
    }

    pub fn subtitles_on_every_episode(self) -> &'static str {
        match self {
            Lang::Es => "Subtítulos en español en todos los episodios",
            Lang::En => "Subtitles on every episode",
        }
    }

    pub fn subtitles_missing_for(self, episodes: &[u32]) -> String {
        match self {
            Lang::Es => format!("Faltan los subtítulos {}", of_episodes_es(episodes)),
            Lang::En => format!("Subtitles are missing for {}", episodes_en(episodes)),
        }
    }

    pub fn subtitles_missing_for_all(self, total: usize) -> String {
        match self {
            Lang::Es => format!("Faltan los subtítulos de los {total} episodios"),
            Lang::En => format!("Subtitles are missing for all {total} episodes"),
        }
    }

    pub fn subtitles_missing_count(self, missing: usize, total: usize) -> String {
        match self {
            Lang::Es => format!("Faltan los subtítulos de {missing} episodios de {total}"),
            Lang::En => format!("Subtitles are missing for {missing} of {total} episodes"),
        }
    }

    pub fn subtitles_already_there(self) -> &'static str {
        match self {
            Lang::Es => "Ya están los subtítulos en español.",
            Lang::En => "The subtitles are already there.",
        }
    }

    pub fn subtitles_were_already_there(self) -> &'static str {
        match self {
            Lang::Es => "Ya estaban los subtítulos en español.",
            Lang::En => "The subtitles were already there.",
        }
    }

    pub fn all_subtitles_already_there(self) -> &'static str {
        match self {
            Lang::Es => "Ya estaban todos los subtítulos.",
            Lang::En => "All the subtitles were already there.",
        }
    }

    pub fn all_subtitles_there_now(self) -> &'static str {
        match self {
            Lang::Es => "Ya están todos los subtítulos.",
            Lang::En => "All the subtitles are there now.",
        }
    }

    pub fn subtitles_fetched_and_missing(self, fetched: &str, missing: &str) -> String {
        match self {
            Lang::Es => {
                format!("Ya están los subtítulos {fetched}. Todavía faltan los {missing}.")
            }
            Lang::En => {
                format!(
                    "Subtitles for {fetched} are there now. Those for {missing} are still missing."
                )
            }
        }
    }

    pub fn allowance_gone_try_tomorrow(self) -> String {
        match self {
            Lang::Es => format!(
                "{}. Mañana se puede volver a intentar.",
                self.allowance_gone()
            ),
            Lang::En => format!("{}. It can be tried again tomorrow.", self.allowance_gone()),
        }
    }

    pub fn no_subtitles_found_for(self, episodes: &str) -> String {
        match self {
            Lang::Es => {
                format!("No hay subtítulos en español para {episodes}. Puede que aparezcan más adelante.")
            }
            Lang::En => format!("There are no subtitles for {episodes}. They may appear later on."),
        }
    }

    /// "del episodio 4", "de los episodios 4 y 7" · "episode 4", "episodes 4 and 7".
    pub fn of_episodes(self, episodes: &[u32]) -> String {
        match self {
            Lang::Es => of_episodes_es(episodes),
            Lang::En => episodes_en(episodes),
        }
    }

    /// "el episodio 4", "los episodios 4 y 7" · "episode 4", "episodes 4 and 7".
    pub fn the_episodes(self, episodes: &[u32]) -> String {
        match self {
            Lang::Es => match episodes {
                [only] => format!("el episodio {only}"),
                _ => format!("los episodios {}", list_es(episodes)),
            },
            Lang::En => episodes_en(episodes),
        }
    }

    pub fn of_count_episodes(self, count: usize) -> String {
        match self {
            Lang::Es => format!("de {count} episodios"),
            Lang::En => format!("{count} episodes"),
        }
    }

    pub fn count_episodes(self, count: usize) -> String {
        match self {
            Lang::Es => format!("{count} episodios"),
            Lang::En => format!("{count} episodes"),
        }
    }

    pub fn could_not_search_subtitles(self) -> &'static str {
        match self {
            Lang::Es => "No se han podido buscar subtítulos ahora mismo",
            Lang::En => "Subtitles could not be searched for right now",
        }
    }

    pub fn film_file_missing(self) -> &'static str {
        match self {
            Lang::Es => "No se encuentra el archivo de la película.",
            Lang::En => "The film's file cannot be found.",
        }
    }

    pub fn copy_swapped(self) -> &'static str {
        match self {
            Lang::Es => "He cambiado esta copia por otra. La anterior está en la papelera.",
            Lang::En => "I swapped this copy for another. The old one is in the recycle bin.",
        }
    }

    pub fn ready_series_note(self) -> &'static str {
        match self {
            Lang::Es => "Ya está lista. Ábrela para ver los episodios.",
            Lang::En => "It is ready. Open it to see the episodes.",
        }
    }

    pub fn ready_film_note(self) -> &'static str {
        match self {
            Lang::Es => "Ya está lista para ver.",
            Lang::En => "It is ready to watch.",
        }
    }

    pub fn ready_series_notification(self) -> &'static str {
        match self {
            Lang::Es => "Ya está lista. Abre Mamá Cine para ver los episodios.",
            Lang::En => "It is ready. Open Mamá Cine to see the episodes.",
        }
    }

    pub fn ready_film_notification(self) -> &'static str {
        match self {
            Lang::Es => "Ya está lista para ver en Mamá Cine.",
            Lang::En => "It is ready to watch in Mamá Cine.",
        }
    }

    // --- the supervisor and the composition root ------------------------------------

    pub fn the_log_says_why(self) -> &'static str {
        match self {
            Lang::Es => "En Ajustes, «Abrir el registro» dice por qué.",
            Lang::En => "In Settings, \"Open the log\" says why.",
        }
    }

    pub fn downloader_would_not_start(self) -> String {
        match self {
            Lang::Es => format!(
                "No he podido arrancar el descargador. {}",
                self.the_log_says_why()
            ),
            Lang::En => format!(
                "I could not start the downloader. {}",
                self.the_log_says_why()
            ),
        }
    }

    pub fn downloader_closed_at_once(self) -> String {
        match self {
            Lang::Es => format!(
                "El descargador se ha cerrado nada más arrancar. {}",
                self.the_log_says_why()
            ),
            Lang::En => format!(
                "The downloader closed as soon as it started. {}",
                self.the_log_says_why()
            ),
        }
    }

    pub fn downloader_never_answered(self) -> String {
        match self {
            Lang::Es => format!(
                "El descargador no ha arrancado. {}",
                self.the_log_says_why()
            ),
            Lang::En => format!("The downloader did not start. {}", self.the_log_says_why()),
        }
    }

    pub fn still_starting(self) -> &'static str {
        match self {
            Lang::Es => "La aplicación no ha terminado de arrancar.",
            Lang::En => "The app has not finished starting.",
        }
    }

    pub fn where_to_save_films(self) -> &'static str {
        match self {
            Lang::Es => "Dónde guardar las películas",
            Lang::En => "Where to save the films",
        }
    }

    pub fn saved_but(self, problem: &str) -> String {
        match self {
            Lang::Es => format!("Se ha guardado, pero hay un problema: {problem}"),
            Lang::En => format!("Saved, but there is a problem: {problem}"),
        }
    }

    pub fn could_not_open_it(self, code: i32) -> String {
        match self {
            Lang::Es => format!("El ordenador no ha podido abrirlo (error {code})."),
            Lang::En => format!("The computer could not open it (error {code})."),
        }
    }

    pub fn settings_from_a_newer_app(self) -> &'static str {
        match self {
            Lang::Es => {
                "Los ajustes son de una versión más nueva de la aplicación. Hay que actualizarla."
            }
            Lang::En => "The settings are from a newer version of the app. It needs updating.",
        }
    }

    pub fn library_from_a_newer_app(self) -> &'static str {
        match self {
            Lang::Es => {
                "Los datos de las películas son de una versión más nueva de la \
                         aplicación. Hay que actualizar la aplicación para seguir."
            }
            Lang::En => {
                "The film records are from a newer version of the app. \
                         The app needs updating to continue."
            }
        }
    }

    // --- updates ---------------------------------------------------------------------

    pub fn update_available_title(self, version: &str) -> String {
        match self {
            Lang::Es => format!("Mamá Cine {version} ya está disponible"),
            Lang::En => format!("Mamá Cine {version} is available"),
        }
    }

    pub fn update_available_body(self) -> &'static str {
        match self {
            Lang::Es => "Abre Mamá Cine para instalarla.",
            Lang::En => "Open Mamá Cine to install it.",
        }
    }

    pub fn update_installed_title(self, version: &str) -> String {
        match self {
            Lang::Es => format!("Mamá Cine {version} ya está instalada"),
            Lang::En => format!("Mamá Cine {version} is installed"),
        }
    }

    pub fn update_installed_body(self) -> &'static str {
        match self {
            Lang::Es => "Se estrenará la próxima vez que abras la aplicación.",
            Lang::En => "It starts the next time you open the app.",
        }
    }

    pub fn in_the_menu_title(self) -> &'static str {
        match self {
            Lang::Es => "Mamá Cine ya está en el menú de aplicaciones",
            Lang::En => "Mamá Cine is now in the applications menu",
        }
    }

    pub fn in_the_menu_body(self) -> &'static str {
        match self {
            Lang::Es => {
                "A partir de ahora se abre desde ahí. El archivo descargado ya no hace falta."
            }
            Lang::En => "From now on it opens from there. The downloaded file is no longer needed.",
        }
    }

    // --- the settings check --------------------------------------------------------

    pub fn check_no_indexer(self) -> &'static str {
        match self {
            Lang::Es => "Buscadores: no hay ninguno configurado.",
            Lang::En => "Indexers: none configured.",
        }
    }

    pub fn check_indexer_fallback_name(self) -> &'static str {
        match self {
            Lang::Es => "Buscador",
            Lang::En => "Indexer",
        }
    }

    pub fn check_works(self, name: &str) -> String {
        match self {
            Lang::Es => format!("{name}: funciona."),
            Lang::En => format!("{name}: works."),
        }
    }

    pub fn check_news_missing_host(self) -> &'static str {
        match self {
            Lang::Es => "Servidor de descargas: falta la dirección.",
            Lang::En => "Download server: the address is missing.",
        }
    }

    pub fn check_news_works(self) -> &'static str {
        match self {
            Lang::Es => "Servidor de descargas: funciona.",
            Lang::En => "Download server: works.",
        }
    }

    pub fn check_news_refused(self) -> &'static str {
        match self {
            Lang::Es => "Servidor de descargas: ha rechazado el usuario o la contraseña.",
            Lang::En => "Download server: it rejected the user or the password.",
        }
    }

    pub fn check_news_unreachable(self) -> &'static str {
        match self {
            Lang::Es => "Servidor de descargas: no se puede conectar. Revisa la dirección, o puede que no haya internet.",
            Lang::En => "Download server: cannot connect. Check the address, or there may be no internet.",
        }
    }

    pub fn check_news_unknown(self) -> &'static str {
        match self {
            Lang::Es => "Servidor de descargas: no se ha podido comprobar ahora mismo.",
            Lang::En => "Download server: it could not be checked right now.",
        }
    }

    pub fn check_news_prefix(self, problem: &str) -> String {
        match self {
            Lang::Es => format!("Servidor de descargas: {problem}"),
            Lang::En => format!("Download server: {problem}"),
        }
    }

    pub fn check_metadata_works(self) -> &'static str {
        match self {
            Lang::Es => "Fichas de películas: funciona.",
            Lang::En => "Film pages: works.",
        }
    }

    pub fn check_metadata_prefix(self, said: &str) -> String {
        match self {
            Lang::Es => format!("Fichas de películas: {said}"),
            Lang::En => format!("Film pages: {said}"),
        }
    }

    pub fn check_subtitles_works(self) -> &'static str {
        match self {
            Lang::Es => "Subtítulos: funciona.",
            Lang::En => "Subtitles: works.",
        }
    }

    pub fn check_subtitles_prefix(self, said: &str) -> String {
        match self {
            Lang::Es => format!("Subtítulos: {said}"),
            Lang::En => format!("Subtitles: {said}"),
        }
    }

    pub fn check_subtitles_no_account(self) -> &'static str {
        match self {
            Lang::Es => {
                "Subtítulos: falta el usuario o la contraseña, así que no se pueden descargar."
            }
            Lang::En => {
                "Subtitles: the user or the password is missing, so nothing can be downloaded."
            }
        }
    }

    pub fn check_subtitles_unconfigured(self) -> &'static str {
        match self {
            Lang::Es => "Subtítulos: sin configurar. La aplicación funciona igual.",
            Lang::En => "Subtitles: not configured. The app works all the same.",
        }
    }
}

fn of_episodes_es(episodes: &[u32]) -> String {
    match episodes {
        [only] => format!("del episodio {only}"),
        _ => format!("de los episodios {}", list_es(episodes)),
    }
}

fn episodes_en(episodes: &[u32]) -> String {
    match episodes {
        [only] => format!("episode {only}"),
        _ => format!("episodes {}", join_list(episodes, " and ")),
    }
}

fn list_es(episodes: &[u32]) -> String {
    join_list(episodes, " y ")
}

fn join_list(episodes: &[u32], last_joint: &str) -> String {
    let said: Vec<String> = episodes.iter().map(u32::to_string).collect();
    match said.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{}{last_joint}{last}", rest.join(", ")),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_speaks_the_codes_it_claims() {
        assert_eq!(Lang::from_code("es"), Some(Lang::Es));
        assert_eq!(Lang::from_code("en"), Some(Lang::En));
        assert_eq!(Lang::from_code("fr"), None);
        assert_eq!(Lang::Es.code(), "es");
    }

    #[test]
    fn every_claimed_language_code_has_a_noun_in_every_language() {
        for code in [
            "es", "es-419", "en", "it", "fr", "de", "nl", "pl", "cs", "hu", "ru", "tr", "hi", "ko",
            "ja", "pt", "nordic",
        ] {
            for lang in [Lang::Es, Lang::En] {
                assert!(language_noun(lang, code).is_some(), "{lang:?} {code}");
            }
        }
    }

    #[test]
    fn a_season_is_labelled_in_the_language_of_the_screen() {
        assert_eq!(Lang::Es.season_label(1, 1), "Temporada 1");
        assert_eq!(Lang::Es.season_label(1, 3), "Temporadas 1 a 3");
        assert_eq!(Lang::En.season_label(1, 1), "Season 1");
        assert_eq!(Lang::En.season_label(1, 3), "Seasons 1 to 3");
    }

    #[test]
    fn episode_lists_read_as_prose_in_both_languages() {
        assert_eq!(Lang::Es.of_episodes(&[4]), "del episodio 4");
        assert_eq!(Lang::Es.of_episodes(&[4, 7]), "de los episodios 4 y 7");
        assert_eq!(
            Lang::Es.of_episodes(&[4, 7, 9]),
            "de los episodios 4, 7 y 9"
        );
        assert_eq!(Lang::En.of_episodes(&[4]), "episode 4");
        assert_eq!(Lang::En.of_episodes(&[4, 7]), "episodes 4 and 7");
        assert_eq!(Lang::En.of_episodes(&[4, 7, 9]), "episodes 4, 7 and 9");
    }
}
