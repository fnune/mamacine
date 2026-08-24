// The interface. Preact and htm are vendored beside this file, so there is still no build step:
// what runs in the app is what is written here, and the same file opens in a browser.
//
// Everything on screen is a function of state. Nothing reaches into the DOM by hand, because the
// bugs that hurt most here were exactly that: a list looked up by position after it had been
// replaced, a screen that never redrew itself, a name that quietly resolved to an element.

const { html, render, useState, useEffect, useRef } = htmPreact;

const invoke = (command, args) => window.__TAURI__.core.invoke(command, args);

const SUGGESTIONS = [
  'Cinema Paradiso', 'El viaje de Chihiro', 'Mi vecino Totoro', 'Qué bello es vivir',
  'Cantando bajo la lluvia', 'Vacaciones en Roma', 'Sonrisas y lágrimas', 'El mago de Oz',
  'Mary Poppins', 'La vida es bella', 'Los chicos del coro', 'Amélie', 'Chocolat', 'Billy Elliot',
  'Campeones', 'El espíritu de la colmena', 'Belle Époque', 'Marcelino pan y vino',
  'Bienvenido, Mister Marshall', 'Volver', 'La tortuga roja', 'Ponyo en el acantilado',
  'El castillo ambulante', 'Nicky, la aprendiza de bruja', 'Coco', 'Up', 'Buscando a Nemo',
  'Ratatouille', 'Del revés', 'Downton Abbey',
];

const pick = (list) => list[Math.floor(Math.random() * list.length)];

// Where the way out leads, screen by screen. The button for it lives in the masthead, in one
// place for every screen that has one, rather than spending a row of the ficha on itself.
const BACK_TO = { detail: 'search', owned: 'library', episode: 'owned' };

// She reads names, not ISO codes. An untagged track is "sin etiquetar", which is not a verdict.
const LANGUAGE_NAMES = {
  spa: 'español', es: 'español', esp: 'español', cast: 'español',
  eng: 'inglés', en: 'inglés', ger: 'alemán', deu: 'alemán', de: 'alemán',
  fre: 'francés', fra: 'francés', fr: 'francés', ita: 'italiano', it: 'italiano',
  por: 'portugués', pt: 'portugués', jpn: 'japonés', ja: 'japonés', kor: 'coreano',
  chi: 'chino', zho: 'chino', rus: 'ruso', ara: 'árabe', hin: 'hindi', tur: 'turco',
  dut: 'neerlandés', nld: 'neerlandés', swe: 'sueco', dan: 'danés', nor: 'noruego',
  fin: 'finés', pol: 'polaco', ces: 'checo', hun: 'húngaro', ell: 'griego',
  cat: 'catalán', eus: 'euskera', glg: 'gallego', und: 'sin etiquetar',
};

const named = (codes) => [...new Set(codes.map((code) => LANGUAGE_NAMES[code] || code))];

// The quality and the language head a chip of their own, so they arrive capitalised. Spanish
// does not capitalise mid-sentence.
const midSentence = (text) => (text ? text[0].toLowerCase() + text.slice(1) : text);
const SPANISH = ['spa', 'es', 'esp', 'cast', 'spanish'];
const inSpanish = (code) => SPANISH.includes(code);

const STATUS_WORDS = {
  starting: 'Empezando la descarga…',
  downloading: 'Descargando',
  verifying: 'Comprobando que está completa…',
  repairing: 'Recuperando lo que falta…',
  unpacking: 'Casi lista…',
  moving: 'Guardando…',
  finishing: 'Últimos detalles…',
  // nzbget pauses itself when the disk runs out; "últimos detalles" over a stalled bar was a lie
  paused: 'En pausa',
  // retrying while the server is down is not searching, it is waiting, and saying so is what
  // makes the banner above it make sense
  waiting: 'Esperando a que vuelva el servidor…',
  // finding a broken copy and starting another is the ordinary course of this, not an incident:
  // it is one line of the story below, and the headline stays on the thing she is waiting for
  retrying: 'Buscando otra copia…',
  done: 'Lista para ver',
  failed: 'No he podido conseguirla',
};

const clock = new Intl.DateTimeFormat('es-ES', { hour: '2-digit', minute: '2-digit' });
const calendar = new Intl.DateTimeFormat('es-ES', { day: 'numeric', month: 'short' });

// A story can span days; "10:31" about last Tuesday reads as this morning.
function when(at) {
  const then = new Date(at * 1000);
  const today = new Date().toDateString() === then.toDateString();
  return today ? clock.format(then) : `${calendar.format(then)} ${clock.format(then)}`;
}

// Everything the app has done to this film, in order, in her words. The latest line sits under
// the headline; the rest waits behind a fold, because a growing timestamped list reads as a log.
function Story({ story, except }) {
  if (!story?.length) return null;
  const latest = story[story.length - 1];
  // the headline's detail often IS the story's final line; saying it twice reads as a stutter
  const repeat = except && latest.said === except;
  return html`<${Fragment}>
    ${!repeat && html`
      <p class="latest" id="story-latest" title=${latest.why}>${latest.said}</p>`}
    ${story.length > 1 && html`
      <details class="story-fold">
        <summary>Qué ha pasado</summary>
        <ol class="story">
          ${story.map((note, position) => html`
            <li key=${position} class=${position === story.length - 1 ? 'now' : ''} title=${note.why}>
              <span class="when">${when(note.at)}</span>
              <span class="said">${note.said}</span>
            </li>`)}
        </ol>
      </details>`}
  <//>`;
}

// What she needs to know before pressing play: whether she will understand it.
function spokenIn(film) {
  const tracks = film.languages || {};
  const audio = tracks.audio_languages || [];
  const subtitles = tracks.subtitle_languages || [];
  if (!audio.length && !subtitles.length) return '';

  const parts = [];
  if (audio.length) {
    parts.push(audio.every((code) => code === 'und')
      ? 'audio sin etiquetar'
      : `audio en ${named(audio.filter((code) => code !== 'und')).join(', ')}`);
  }
  if (subtitles.length) parts.push(`subtítulos en ${named(subtitles).join(', ')}`);
  if (![...audio, ...subtitles].some((code) => code !== 'und')) parts.push('idioma desconocido');
  return parts.join(' · ');
}

// The facts, hers to judge: what it occupies, what is free, how big the disk is, how long it
// will take. A verdict like "hay sitio de sobra" hid every one of those numbers.
function timeWords(minutes) {
  if (!minutes) return '';
  return `tardará ${minutes > 90
    ? `unas ${Math.round(minutes / 60)} horas` : `unos ${minutes} minutos`}`;
}

// Under 40 GB, a film plus its unpacking scratch space stops fitting comfortably.
const lowOnSpace = (progress) => progress.free_bytes > 0
  && progress.free_bytes < 40 * 1024 ** 3;

// A disk drawn rather than narrated: "445 GB libres de 1861 GB" asks her to divide, a bar just
// shows it. The exact numbers stay beside it and the percentage rides on the tooltip, so the
// picture never hides what it summarises.
function Meter({ free, total, slice, low }) {
  if (!total) return null;
  const used = Math.max(0, total - free);
  const usedShare = Math.min(100, (used / total) * 100);
  // the download's own share of the disk; a sliver is drawn as a sliver, but never invisibly
  const sliceShare = slice
    ? Math.min(100 - usedShare, Math.max(0.8, (slice / total) * 100))
    : 0;
  const percent = Math.round((free / total) * 100);
  return html`<span class="meter ${low ? 'low' : ''}"
        title=${`${percent} % libre`} role="img" aria-label=${`${percent} % libre`}>
    <i class="used" style=${`width: ${usedShare}%`}></i>
    ${sliceShare > 0 && html`<i class="slice" style=${`width: ${sliceShare}%`}></i>`}
  </span>`;
}

// The download she is waiting for, present on every screen, gone when there is none. It names
// the film the download screen headlines; the rest are a count, because an averaged bar answers
// no real question. Tapping it opens that screen.
function Pill({ film, others, on, onOpen }) {
  const settled = film.status === 'done';
  const failed = film.status === 'failed';
  const percent = settled ? 100 : Math.round(film.percent || 0);
  return html`
    <button class="pill ${settled ? 'done' : failed ? 'failed' : ''} ${on ? 'on' : ''}"
            id="pill" data-screen="film" title=${film.title} onClick=${onOpen}>
      <${Poster} url=${film.cover_url} />
      <span class="pill-name">${film.title}</span>
      ${failed
        ? html`<span class="word">No he podido</span>`
        : html`<${Fragment}>
            <span class="mini"><i style=${`width: ${percent}%`}></i></span>
            <span class="word">${settled ? 'Lista' : `${percent} %`}</span>
          <//>`}
      ${others > 0 && html`<span class="badge" id="pill-count">+${others}</span>`}
    </button>`;
}

// --- posters -----------------------------------------------------------------

// Fetched through the app, because the window is not allowed to reach the internet itself. Kept,
// so the same poster is never asked for twice.
const posters = new Map();

// A film with no cover still needs a card that looks finished, so the frame draws a filmstrip
// rather than a hole. Drawn only once the cover is known to be missing: while it is still coming,
// the plain frame is the placeholder.
function filmMark() {
  return html`
    <svg class="mark" viewBox="0 0 48 48" aria-hidden="true">
      <rect x="6" y="11" width="36" height="26" rx="3" />
      <path d="M15 11v26M33 11v26M6 19h9M6 29h9M33 19h9M33 29h9" />
    </svg>`;
}

function Poster({ url, alt }) {
  const [data, setData] = useState(url ? posters.get(url) || null : null);
  const [missing, setMissing] = useState(!url);

  useEffect(() => {
    let current = true;
    if (!url) {
      setData(null);
      setMissing(true);
      return undefined;
    }
    setMissing(false);
    if (posters.has(url)) {
      setData(posters.get(url) || null);
      setMissing(!posters.get(url));
      return undefined;
    }
    setData(null);
    invoke('cover', { url })
      .then((image) => {
        posters.set(url, image);
        if (!current) return;
        setData(image || null);
        setMissing(!image);
      })
      .catch(() => { if (current) setMissing(true); });
    return () => { current = false; };
  }, [url]);

  if (data) return html`<div class="poster"><img src=${data} alt=${alt || ''} /></div>`;
  return html`<div class=${`poster blank${missing ? ' none' : ''}`}>
    ${missing && filmMark()}
  </div>`;
}

// --- pieces ------------------------------------------------------------------

function Card({ title, lines, cover, onOpen }) {
  return html`
    <button class="film" onClick=${onOpen} title=${title}>
      <${Poster} url=${cover} />
      <span class="caption">
        <span class="name">${title}</span>
        ${lines.filter(Boolean).map((line) => html`<span class="meta">${line}</span>`)}
      </span>
    </button>`;
}

function Waiting() {
  return html`<${Fragment}>
    ${Array.from({ length: 6 }, (_, index) => html`
      <div class="film waiting" key=${index}>
        <div class="poster"></div>
        <span class="caption"><span class="line"></span><span class="line short"></span></span>
      </div>`)}
  <//>`;
}

const Fragment = ({ children }) => html`${children}`;

function Notice({ notice }) {
  if (!notice) return null;
  return html`<div id="notice"><div class="note ${notice.bad ? 'bad' : ''}">${notice.text}</div></div>`;
}

// The downloader stopped answering, or never started: said calmly, on every screen, because a
// frozen screen that says nothing was one of the ways this app used to lie.
function Problem({ problem, actions }) {
  if (!problem) return null;
  return html`<div id="problem" class="problem">
    ${problem}
    <button type="button" class="link" id="problem-log" onClick=${actions.openLogFile}>
      Abrir el registro
    </button>
  </div>`;
}

// --- the screens -------------------------------------------------------------

function Search({ state, actions }) {
  // one list, best answer first: the thing she named must never sit below what merely mentions it
  const results = [
    ...state.films.map((item) => ({ item, series: false })),
    ...state.seasons.map((item) => ({ item, series: true })),
  ].sort((a, b) => (b.item.relevance ?? 0) - (a.item.relevance ?? 0));
  return html`
    <section class="screen" id="screen-search">
      <h1 id="search-title">¿Qué te apetece ver hoy?</h1>
      <p class="lead" id="search-lead">Escribe el nombre de una película o de una serie.</p>

      <form class="search-row" id="search" onSubmit=${actions.submitSearch}>
        <div class="search-box">
          <input type="search" id="query" autocomplete="off" autofocus
                 placeholder=${state.placeholder}
                 value=${state.query}
                 onInput=${(event) => actions.typed(event.target.value)} />
          ${state.suggestions.length > 0 && html`
            <div class="suggestions" id="suggestions">
              ${state.suggestions.map((title, position) => html`
                <button type="button" class="suggestion" key=${title.id}
                        onClick=${() => actions.pickSuggestion(position)}>
                  <${Poster} url=${title.poster_url} />
                  <span class="what">
                    <span class="name">${title.title}${title.original
                      && ` (${title.original})`}</span>
                    <span class="meta">
                      ${[title.year, title.series ? 'Serie' : ''].filter(Boolean).join(' · ')}
                    </span>
                  </span>
                </button>`)}
            </div>`}
        </div>
        <button class="primary" type="submit" disabled=${state.searching}>Buscar</button>
      </form>

      <${Notice} notice=${state.notice} />

      <div class="films" id="results">
        ${state.searching
          ? html`<${Waiting} />`
          : results.length
            ? results.map(({ item, series }) => html`
                <${Card} key=${`${series ? 'season' : 'film'}-${item.index}`}
                  title=${series ? item.show : item.title}
                  cover=${item.cover_url}
                  lines=${[series ? item.label : item.year]}
                  onOpen=${() => actions.openDetail(item, series)} />`)
            : state.searched && state.suggestions.length === 0 && html`<div class="empty">
                ${state.searchedExact
                  ? 'Existe, pero ahora mismo no la encuentro en los sitios donde busco. Puede que aparezca más adelante.'
                  : 'No hay nada con ese nombre. Escríbelo otra vez y elige uno de los títulos que aparecen.'}
              </div>`}
      </div>
    </section>`;
}

// One film, one page, whichever way she arrived at it. It knows nothing about where its facts
// came from: the two routes below fetch them and hand them over. Search and Mi colección used to
// be separate pages that had drifted apart, and the one reached from a search could not tell her
// a thing about the copy she already had.
function FilmPage({
  screenId, titleId, synopsisId, bandId,
  cover, title, factline, synopsis, bad, facts, actions, copies, children,
}) {
  return html`
    <section class="screen split" id=${screenId}>
      <div class="scroll">
        <div class="now">
          <${Poster} url=${cover} alt=${title} />
          <div class="detail">
            <h1 id=${titleId}>${title}</h1>
            <p class="factline">${factline}</p>
            ${synopsis && html`<p class="synopsis" id=${synopsisId}>${synopsis}</p>`}
            ${children}
          </div>
        </div>
      </div>
      <div class="band decision ${bad ? 'bad' : ''}" id=${bandId}>
        <div class="facts">${facts}</div>
        <div class="actions">
          ${actions}
          ${copies?.more && html`
            <button class="quiet" id="show-copies" onClick=${copies.onToggle}>
              ${copies.more}
            </button>`}
        </div>
        ${copies && html`<${Copies} ...${copies} />`}
      </div>
    </section>`;
}

// The copies, each with the button that starts it. The row itself was the button, so nothing on
// the screen said that touching a line of text would spend an hour of her connection — and when
// it did start, the list sat there unchanged while the masthead already knew.
function Copies({ versions, open, verb, onPick, confirming, onConfirm, onDismiss,
                  loading, problem }) {
  if (!open) return null;
  return html`
    <div class="versions" id="copies">
      ${loading && html`<p class="factline" id="copies-waiting">Buscando otras copias…</p>`}
      ${problem && html`<${Notice} notice=${{ text: problem, bad: true }} />`}
      ${(versions || []).map((version) => html`
        <div class="version" key=${version.index} title=${version.name}>
          <span class="what">${version.quality} · ${version.size}</span>
          <span class="who">${version.language} · ${version.grabs} descargas</span>
          ${version.chosen && html`<span class="mark">la elegida</span>`}
          ${confirming === version.index
            ? html`<span class="confirm">
                <span class="word">¿Cambiar la copia?</span>
                <button class="quiet bad" onClick=${() => onConfirm(version)}>Sí, cambiar</button>
                <button class="quiet" onClick=${onDismiss}>No</button>
              </span>`
            : html`<button class="primary pick" onClick=${() => onPick(version)}>${verb}</button>`}
        </div>`)}
    </div>`;
}

// Search, then a film. The facts come from the live result list, so this route is the one that
// can offer copies without asking anybody for a handle first.
function Detail({ state, actions }) {
  const item = state.detail;
  if (!item) return null;
  const series = state.detailSeries;
  const chosen = (state.versions || []).find((version) => version.chosen);
  const have = state.have;
  const thing = series ? 'esta temporada' : 'esta película';
  const disk = state.progress;
  const episodes = series ? (state.seasonEpisodes || []) : [];
  // a pack of several seasons says how much television it is; fifty names whose numbering starts
  // over five times is not a list she can read
  const oneSeason = new Set(episodes.map((episode) => episode.season)).size === 1;
  const named = oneSeason ? episodes.filter((episode) => episode.title) : [];
  const coming = comingDown(state);
  const failed = coming?.status === 'failed';
  const shown = coming?.status === 'retrying' && state.problem ? 'waiting' : coming?.status;

  const facts = have
    ? html`<p class="chosen" id="already">Ya tienes ${thing} en este ordenador.</p>`
    : coming
    ? html`<${Progress} coming=${coming} shown=${shown} failed=${failed} id="detail-status" />`
    // started somewhere else and not yet in the live list: still not something to offer again
    : state.downloadingId
    ? html`<p class="chosen" id="already-downloading">Ya se está descargando.</p>`
    : html`<${Fragment}>
        ${chosen && html`
          <div class="fact-row">
            <p class="room" id="room">
              ${[`Ocupa ${chosen.size}`, timeWords(chosen.minutes)].filter(Boolean).join(' · ')}
            </p>
            ${disk.total_bytes > 0 && html`
              <p class="room-disk" id="room-disk">
                <${Meter} free=${disk.free_bytes} total=${disk.total_bytes}
                          slice=${chosen.size_bytes} low=${chosen.room !== 'fits'} />
                <span>quedan ${disk.free_space} libres</span>
              </p>`}
          </div>`}
        ${chosen && chosen.room !== 'fits' && html`
          <p class="room bad" id="room-warning">
            ${chosen.room === 'no' ? 'No hay sitio suficiente' : 'Puede que no quepa'}:
            mientras se descarga y se prepara, necesita unos ${chosen.needs}.
          </p>`}
        ${chosen && html`
          <p class="chosen" id="what-comes">
            Se descargará en ${`${midSentence(chosen.quality)}, ${midSentence(chosen.language)}`}
          </p>`}
      <//>`;

  const buttons = have
    ? html`<${Fragment}>
        ${!series && html`
          <button class="primary" onClick=${() => invoke('play', { id: have }).catch(actions.tell('notice'))}>
            Ver la película
          </button>`}
        ${series && html`
          <button class="primary" onClick=${() => actions.openOwned(have)}>
            Ver los episodios
          </button>`}
      <//>`
    : coming
    ? html`<${DownloadActions} coming=${coming} failed=${failed} actions=${actions} />`
    : state.downloadingId
    ? html`
      <button class="primary" id="watch-download"
              onClick=${() => actions.watchDownload(state.downloadingId)}>
        Ver cómo va
      </button>`
    : html`
      <button class="primary" id="download" onClick=${() => actions.download()}>
        Descargar
      </button>`;

  return html`
    <${FilmPage}
      screenId="screen-detail" titleId="detail-title" synopsisId="synopsis" bandId="detail-band"
      cover=${item.cover_url} title=${series ? `${item.show} · ${item.label}` : item.title}
      factline=${series ? 'Temporada completa'
                        : [item.year, item.about].filter(Boolean).join(' · ')}
      synopsis=${state.synopsis} bad=${failed} facts=${facts} actions=${buttons}
      copies=${copiesProps(state, actions, state.versions, have)}>
      ${series && !have && html`<p class="factline">
        ${episodes.length > 0 ? `Son ${episodes.length} episodios.` : 'Son varios episodios.'}
        Cuando termine la descarga, podrás verlos aquí, uno a uno.</p>`}
      ${named.length > 0 && html`
        <ol class="episode-names" id="episode-names">
          ${named.map((episode) => html`
            <li key=${`${episode.season}-${episode.number}`}>
              <span class="which">${episode.number}</span> ${episode.title}
            </li>`)}
        </ol>`}
      ${item.imdb && html`
        <button class="quiet" onClick=${() => invoke(series ? 'open_imdb_season' : 'open_imdb', { index: item.index }).catch(actions.tell('notice'))}>
          ${series ? 'Ver los episodios en IMDb' : 'Ver la ficha en IMDb'}
        </button>`}
      <${Notice} notice=${state.notice} />
    <//>`;
}

// The copy coming down, when it is the one this page is about. It is shown the moment she picks
// it rather than when the downloader answers: the masthead changed instantly and the copies list
// sat there for seconds looking like nothing had happened.
function comingDown(state) {
  if (!state.watching) return null;
  if (state.starting) return state.watching;
  return state.watching.id === state.downloadingId ? state.watching : null;
}

function Progress({ coming, shown, failed, id }) {
  const beneath = [coming.detail || '', coming.beneath, coming.speed].filter(Boolean).join(' · ');
  return html`
    <${Fragment}>
      <p class="status ${failed ? 'bad failed' : 'working'}" id=${id}>
        ${STATUS_WORDS[shown] || 'Trabajando…'}
        ${coming.status === 'downloading' && ` ${Math.round(coming.percent || 0)} %`}
      </p>
      <div class="bar"><i style=${`width: ${coming.percent || 0}%`}></i></div>
      <p class="beneath">${beneath}</p>
    <//>`;
}

function DownloadActions({ coming, failed, actions }) {
  return html`
    <${Fragment}>
      ${failed && coming.untried > 0 && html`
        <button class="primary" id="try-more" onClick=${() => actions.tryMore(coming.id)}>
          Probar más copias (quedan ${coming.untried})
        </button>`}
      ${!failed && coming.id && html`
        <button class="quiet" onClick=${() => actions.cancel(coming.id)}>
          Cancelar la descarga
        </button>`}
    <//>`;
}

// What the copies fold needs, from wherever the copies came from. Swapping a copy she already
// has is the one that asks first: it ends with a film of hers in the papelera.
function copiesProps(state, actions, versions, owned) {
  const list = versions || [];
  if (!list.length) return null;
  return {
    versions: state.showVersions ? list : [],
    open: state.showVersions,
    onToggle: actions.toggleVersions,
    verb: owned ? 'Cambiar a esta copia' : 'Descargar esta copia',
    // swapping ends with a film of hers in the papelera, so that one is asked first
    onPick: (version) => (owned
      ? actions.confirmSwap(version.index)
      : actions.download(version.index)),
    confirming: state.confirmSwap,
    onConfirm: (version) => actions.download(version.index, owned),
    onDismiss: () => actions.confirmSwap(null),
    more: state.showVersions ? 'Ocultar las copias' : `Otras copias (${list.length})`,
  };
}

function Now({ state, actions }) {
  const film = state.watching;
  if (!film) {
    return html`
      <section class="screen" id="screen-film">
        <div class="empty now-empty" id="now-empty">
          <p>Ahora mismo no se está descargando nada.</p>
          <button class="primary" onClick=${() => actions.show('search')}>Buscar algo</button>
        </div>
      </section>`;
  }

  const settled = film.status === 'done';
  const failed = film.status === 'failed';
  const paused = film.status === 'paused';
  // "buscando otra versión" while the server is down would be a small lie under a banner
  const shown = film.status === 'retrying' && state.problem ? 'waiting' : film.status;
  const others = state.progress.active.filter((active) => active.id !== film.id);
  const beneath = [film.detail || '', film.beneath, film.speed].filter(Boolean).join(' · ');
  // The film fills the screen; the machinery is a band pinned below, like a player. The same
  // spot answers "¿ya está?" from start to finish, and turns green when the answer is yes.
  return html`
    <section class="screen split" id="screen-film">
      <div class="scroll">
        ${others.length > 0 && html`
          <div class="also" id="also-downloading">
            <span>${others.length === 1
              ? 'También se está descargando:'
              : 'También se están descargando:'}</span>
            ${others.map((other) => html`
              <button class="quiet" key=${other.id} onClick=${() => actions.watch(other)}>
                ${other.title}
              </button>`)}
          </div>`}
        <div class="now">
          <${Poster} url=${film.cover_url} alt=${film.title} />
          <div class="detail">
            <h1 id="now-title">${film.title}</h1>
            <p class="factline">${film.year || ''}</p>
            <${Story} story=${film.story} except=${film.detail} />
          </div>
        </div>
      </div>
      <div class="band ${settled ? 'ok' : failed ? 'bad' : ''}" id="now-band">
        <p class="status ${settled ? 'done' : failed ? 'bad failed' : 'working'}" id="now-status">
          ${STATUS_WORDS[shown] || 'Trabajando…'}
          ${film.status === 'downloading' && ` ${Math.round(film.percent || 0)} %`}
        </p>
        <div class="bar"><i style=${`width: ${settled ? 100 : film.percent || 0}%`}></i></div>
        <p class="beneath">${paused && state.progress.free_bytes > 0
          && state.progress.free_bytes < 20 * 1024 ** 3
          ? `El disco está casi lleno: quedan ${state.progress.free_space} libres. `
            + 'Borra alguna película que ya hayas visto.'
          : beneath}</p>

        <div class="actions">
          ${settled && html`
            <${Fragment}>
              ${!film.series && html`
                <button class="primary" onClick=${() => invoke('play', { id: film.id }).catch(actions.tell('notice'))}>
                  Ver la película
                </button>`}
              ${film.series && html`
                <button class="primary" onClick=${() => actions.openOwned(film.id)}>
                  Ver los episodios
                </button>`}
              <button class="quiet" onClick=${() => actions.show('search')}>Buscar otra</button>
            <//>`}
          ${failed && html`
            <${Fragment}>
              ${film.untried > 0 && html`
                <button class="primary" id="try-more" onClick=${() => actions.tryMore(film.id)}>
                  Probar más copias (quedan ${film.untried})
                </button>`}
              <button class=${film.untried > 0 ? 'quiet' : 'primary'}
                      onClick=${() => actions.show('search')}>Buscar otra</button>
            <//>`}
          ${!settled && !failed && html`
            <button class="quiet" onClick=${() => actions.cancel(film.id)}>
              Cancelar la descarga
            </button>`}
        </div>
        <${Notice} notice=${state.notice} />
      </div>
    </section>`;
}

function Shelf({ state, actions }) {
  const films = state.progress.shelf || [];
  return html`
    <section class="screen" id="screen-library">
      <h1>Mis películas y series</h1>
      <p class="lead">Todo lo que ya está en este ordenador.</p>
      ${state.progress.total_bytes > 0 && html`
        <p class="room-disk" id="shelf-disk">
          <${Meter} free=${state.progress.free_bytes} total=${state.progress.total_bytes}
                    low=${lowOnSpace(state.progress)} />
          <span>quedan ${state.progress.free_space} libres</span>
        </p>`}
      <${Notice} notice=${state.shelfNotice} />
      <div class="films" id="shelf">
        ${films.length
          ? films.map((film) => html`
              <${Card} key=${film.id}
                title=${film.title}
                cover=${film.cover_url}
                lines=${[film.series ? 'Serie' : film.year]}
                onOpen=${() => actions.openOwned(film.id)} />`)
          : html`<div class="empty">Todavía no hay nada aquí.</div>`}
      </div>
    </section>`;
}

// What the app knows about her subtitles, as a state rather than as an errand. A button offering
// to look for what is already there was work she had no reason to do, and no way to judge.
function subtitleState(film) {
  if (film.subtitle_note) return film.subtitle_note;
  const found = (film.languages?.subtitle_languages || []).some(inSpanish);
  return found ? 'Subtítulos en español' : 'Sin subtítulos en español';
}

function hasSpanishSubtitles(film) {
  if (film.subtitle_note) return !/^(no hay|sin|faltan)/i.test(film.subtitle_note);
  return (film.languages?.subtitle_languages || []).some(inSpanish);
}

// The line that says how she will understand it, and the one small way to try again. The button
// carries a word as well as its mark: a bare glyph is a guess about what it does.
function Subtitles({ said, ok, working, onFind }) {
  return html`
    <p class="subtitles ${ok ? 'ok' : 'bad'}" id="subtitle-state">
      <span class="what">${said}</span>
      <button class="find" id="find-subtitles" disabled=${Boolean(working)}
              title="Buscar los subtítulos otra vez" onClick=${onFind}>
        <span aria-hidden="true">⟳</span> ${working ? 'Buscando…' : 'Buscar otra vez'}
      </button>
    </p>`;
}

// Mi colección, then a film. The facts are what is on the disk, so this route opens with no
// question asked of anything outside this computer; the copies are fetched only if she asks for
// them, because a film of hers has to open when the indexer is down.
function Owned({ state, actions }) {
  const film = (state.progress.shelf || []).find((item) => item.id === state.ownedId);
  if (!film) {
    return html`
      <section class="screen" id="screen-owned">
        <div class="empty" id="owned-gone">Esto ya no está en este ordenador.</div>
      </section>`;
  }
  const episodes = state.ownedEpisodes || [];
  const first = episodes[0];
  // the copy she swapped to, coming down under the film it is replacing: she stays where she
  // was, and the one she has keeps playing until the new one is really here
  const coming = comingDown(state);
  const failed = coming?.status === 'failed';
  const shown = coming?.status === 'retrying' && state.problem ? 'waiting' : coming?.status;

  const facts = coming
    ? html`<${Progress} coming=${coming} shown=${shown} failed=${failed} id="owned-status" />`
    : html`
      <${Subtitles} said=${subtitleState(film)} ok=${hasSpanishSubtitles(film)}
                    working=${state.findingSubtitles}
                    onFind=${() => actions.refetchSubtitles(film)} />`;

  const buttons = html`
    <${Fragment}>
      ${!film.series && html`
        <button class="primary play" id="play" onClick=${() => actions.play(film)}>
          <span aria-hidden="true">▶</span> Ver la película
        </button>`}
      ${film.series && first && html`
        <button class="primary play" id="play-first" onClick=${() => actions.openEpisode(0)}>
          <span aria-hidden="true">▶</span> ${first.number
            ? `Empezar por el episodio ${first.number}`
            : 'Empezar por el primero'}
        </button>`}
      <button class="quiet" onClick=${() => actions.reveal(film)}>Abrir la carpeta</button>
      ${state.confirmRemove === film.id
        ? html`<span class="confirm">
            <span class="word">¿Seguro?</span>
            <button class="quiet bad" onClick=${() => actions.remove(film)}>Sí, borrar</button>
            <button class="quiet" onClick=${() => actions.confirmRemove(null)}>No</button>
          </span>`
        : html`<button class="quiet" id="remove"
                       onClick=${() => actions.confirmRemove(film.id)}>Borrar</button>`}
    <//>`;

  return html`
    <${FilmPage}
      screenId="screen-owned" titleId="owned-title" synopsisId="owned-synopsis"
      bandId="owned-band"
      cover=${film.cover_url} title=${film.title}
      factline=${[film.series ? 'Temporada completa' : film.year, spokenIn(film)]
        .filter(Boolean).join(' · ')}
      synopsis=${state.ownedSynopsis} bad=${failed} facts=${facts} actions=${buttons}
      copies=${ownedCopies(state, actions)}>
      ${film.series && html`
        <div class="episodes" id="episodes">
          ${episodes.map((episode, position) => html`
            <button class="episode" key=${position}
                    onClick=${() => actions.openEpisode(position)}>
              <span class="which">${episode.number ?? position + 1}</span>
              <span class="what">
                <span class="name">${episode.title || episode.label}</span>
                ${episode.title && html`<span class="meta">${episode.label}</span>`}
              </span>
              ${!episode.subtitles && html`<span class="without">sin subtítulos</span>`}
              <span class="go" aria-hidden="true">→</span>
            </button>`)}
          ${state.ownedEpisodes === null && html`
            <p class="factline" id="episodes-waiting">Buscando los episodios…</p>`}
          ${episodes.length === 0 && state.ownedEpisodes !== null && html`
            <p class="factline">En esta carpeta no queda ningún episodio.</p>`}
        </div>`}
      <${Notice} notice=${state.shelfNotice} />
    <//>`;
}

// Her own film's copies: never fetched until she asks, and the fold's own button is what asks.
function ownedCopies(state, actions) {
  const copies = state.copies;
  return {
    versions: copies?.versions || [],
    open: state.showCopies,
    onToggle: actions.toggleCopies,
    verb: 'Cambiar a esta copia',
    onPick: (version) => actions.confirmSwap(version.index),
    confirming: state.confirmSwap,
    onConfirm: (version) => actions.swapCopy(version.index),
    onDismiss: () => actions.confirmSwap(null),
    loading: state.loadingCopies,
    problem: state.copiesProblem,
    more: state.showCopies ? 'Ocultar las copias' : 'Ver otras copias',
  };
}


// One episode, with the same page a film gets: what it is called, what happens in it, and the
// button that plays it. A row in a list that plays something the moment it is touched never said
// what it was about to do.
function EpisodeScreen({ state, actions }) {
  const film = (state.progress.shelf || []).find((item) => item.id === state.ownedId);
  const episode = (state.ownedEpisodes || [])[state.episodeAt];
  if (!film || !episode) {
    return html`
      <section class="screen" id="screen-episode">
        <div class="empty" id="episode-gone">Ese episodio ya no está en la carpeta.</div>
      </section>`;
  }
  return html`
    <section class="screen split" id="screen-episode">
      <div class="scroll">
        <div class="now">
          <${Poster} url=${film.cover_url} alt=${film.title} />
          <div class="detail">
            <p class="eyebrow" id="episode-of">${film.title}</p>
            <h1 id="episode-title">${episode.title || episode.label}</h1>
            ${episode.title && html`<p class="factline">${episode.label}</p>`}
            ${episode.overview && html`
              <p class="synopsis" id="episode-overview">${episode.overview}</p>`}
            <${Notice} notice=${state.shelfNotice} />
          </div>
        </div>
      </div>
      <div class="band decision" id="episode-band">
        <div class="facts">
          <${Subtitles}
            said=${episode.subtitles
              ? 'Subtítulos en español'
              : 'Este episodio no tiene subtítulos en español'}
            ok=${episode.subtitles}
            working=${state.findingSubtitles}
            onFind=${() => actions.refetchSubtitles(film)} />
        </div>
        <div class="actions">
          <button class="primary play" id="play-episode"
                  onClick=${() => actions.playEpisode(film, state.episodeAt)}>
            <span aria-hidden="true">▶</span> Ver este episodio
          </button>
        </div>
      </div>
    </section>`;
}

// Passwords the backend answers about rather than with: it sends news_password_set, and the field
// says "sin cambios" instead of showing anything. Dropped from whatever the backend sends, so a
// backend that ever answered with one would still not put it back on a screen. Dropped on the way
// in and not on every redraw: blanking the box as it was drawn threw away each letter as she typed
// it, and what got saved was the last one.
const WITHHELD = ['news_password', 'subtitles_password'];

const withoutPasswords = (settings) => {
  const kept = { ...settings };
  for (const name of WITHHELD) delete kept[name];
  return kept;
};

function Settings({ state, actions }) {
  const settings = state.settings;
  if (!settings) return html`<section class="screen" id="screen-settings"></section>`;

  const field = (name, label, extra = {}) => html`
    <label>${label}
      <input name=${name} value=${settings[name] ?? ''}
             autocomplete="off" ...${extra}
             onInput=${(event) => actions.setSetting(name, event.target.value)} /></label>`;

  return html`
    <section class="screen" id="screen-settings">
      <div class="inner">
      <h1>Ajustes</h1>
      <p class="lead">Esto se rellena una vez. Después no hay que volver aquí.</p>

      <h2>Dónde se guardan las películas</h2>
      <label>Carpeta
        <span class="folder">
          <input readonly placeholder="Sin elegir" value=${settings.destination ?? ''} />
          <button type="button" onClick=${actions.chooseFolder}>Elegir…</button>
        </span>
      </label>

      <h2>En qué idioma</h2>
      <div class="chips" id="language">
        ${[['any', 'Me da igual'], ['es', 'En español'], ['original', 'Versión original']]
          .map(([code, label]) => html`
            <button type="button" class="chip ${settings.language === code ? 'on' : ''}"
                    data-lang=${code}
                    onClick=${() => actions.setSetting('language', code)}>${label}</button>`)}
      </div>

      <h2>Al encender y al cerrar</h2>
      <label class="switch">
        <input type="checkbox" name="autostart" checked=${settings.autostart === true}
               onChange=${(event) => actions.setSetting('autostart', event.target.checked)} />
        Abrir Mamá Cine al encender el ordenador
      </label>
      <label class="switch">
        <input type="checkbox" name="keep_running" checked=${settings.keep_running !== false}
               onChange=${(event) => actions.setSetting('keep_running', event.target.checked)} />
        Al cerrar la ventana, seguir con las descargas
      </label>
      <p class="hint">La aplicación se queda como un icono pequeño junto al reloj y avisa
        cuando una película está lista.</p>

      <details class="technical" id="technical">
        <summary>Ajustes técnicos</summary>

        <h2>Dónde buscar</h2>
        <p class="hint">Puedes añadir varios: cuantos más, más películas encontrarás.</p>
        <div id="indexers">
          ${(settings.indexers || []).map((indexer, position) => html`
            <div class="indexer" key=${position}>
              <label>Nombre
                <input value=${indexer.name} placeholder="NZBGeek"
                       onInput=${(event) => actions.setIndexer(position, 'name', event.target.value)} /></label>
              <label>Dirección
                <input value=${indexer.url} placeholder="https://api.nzbgeek.info"
                       onInput=${(event) => actions.setIndexer(position, 'url', event.target.value)} /></label>
              <label>Clave
                <input type="password" value=${indexer.key}
                       onInput=${(event) => actions.setIndexer(position, 'key', event.target.value)} /></label>
              <div class="indexer-actions">
                <label class="switch">
                  <input type="checkbox" checked=${indexer.enabled}
                         onChange=${(event) => actions.setIndexer(position, 'enabled', event.target.checked)} />
                  En uso
                </label>
                <button type="button" class="link" onClick=${() => actions.removeIndexer(position)}>
                  Quitar
                </button>
              </div>
            </div>`)}
        </div>
        <button type="button" class="quiet" id="add-indexer" onClick=${actions.addIndexer}>
          Añadir otro buscador
        </button>

        <h2>Servidor de descargas</h2>
        ${field('news_host', 'Servidor')}
        <div class="pair">
          ${field('news_port', 'Puerto', { type: 'number' })}
          ${field('news_connections', 'Conexiones', { type: 'number' })}
        </div>
        ${field('news_user', 'Usuario')}
        ${field('news_password', 'Contraseña', { type: 'password', placeholder: 'sin cambios' })}
        <label class="switch">
          <input type="checkbox" name="news_encrypted" checked=${settings.news_encrypted !== false}
                 onChange=${(event) => actions.setSetting('news_encrypted', event.target.checked)} />
          Conexión cifrada
        </label>

        <h2>Fichas de películas</h2>
        <p class="hint">Opcional. Con una clave de TMDB los títulos y las sugerencias salen en
          español, con el título original al lado.</p>
        ${field('tmdb_key', 'Clave de TMDB', { type: 'password' })}

        <h2>Subtítulos</h2>
        ${field('subtitles_key', 'Clave', { type: 'password' })}
        ${field('subtitles_agent', 'Nombre de la aplicación registrada')}
        ${field('subtitles_user', 'Usuario', { placeholder: 'el nombre de usuario, no el correo' })}
        ${field('subtitles_password', 'Contraseña', { type: 'password', placeholder: 'sin cambios' })}

        <h2>El archivo de ajustes</h2>
        <p class="hint">Todo esto se guarda aquí. Abrirlo sirve para mirarlo o para copiarlo a otro
          ordenador; después de cambiarlo a mano hay que reiniciar Mamá Cine.</p>
        <p class="path">${settings.settings_path}</p>
        <button type="button" class="quiet" id="open-settings-file"
                onClick=${actions.openSettingsFile}>
          Abrir el archivo de ajustes
        </button>

        <h2>El registro</h2>
        <p class="hint">Lo que ha ido pasando por dentro, y lo que dice el descargador cuando no
          arranca. Es lo primero que hay que mirar cuando algo falla, y lo que hay que enviar a
          quien pueda arreglarlo.</p>
        <p class="path">${settings.log_path}</p>
        <div class="two-buttons">
          <button type="button" class="quiet" id="open-log-file" onClick=${actions.openLogFile}>
            Abrir el registro
          </button>
          <button type="button" class="quiet" id="open-log-folder"
                  onClick=${actions.openLogFolder}>
            Abrir la carpeta
          </button>
        </div>
      </details>

      <div class="actions">
        <button class="primary" onClick=${actions.saveSettings}>Guardar</button>
        <button class="quiet" onClick=${actions.checkSettings}>Comprobar</button>
      </div>
      <${Notice} notice=${state.settingsNotice} />
      </div>
    </section>`;
}

// --- the app -----------------------------------------------------------------

function App() {
  const [state, setState] = useState({
    screen: 'search',
    query: '',
    placeholder: `${pick(SUGGESTIONS)}…`,
    suggestions: [],
    searching: false,
    searched: false,
    searchedExact: false,
    films: [],
    seasons: [],
    notice: null,
    shelfNotice: null,
    settingsNotice: null,
    problem: null,
    detail: null,
    detailSeries: false,
    synopsis: '',
    have: null,
    downloadingId: null,
    versions: null,
    showVersions: false,
    // her own film's copies, and the fold that asks for them
    copies: null,
    showCopies: false,
    loadingCopies: false,
    copiesProblem: null,
    confirmSwap: null,
    // a copy she has just picked, before the downloader has answered with its id
    starting: false,
    watching: null,
    ownedId: null,
    ownedEpisodes: null,
    ownedSynopsis: '',
    episodeAt: null,
    findingSubtitles: false,
    seasonEpisodes: [],
    confirmRemove: null,
    settings: null,
    progress: { active: [], finished: [], shelf: [], free_space: '', free_bytes: 0 },
  });

  const change = (fields) => setState((current) => ({ ...current, ...fields }));
  // For a change that is derived from the state it replaces. `latest` only catches up on a render,
  // so two edits in one tick would both build on the older state and the first would be lost:
  // typing a name and then an address before anything redrew left the name behind.
  const edit = (make) => setState((current) => ({ ...current, ...make(current) }));
  const latest = useRef(state);
  latest.current = state;
  const pending = useRef(null);
  // bumped whenever suggestions become unwanted, so a lookup already in flight when she hits
  // Enter cannot land afterwards and reopen the popover over her results
  const wanted = useRef(0);

  // the popover follows the conventions she knows: a click anywhere else, or Escape, closes it
  useEffect(() => {
    const outside = (event) => {
      if (!latest.current.suggestions.length) return;
      if (event.target.closest?.('.search-box')) return;
      actions.hideSuggestions();
    };
    const escape = (event) => {
      if (event.key === 'Escape' && latest.current.suggestions.length) {
        actions.hideSuggestions();
      }
    };
    document.addEventListener('click', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('click', outside);
      document.removeEventListener('keydown', escape);
    };
  }, []);

  useEffect(() => {
    invoke('read_settings')
      .then((settings) => change({
        settings: withoutPasswords(settings),
        screen: settings.ready ? 'search' : 'settings',
      }))
      .catch(() => {});
  }, []);

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      let progress;
      try {
        progress = await invoke('progress');
      } catch (error) {
        return;
      }
      if (!alive) return;

      const now = latest.current;
      let watching = now.watching;
      let downloadingId = now.downloadingId;
      if (watching?.id) {
        // a copy that failed hands over to the next one; the screen follows it rather than
        // stopping to tell her something she cannot act on
        let landed = progress.finished.find((film) => film.id === watching.id);
        while (landed?.next_id) {
          // the ficha's own band is following the same chase, so it follows the handover too
          if (downloadingId === watching.id) downloadingId = landed.next_id;
          // a new copy is a new download: nothing the dead one said about itself still holds
          watching = { ...watching, id: landed.next_id, status: 'retrying', detail: '', percent: 0 };
          landed = progress.finished.find((film) => film.id === landed.next_id);
        }
        const running = progress.active.find((film) => film.id === watching.id);
        if (running) {
          watching = { ...watching, detail: '', ...running };
        } else if (landed) {
          // failed-but-not-decided keeps the screen waiting: the chase may already be replacing
          // it. A dead copy's bar empties here, once, as the words change — never frozen at the
          // corpse's percent, never sliding backwards later.
          const status = landed.ok ? 'done' : landed.retrying ? 'retrying' : 'failed';
          watching = {
            ...watching,
            ...landed,
            status,
            percent: status === 'done' ? 100 : 0,
          };
        }
      } else if (!watching) {
        if (progress.active.length) {
          watching = { ...progress.active[0] };
        } else {
          // a film mid-chase lives in `finished` while its next copy is being decided; a fresh
          // window claiming "no downloads ongoing" over it was a lie
          const chasing = progress.finished.find((film) => film.retrying && !film.next_id);
          if (chasing) watching = { ...chasing, status: 'retrying', percent: 0 };
        }
      }
      // A copy that arrived is a film on her shelf, not a screen to read about it. If she is
      // watching it come down, the app goes where the film now is; if she is somewhere else,
      // the notification has already told her and nothing takes the screen out from under her.
      if (watching?.status === 'done') {
        // 'owned' is where a swap is started from, and the copy that lands is a new record: the
        // page she is on is about the one being replaced, so leaving her there showed her the
        // copy she had just changed, with nothing to say the new one was here.
        const looking = now.screen === 'film'
          || (['detail', 'owned'].includes(now.screen) && downloadingId === watching.id);
        change({
          progress, watching: null, downloadingId: null, problem: progress.problem || null,
        });
        if (looking) actions.openOwned(watching.id);
        return;
      }
      change({ progress, watching, downloadingId, problem: progress.problem || null });
    };

    tick();
    const beat = setInterval(tick, 1500);
    return () => { alive = false; clearInterval(beat); };
  }, []);

  const actions = {
    hideSuggestions: () => {
      wanted.current += 1;
      if (pending.current) clearTimeout(pending.current);
      change({ suggestions: [] });
    },

    show: (screen) => {
      actions.hideSuggestions();
      // a download she has seen settle is over: leaving the screen retires it, and the pill with
      // it, so a finished film does not ride the masthead forever
      edit((state) => ({
        screen,
        notice: null,
        watching: state.screen === 'film' && screen !== 'film'
          && ['done', 'failed'].includes(state.watching?.status)
          ? null
          : state.watching,
      }));
    },

    watch: (film) => change({ watching: { ...film } }),

    // As she types, titles are suggested, so a misspelling never ends in "no hay nada".
    typed: (query) => {
      change({ query });
      if (pending.current) clearTimeout(pending.current);
      wanted.current += 1;
      const generation = wanted.current;
      if (query.trim().length < 3) {
        change({ suggestions: [] });
        return;
      }
      pending.current = setTimeout(async () => {
        try {
          const suggestions = await invoke('suggest', { text: query });
          // only if nothing has made suggestions unwanted since: a search she already ran
          // must not have the popover land on top of its results
          if (generation === wanted.current) change({ suggestions });
        } catch (error) {
          if (generation === wanted.current) change({ suggestions: [] });
        }
      }, 300);
    },

    // She told us what it is by picking it: a series is not also searched as a film, which
    // buried the seasons she chose under parodies and episode reviews. Only the provider knows
    // how its id becomes a searchable query, so the pick is resolved by position.
    pickSuggestion: async (position) => {
      const title = latest.current.suggestions[position];
      if (!title) return;
      actions.hideSuggestions();
      // her language and nothing else: the original beside it helped her choose between two rows
      // and would only make the box hold something she could never have typed
      change({ query: title.title, searching: true });
      try {
        const picked = await invoke('pick_suggestion', { index: position });
        await actions.runSearch(picked.query, picked.series ? 'series' : 'film', picked.title);
      } catch (error) {
        // the resolver failing must not dead-end her: her words still make a search
        await actions.runSearch(title.title, title.series ? 'series' : 'film', title.title);
      }
    },

    submitSearch: (event) => {
      event.preventDefault();
      actions.hideSuggestions();
      actions.runSearch();
    },

    runSearch: async (asked, kind, shown) => {
      const query = asked ?? latest.current.query;
      if (!query.trim()) return;
      change({ searching: true, notice: null, films: [], seasons: [] });
      try {
        const found = await invoke('search',
                                   { query, kind: kind ?? null, shown: shown ?? null });
        change({
          films: found.films,
          seasons: found.seasons,
          searched: true,
          // the search knew which title she meant, whether she picked it or typed its name: an
          // empty answer means the sites do not carry it, and telling her to check her spelling
          // would blame her for that
          searchedExact: Boolean(found.exact),
          notice: found.notice ? { text: found.notice } : null,
        });
      } catch (error) {
        change({
          films: [], seasons: [], searched: true,
          notice: { text: String(error), bad: true },
        });
      } finally {
        change({ searching: false });
      }
    },

    // A tap opens the film, it does not start anything. Downloading is its own decision.
    openDetail: async (item, series) => {
      change({
        detail: item, detailSeries: series, have: null, downloadingId: null,
        versions: null, showVersions: false, synopsis: '', seasonEpisodes: [],
        copies: null, showCopies: false, copiesProblem: null, confirmSwap: null,
        starting: false, screen: 'detail', notice: null,
      });
      // the synopsis is garnish: without a TMDB key there is none, and the ficha stands without
      // it, so a refusal here changes nothing she can act on
      if (!series) {
        invoke('synopsis', { index: item.index })
          .then((synopsis) => edit((state) => (
            state.detail === item ? { synopsis: synopsis || '' } : {})))
          .catch(() => {});
      }
      // what a season actually holds, when the show database knows: garnish in the same way
      if (series) {
        invoke('season_episodes', { index: item.index })
          .then((episodes) => edit((state) => (
            state.detail === item ? { seasonEpisodes: episodes || [] } : {})))
          .catch(() => {});
      }
      try {
        change({ versions: await invoke('versions', { index: item.index, series }) });
      } catch (error) {
        change({ versions: [], notice: { text: String(error), bad: true } });
      }
      // whether she has it is decided by her folder, not by what a downloader remembers
      try {
        const owned = await invoke('have', { index: item.index, series });
        // opened onto a copy already coming down: the band below the ficha follows it from here,
        // so the live row it needs is picked up rather than waited for
        const running = owned.downloading
          && latest.current.progress.active.find((film) => film.id === owned.downloading);
        change({
          have: owned.have,
          downloadingId: owned.downloading,
          ...(running ? { watching: { ...running } } : {}),
        });
      } catch (error) {
        change({ have: null, downloadingId: null });
      }
    },

    // the film is already on its way: show her the download instead of offering it again
    watchDownload: (id) => {
      const { detail, detailSeries } = latest.current;
      change({
        watching: {
          id,
          title: detailSeries ? `${detail.show} · ${detail.label}` : detail.title,
          year: detail.year,
          cover_url: detail.cover_url,
          series: detailSeries,
          status: 'starting',
          percent: 0,
        },
        screen: 'film',
      });
    },

    toggleVersions: () => edit((state) => ({
      showVersions: !state.showVersions, confirmSwap: null,
    })),

    // Her own film's copies are a question for the indexer, so they are asked for rather than
    // fetched on every open: opening a film of hers must work with nothing else answering.
    toggleCopies: async () => {
      const { showCopies, copies, ownedId } = latest.current;
      if (showCopies) {
        change({ showCopies: false, confirmSwap: null });
        return;
      }
      change({ showCopies: true, confirmSwap: null, copiesProblem: null });
      if (copies) return;
      change({ loadingCopies: true });
      try {
        const found = await invoke('copies', { id: ownedId });
        if (latest.current.ownedId === ownedId) change({ copies: found });
      } catch (error) {
        if (latest.current.ownedId === ownedId) change({ copiesProblem: String(error) });
      } finally {
        change({ loadingCopies: false });
      }
    },

    // Swapping ends with a film of hers in the papelera, so it is asked before it is done.
    confirmSwap: (version) => change({ confirmSwap: version }),

    swapCopy: async (version) => {
      const { copies, ownedId, progress } = latest.current;
      if (!copies) return;
      const film = (progress.shelf || []).find((item) => item.id === ownedId);
      change({
        confirmSwap: null,
        showCopies: false,
        starting: true,
        shelfNotice: null,
        watching: {
          title: film?.title || '', year: film?.year, cover_url: film?.cover_url,
          series: Boolean(film?.series), status: 'starting', percent: 0,
        },
      });
      try {
        const grabbed = await invoke('grab', {
          index: copies.index, version, series: copies.series, replacing: ownedId,
        });
        // nothing was started, so there is nothing to watch: saying otherwise left a bar at zero
        // that emptied itself a second later
        if (grabbed.already) {
          change({ starting: false, watching: null });
          return;
        }
        change({
          watching: { ...latest.current.watching, id: grabbed.id },
          downloadingId: grabbed.id,
          starting: false,
        });
      } catch (error) {
        change({ starting: false, watching: null, shelfNotice: { text: String(error), bad: true } });
      }
    },

    download: async (version, replacing = null) => {
      const { detail, detailSeries } = latest.current;
      if (!detail) return;
      const watching = {
        title: detailSeries ? `${detail.show} · ${detail.label}` : detail.title,
        year: detail.year,
        cover_url: detail.cover_url,
        series: detailSeries,
        status: 'starting',
        percent: 0,
      };
      // the ficha stays: it is the page about this film, and what the copy is doing is a band
      // under it rather than a screen somewhere else
      // said on the screen the moment she picks it. The masthead knew instantly and the copies
      // list sat there for seconds, so picking a copy read as having done nothing.
      change({
        watching, notice: null, starting: true, confirmSwap: null, showVersions: false,
      });
      try {
        const grabbed = await invoke('grab', {
          index: detail.index, version, series: detailSeries, replacing,
        });
        if (grabbed.already) {
          // nothing was started: she has it already
          change({ watching: null, have: grabbed.id, starting: false });
          return;
        }
        change({
          watching: { ...latest.current.watching, id: grabbed.id },
          downloadingId: grabbed.id,
          starting: false,
        });
      } catch (error) {
        change({
          starting: false,
          watching: { ...latest.current.watching, status: 'failed', detail: String(error) },
        });
      }
    },

    // the give-up screen's door: spend the copies the chase kept beyond its limit
    tryMore: async (id) => {
      try {
        const grabbed = await invoke('try_more', { id });
        change({
          watching: {
            ...latest.current.watching,
            id: grabbed.id, status: 'retrying', detail: '', percent: 0,
          },
          // the band under the ficha is about this film, so it follows the copy being spent
          ...(latest.current.downloadingId === id ? { downloadingId: grabbed.id } : {}),
        });
      } catch (error) {
        actions.tell('notice')(error);
      }
    },

    cancel: async (id) => {
      if (id) await invoke('cancel', { id });
      change({ watching: null, screen: 'search' });
    },

    // a button that fails silently is the app lying by omission
    tell: (field) => (error) => change({ [field]: { text: String(error), bad: true } }),

    // Her own copy of something, opened: what it is, what it is about, and its episodes when it
    // has any. Nothing here plays anything; the page it opens is where that decision is made.
    openOwned: async (id) => {
      change({
        ownedId: id,
        ownedEpisodes: null,
        ownedSynopsis: '',
        episodeAt: null,
        shelfNotice: null,
        screen: 'owned',
        copies: null, showCopies: false, copiesProblem: null, confirmSwap: null,
        starting: false,
      });
      invoke('library_synopsis', { id })
        .then((words) => {
          if (latest.current.ownedId === id) change({ ownedSynopsis: words });
        })
        .catch(() => {});
      await actions.loadEpisodes(id);
    },

    loadEpisodes: async (id) => {
      const film = (latest.current.progress.shelf || []).find((item) => item.id === id);
      if (!film?.series) return;
      try {
        const episodes = await invoke('episodes', { id });
        if (latest.current.ownedId === id) change({ ownedEpisodes: episodes });
      } catch (error) {
        change({ shelfNotice: { text: String(error), bad: true } });
      }
    },

    openEpisode: (position) => change({ episodeAt: position, screen: 'episode' }),

    play: (film) => invoke('play', { id: film.id }).catch(actions.tell('shelfNotice')),

    playEpisode: (film, position) => invoke('play_episode', { id: film.id, position })
      .catch(actions.tell('shelfNotice')),

    reveal: (film) => invoke('reveal', { id: film.id }).catch(actions.tell('shelfNotice')),

    confirmRemove: (id) => change({ confirmRemove: id }),

    remove: async (film) => {
      change({ confirmRemove: null });
      try {
        await invoke('remove_film', { id: film.id });
        change({
          screen: 'library',
          ownedId: null,
          shelfNotice: {
            text: `He enviado ${film.title} a la papelera. Desde ahí todavía la puedes recuperar.`,
          },
        });
      } catch (error) {
        change({ shelfNotice: { text: String(error), bad: true } });
      }
    },

    // Only ever the missing ones: what is already there is a state on the screen, not an errand.
    // The episode list is asked again afterwards, because which episodes can be understood is
    // exactly what may have changed.
    refetchSubtitles: async (film) => {
      change({ findingSubtitles: true, shelfNotice: { text: 'Buscando subtítulos…' } });
      try {
        const said = await invoke('fetch_subtitles', { id: film.id });
        change({ findingSubtitles: false, shelfNotice: { text: said } });
      } catch (error) {
        change({ findingSubtitles: false, shelfNotice: { text: String(error), bad: true } });
      }
      await actions.loadEpisodes(film.id);
    },

    setSetting: (name, value) => edit((state) => ({
      settings: { ...state.settings, [name]: value },
    })),

    setIndexer: (position, field, value) => edit((state) => {
      const indexers = [...(state.settings.indexers || [])];
      indexers[position] = { ...indexers[position], [field]: value };
      return { settings: { ...state.settings, indexers } };
    }),

    addIndexer: () => edit((state) => ({
      settings: {
        ...state.settings,
        indexers: [...(state.settings.indexers || []),
                   { name: '', url: '', key: '', enabled: true }],
      },
    })),

    removeIndexer: (position) => edit((state) => ({
      settings: {
        ...state.settings,
        indexers: (state.settings.indexers || []).filter((_, index) => index !== position),
      },
    })),

    chooseFolder: async () => {
      const chosen = await invoke('choose_folder');
      if (chosen) actions.setSetting('destination', chosen);
    },

    openSettingsFile: async () => {
      try {
        await invoke('open_settings_file');
      } catch (error) {
        change({ settingsNotice: { text: String(error), bad: true } });
      }
    },

    // reachable from the problem banner as well as from the settings screen, so the notice goes
    // where she is looking: the settings screen is not where a failed start leaves her
    openLogFile: async () => {
      try {
        await invoke('open_log_file');
      } catch (error) {
        change({ notice: { text: String(error), bad: true },
                 settingsNotice: { text: String(error), bad: true } });
      }
    },

    openLogFolder: async () => {
      try {
        await invoke('open_log_folder');
      } catch (error) {
        change({ settingsNotice: { text: String(error), bad: true } });
      }
    },

    saveSettings: async () => {
      change({ settingsNotice: { text: 'Guardando…' } });
      try {
        const saved = await invoke('save_settings', { incoming: latest.current.settings });
        change({
          settings: withoutPasswords(saved),
          settingsNotice: {
            text: saved.ready
              ? 'Guardado. Ya puedes buscar películas.'
              : 'Guardado, pero todavía falta un buscador o el servidor de descargas.',
          },
        });
      } catch (error) {
        change({ settingsNotice: { text: String(error), bad: true } });
      }
    },

    checkSettings: async () => {
      change({ settingsNotice: { text: 'Comprobando…' } });
      try {
        const report = await invoke('check_settings', { incoming: latest.current.settings });
        change({ settingsNotice: { text: report } });
      } catch (error) {
        change({ settingsNotice: { text: String(error), bad: true } });
      }
    },
  };

  const screens = {
    search: Search,
    detail: Detail,
    film: Now,
    library: Shelf,
    owned: Owned,
    episode: EpisodeScreen,
    settings: Settings,
  };
  const Screen = screens[state.screen] || Search;
  const following = state.watching;
  const otherDownloads = following
    ? state.progress.active.filter((active) => active.id !== following.id).length
    : 0;

  // The masthead: the app's name, the two places she goes, the download she is waiting for, and
  // the gear for the screen she fills once. Ajustes earns an icon, not a tab.
  return html`
    <header>
      <button class="brand" title="Volver al inicio" onClick=${() => actions.show('search')}>
        <img class="mark" src="icon.svg" alt="" />
        <span>Mamá Cine</span>
      </button>
      <nav>
        ${BACK_TO[state.screen] && html`
          <button class="back" onClick=${() => actions.show(BACK_TO[state.screen])}>
            <span aria-hidden="true">←</span> Volver</button>`}
        ${[['search', 'Buscar'], ['library', 'Mi colección']].map(([screen, label]) => html`
          <button key=${screen} data-screen=${screen}
                  class=${state.screen === screen ? 'on' : ''}
                  onClick=${() => actions.show(screen)}>${label}</button>`)}
        <span class="grow"></span>
        ${following && html`
          <${Pill} film=${following} others=${otherDownloads}
                   on=${state.screen === 'film'}
                   onOpen=${() => actions.show('film')} />`}
        <button class=${`gear ${state.screen === 'settings' ? 'on' : ''}`} data-screen="settings"
                title="Ajustes" aria-label="Ajustes"
                onClick=${() => actions.show('settings')}>⚙</button>
      </nav>
    </header>
    ${lowOnSpace(state.progress) && html`<div id="space" class="space">
      Queda poco sitio en el disco: solo ${state.progress.free_space} libres.
      Borra alguna película que ya hayas visto.
    </div>`}
    <${Problem} problem=${state.problem} actions=${actions} />
    <main>
      <${Screen} state=${state} actions=${actions} />
    </main>`;
}

render(html`<${App} />`, document.getElementById('app'));
