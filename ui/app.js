
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

// Everything the window says, per language. `es` is the reference; a missing key anywhere else
// falls back to it. Adding a language is adding an entry here and a chip in Ajustes.
const STRINGS = {
  es: {
    statusWords: {
      starting: 'Empezando la descarga…',
      downloading: 'Descargando',
      verifying: 'Comprobando que está completa…',
      repairing: 'Recuperando lo que falta…',
      unpacking: 'Casi lista…',
      moving: 'Guardando…',
      finishing: 'Últimos detalles…',
      paused: 'En pausa',
      waiting: 'Esperando a que vuelva el servidor…',
      retrying: 'Buscando otra copia…',
      done: 'Lista para ver',
      failed: 'No he podido conseguirla',
    },
    working: 'Trabajando…',
    trackNouns: {
      spa: 'español', es: 'español', esp: 'español', cast: 'español',
      eng: 'inglés', en: 'inglés', ger: 'alemán', deu: 'alemán', de: 'alemán',
      fre: 'francés', fra: 'francés', fr: 'francés', ita: 'italiano', it: 'italiano',
      por: 'portugués', pt: 'portugués', jpn: 'japonés', ja: 'japonés', kor: 'coreano',
      chi: 'chino', zho: 'chino', rus: 'ruso', ara: 'árabe', hin: 'hindi', tur: 'turco',
      dut: 'neerlandés', nld: 'neerlandés', swe: 'sueco', dan: 'danés', nor: 'noruego',
      fin: 'finés', pol: 'polaco', ces: 'checo', hun: 'húngaro', ell: 'griego',
      cat: 'catalán', eus: 'euskera', glg: 'gallego', rum: 'rumano', ron: 'rumano',
      ro: 'rumano', und: 'sin etiquetar',
      'es-419': 'español latinoamericano', 'pt-br': 'portugués brasileño',
    },
    storySummary: 'Qué ha pasado',
    audioUntagged: 'audio sin etiquetar',
    audioIn: (names) => `audio en ${names}`,
    subtitlesIn: (names) => `subtítulos en ${names}`,
    unknownLanguage: 'idioma desconocido',
    takesAbout: (minutes) => `tardará ${minutes > 90
      ? `unas ${Math.round(minutes / 60)} horas` : `unos ${minutes} minutos`}`,
    percentFree: (percent) => `${percent} % libre`,
    pillFailed: 'No he podido',
    pillDone: 'Lista',
    backHome: 'Volver al inicio',
    back: 'Volver',
    navSearch: 'Buscar',
    navLibrary: 'Mi colección',
    navSettings: 'Ajustes',
    lowSpace: (free) => `Queda poco sitio en el disco: solo ${free} libres.
      Borra alguna película que ya hayas visto.`,
    openLog: 'Abrir el registro',
    searchTitle: '¿Qué te apetece ver hoy?',
    searchLead: 'Escribe el nombre de una película o de una serie.',
    searchButton: 'Buscar',
    series: 'Serie',
    emptyExact: 'Existe, pero ahora mismo no la encuentro en los sitios donde busco. Puede que aparezca más adelante.',
    emptyNoMatch: 'No hay nada con ese nombre. Escríbelo otra vez y elige uno de los títulos que aparecen.',
    copiesWaiting: 'Buscando otras copias…',
    chosenMark: 'la elegida',
    grabs: (count) => `${count} descargas`,
    swapQuestion: '¿Cambiar la copia?',
    yesSwap: 'Sí, cambiar',
    no: 'No',
    completeSeason: 'Temporada completa',
    thisSeason: 'esta temporada',
    thisFilm: 'esta película',
    alreadyHave: (thing) => `Ya tienes ${thing} en este ordenador.`,
    alreadyDownloading: 'Ya se está descargando.',
    occupies: (size) => `Ocupa ${size}`,
    freeLeft: (free) => `quedan ${free} libres`,
    notEnoughRoom: 'No hay sitio suficiente',
    mayNotFit: 'Puede que no quepa',
    needsWhilePreparing: (needs) => `mientras se descarga y se prepara, necesita unos ${needs}.`,
    willArriveIn: (what) => `Se descargará en ${what}`,
    watchFilm: 'Ver la película',
    watchEpisodes: 'Ver los episodios',
    watchProgress: 'Ver cómo va',
    download: 'Descargar',
    episodeCount: (count) => `Son ${count} episodios.`,
    severalEpisodes: 'Son varios episodios.',
    afterDownload: 'Cuando termine la descarga, podrás verlos aquí, uno a uno.',
    imdbEpisodes: 'Ver los episodios en IMDb',
    imdbPage: 'Ver la ficha en IMDb',
    tryMore: (untried) => `Probar más copias (quedan ${untried})`,
    cancelDownload: 'Cancelar la descarga',
    hideCopies: 'Ocultar las copias',
    otherCopiesCount: (count) => `Otras copias (${count})`,
    otherCopies: 'Ver otras copias',
    swapToThisCopy: 'Cambiar a esta copia',
    downloadThisCopy: 'Descargar esta copia',
    nothingDownloading: 'Ahora mismo no se está descargando nada.',
    searchSomething: 'Buscar algo',
    alsoDownloadingOne: 'También se está descargando:',
    alsoDownloadingMany: 'También se están descargando:',
    diskAlmostFull: (free) => `El disco está casi lleno: quedan ${free} libres. `
      + 'Borra alguna película que ya hayas visto.',
    searchAnother: 'Buscar otra',
    myFilms: 'Mis películas y series',
    myFilmsLead: 'Todo lo que ya está en este ordenador.',
    emptyShelf: 'Todavía no hay nada aquí.',
    subtitlesReady: 'Subtítulos en español',
    subtitlesMissing: 'Sin subtítulos en español',
    subtitleNoteReadsBad: /^(no hay|sin|faltan|no se|there are no|no subtitles|subtitles are missing|subtitles could not|the subtitles that exist)/i,
    findSubtitlesAgain: 'Buscar los subtítulos otra vez',
    findAgain: 'Buscar otra vez',
    findingShort: 'Buscando…',
    ownedGone: 'Esto ya no está en este ordenador.',
    startWithEpisode: (number) => `Empezar por el episodio ${number}`,
    startWithFirst: 'Empezar por el primero',
    openFolder: 'Abrir la carpeta',
    areYouSure: '¿Seguro?',
    yesDelete: 'Sí, borrar',
    deleteWord: 'Borrar',
    withoutSubtitles: 'sin subtítulos',
    episodesWaiting: 'Buscando los episodios…',
    emptySeasonFolder: 'En esta carpeta no queda ningún episodio.',
    episodeGone: 'Ese episodio ya no está en la carpeta.',
    episodeNoSubtitles: 'Este episodio no tiene subtítulos en español',
    watchThisEpisode: 'Ver este episodio',
    settingsTitle: 'Ajustes',
    settingsLead: 'Esto se rellena una vez. Después no hay que volver aquí.',
    whereFilmsGo: 'Dónde se guardan las películas',
    folder: 'Carpeta',
    unchosen: 'Sin elegir',
    choose: 'Elegir…',
    inWhatLanguage: 'En qué idioma',
    languageAny: 'Me da igual',
    languageSpanish: 'En español',
    languageOriginal: 'Versión original',
    appLanguage: 'El idioma de la aplicación',
    appLanguageAuto: 'El del ordenador',
    startAndClose: 'Al encender y al cerrar',
    autostartLabel: 'Abrir Mamá Cine al encender el ordenador',
    keepRunningLabel: 'Al cerrar la ventana, seguir con las descargas',
    trayHint: 'La aplicación se queda como un icono pequeño junto al reloj y avisa cuando una película está lista.',
    technical: 'Ajustes técnicos',
    whereToSearch: 'Dónde buscar',
    indexersHint: 'Puedes añadir varios: cuantos más, más películas encontrarás.',
    nameLabel: 'Nombre',
    addressLabel: 'Dirección',
    keyLabel: 'Clave',
    inUse: 'En uso',
    removeIndexer: 'Quitar',
    addIndexer: 'Añadir otro buscador',
    newsServer: 'Servidor de descargas',
    serverLabel: 'Servidor',
    portLabel: 'Puerto',
    connectionsLabel: 'Conexiones',
    userLabel: 'Usuario',
    passwordLabel: 'Contraseña',
    unchangedPlaceholder: 'sin cambios',
    encryptedLabel: 'Conexión cifrada',
    filmPages: 'Fichas de películas',
    tmdbHint: 'Opcional. Con una clave de TMDB los títulos y las sugerencias salen en español, con el título original al lado.',
    tmdbKeyLabel: 'Clave de TMDB',
    subtitlesHeading: 'Subtítulos',
    agentLabel: 'Nombre de la aplicación registrada',
    userNotEmail: 'el nombre de usuario, no el correo',
    settingsFileHeading: 'El archivo de ajustes',
    settingsFileHint: 'Todo esto se guarda aquí. Abrirlo sirve para mirarlo o para copiarlo a otro ordenador; después de cambiarlo a mano hay que reiniciar Mamá Cine.',
    openSettingsFile: 'Abrir el archivo de ajustes',
    logHeading: 'El registro',
    logHint: 'Lo que ha ido pasando por dentro, y lo que dice el descargador cuando no arranca. Es lo primero que hay que mirar cuando algo falla, y lo que hay que enviar a quien pueda arreglarlo.',
    openLogFolder: 'Abrir la carpeta',
    save: 'Guardar',
    check: 'Comprobar',
    saving: 'Guardando…',
    checking: 'Comprobando…',
    savedReady: 'Guardado. Ya puedes buscar películas.',
    savedMissing: 'Guardado, pero todavía falta un buscador o el servidor de descargas.',
    findingSubtitles: 'Buscando subtítulos…',
    sentToBin: (title) => `He enviado ${title} a la papelera. Desde ahí todavía la puedes recuperar.`,
    updateAvailable: (version) => `Hay una versión nueva de Mamá Cine (${version}).`,
    installUpdate: 'Instalarla',
    updating: 'Descargando…',
    updateInstalled: (version) => `Mamá Cine ${version} ya está instalada. `
      + 'Se estrenará la próxima vez que abras la aplicación.',
  },
  en: {
    statusWords: {
      starting: 'Starting the download…',
      downloading: 'Downloading',
      verifying: 'Checking it is complete…',
      repairing: 'Recovering what is missing…',
      unpacking: 'Nearly ready…',
      moving: 'Saving…',
      finishing: 'Last details…',
      paused: 'Paused',
      waiting: 'Waiting for the server to come back…',
      retrying: 'Looking for another copy…',
      done: 'Ready to watch',
      failed: 'I could not get it',
    },
    working: 'Working…',
    trackNouns: {
      spa: 'Spanish', es: 'Spanish', esp: 'Spanish', cast: 'Spanish',
      eng: 'English', en: 'English', ger: 'German', deu: 'German', de: 'German',
      fre: 'French', fra: 'French', fr: 'French', ita: 'Italian', it: 'Italian',
      por: 'Portuguese', pt: 'Portuguese', jpn: 'Japanese', ja: 'Japanese', kor: 'Korean',
      chi: 'Chinese', zho: 'Chinese', rus: 'Russian', ara: 'Arabic', hin: 'Hindi', tur: 'Turkish',
      dut: 'Dutch', nld: 'Dutch', swe: 'Swedish', dan: 'Danish', nor: 'Norwegian',
      fin: 'Finnish', pol: 'Polish', ces: 'Czech', hun: 'Hungarian', ell: 'Greek',
      cat: 'Catalan', eus: 'Basque', glg: 'Galician', rum: 'Romanian', ron: 'Romanian',
      ro: 'Romanian', und: 'untagged',
      'es-419': 'Latin American Spanish', 'pt-br': 'Brazilian Portuguese',
    },
    storySummary: 'What has happened',
    audioUntagged: 'untagged audio',
    audioIn: (names) => `audio in ${names}`,
    subtitlesIn: (names) => `subtitles in ${names}`,
    unknownLanguage: 'language unknown',
    takesAbout: (minutes) => `will take ${minutes > 90
      ? `about ${Math.round(minutes / 60)} hours` : `about ${minutes} minutes`}`,
    percentFree: (percent) => `${percent} % free`,
    pillFailed: 'I could not',
    pillDone: 'Ready',
    backHome: 'Back to the start',
    back: 'Back',
    navSearch: 'Search',
    navLibrary: 'My collection',
    navSettings: 'Settings',
    lowSpace: (free) => `Little room left on the disk: only ${free} free.
      Remove a film you have already watched.`,
    openLog: 'Open the log',
    searchTitle: 'What do you feel like watching today?',
    searchLead: 'Type the name of a film or a series.',
    searchButton: 'Search',
    series: 'Series',
    emptyExact: 'It exists, but right now I cannot find it in the places I search. It may appear later on.',
    emptyNoMatch: 'There is nothing by that name. Type it again and pick one of the titles that appear.',
    copiesWaiting: 'Looking for other copies…',
    chosenMark: 'the chosen one',
    grabs: (count) => `${count} downloads`,
    swapQuestion: 'Swap the copy?',
    yesSwap: 'Yes, swap',
    no: 'No',
    completeSeason: 'Complete season',
    thisSeason: 'this season',
    thisFilm: 'this film',
    alreadyHave: (thing) => `You already have ${thing} on this computer.`,
    alreadyDownloading: 'It is already downloading.',
    occupies: (size) => `Takes up ${size}`,
    freeLeft: (free) => `${free} free`,
    notEnoughRoom: 'There is not enough room',
    mayNotFit: 'It may not fit',
    needsWhilePreparing: (needs) => `while it downloads and unpacks, it needs about ${needs}.`,
    willArriveIn: (what) => `It will arrive in ${what}`,
    watchFilm: 'Watch the film',
    watchEpisodes: 'See the episodes',
    watchProgress: 'See how it is going',
    download: 'Download',
    episodeCount: (count) => `That is ${count} episodes.`,
    severalEpisodes: 'That is several episodes.',
    afterDownload: 'When the download finishes, you can watch them here, one by one.',
    imdbEpisodes: 'See the episodes on IMDb',
    imdbPage: 'See the page on IMDb',
    tryMore: (untried) => `Try more copies (${untried} left)`,
    cancelDownload: 'Cancel the download',
    hideCopies: 'Hide the copies',
    otherCopiesCount: (count) => `Other copies (${count})`,
    otherCopies: 'See other copies',
    swapToThisCopy: 'Swap to this copy',
    downloadThisCopy: 'Download this copy',
    nothingDownloading: 'Nothing is downloading right now.',
    searchSomething: 'Search for something',
    alsoDownloadingOne: 'Also downloading:',
    alsoDownloadingMany: 'Also downloading:',
    diskAlmostFull: (free) => `The disk is nearly full: ${free} free. `
      + 'Remove a film you have already watched.',
    searchAnother: 'Search for another',
    myFilms: 'My films and series',
    myFilmsLead: 'Everything already on this computer.',
    emptyShelf: 'Nothing here yet.',
    subtitlesReady: 'Subtitles',
    subtitlesMissing: 'No subtitles',
    subtitleNoteReadsBad: /^(no hay|sin|faltan|no se|there are no|no subtitles|subtitles are missing|subtitles could not|the subtitles that exist)/i,
    findSubtitlesAgain: 'Look for the subtitles again',
    findAgain: 'Look again',
    findingShort: 'Looking…',
    ownedGone: 'This is no longer on this computer.',
    startWithEpisode: (number) => `Start with episode ${number}`,
    startWithFirst: 'Start with the first one',
    openFolder: 'Open the folder',
    areYouSure: 'Are you sure?',
    yesDelete: 'Yes, delete',
    deleteWord: 'Delete',
    withoutSubtitles: 'no subtitles',
    episodesWaiting: 'Looking for the episodes…',
    emptySeasonFolder: 'No episode remains in this folder.',
    episodeGone: 'That episode is no longer in the folder.',
    episodeNoSubtitles: 'This episode has no subtitles',
    watchThisEpisode: 'Watch this episode',
    settingsTitle: 'Settings',
    settingsLead: 'This is filled in once. After that there is no need to come back.',
    whereFilmsGo: 'Where the films are saved',
    folder: 'Folder',
    unchosen: 'Not chosen',
    choose: 'Choose…',
    inWhatLanguage: 'In which language',
    languageAny: 'I do not mind',
    languageSpanish: 'In Spanish',
    languageOriginal: 'Original version',
    appLanguage: 'The app\'s language',
    appLanguageAuto: 'The computer\'s',
    startAndClose: 'On startup and on closing',
    autostartLabel: 'Open Mamá Cine when the computer starts',
    keepRunningLabel: 'When the window closes, keep downloading',
    trayHint: 'The app stays as a small icon by the clock and says when a film is ready.',
    technical: 'Technical settings',
    whereToSearch: 'Where to search',
    indexersHint: 'You can add several: the more there are, the more films you will find.',
    nameLabel: 'Name',
    addressLabel: 'Address',
    keyLabel: 'Key',
    inUse: 'In use',
    removeIndexer: 'Remove',
    addIndexer: 'Add another indexer',
    newsServer: 'Download server',
    serverLabel: 'Server',
    portLabel: 'Port',
    connectionsLabel: 'Connections',
    userLabel: 'User',
    passwordLabel: 'Password',
    unchangedPlaceholder: 'unchanged',
    encryptedLabel: 'Encrypted connection',
    filmPages: 'Film pages',
    tmdbHint: 'Optional. With a TMDB key, titles and suggestions arrive in your language, with the original title beside them.',
    tmdbKeyLabel: 'TMDB key',
    subtitlesHeading: 'Subtitles',
    agentLabel: 'Registered application name',
    userNotEmail: 'the username, not the email',
    settingsFileHeading: 'The settings file',
    settingsFileHint: 'All of this is saved here. Opening it is for looking, or for copying to another computer; after editing it by hand, restart Mamá Cine.',
    openSettingsFile: 'Open the settings file',
    logHeading: 'The log',
    logHint: 'What has been happening inside, and what the downloader says when it does not start. It is the first thing to look at when something fails, and the thing to send to whoever can fix it.',
    openLogFolder: 'Open the folder',
    save: 'Save',
    check: 'Check',
    saving: 'Saving…',
    checking: 'Checking…',
    savedReady: 'Saved. You can search for films now.',
    savedMissing: 'Saved, but an indexer or the download server is still missing.',
    findingSubtitles: 'Looking for subtitles…',
    sentToBin: (title) => `I sent ${title} to the recycle bin. It can still be recovered from there.`,
    updateAvailable: (version) => `There is a new version of Mamá Cine (${version}).`,
    installUpdate: 'Install it',
    updating: 'Downloading…',
    updateInstalled: (version) => `Mamá Cine ${version} is installed. `
      + 'It starts the next time you open the app.',
  },
};

const initialLang = (navigator.language || 'es').toLowerCase().startsWith('es') ? 'es' : 'en';
let currentLang = initialLang;
let T = STRINGS[currentLang] || STRINGS.es;


const BACK_TO = { detail: 'search', owned: 'library', episode: 'owned' };

const languageName = (code) => {
  const plain = code.toLowerCase();
  if (T.trackNouns[plain]) return T.trackNouns[plain];
  const [base, region] = plain.split('-');
  if (region && T.trackNouns[base]) return `${T.trackNouns[base]} (${region.toUpperCase()})`;
  return code;
};

const named = (codes) => [...new Set(codes.map(languageName))];

const midSentence = (text) => (text ? text[0].toLowerCase() + text.slice(1) : text);
const SPANISH = ['spa', 'es', 'esp', 'cast', 'spanish'];
const inSpanish = (code) => SPANISH.includes(code.toLowerCase().split('-')[0]);

const clock = () => new Intl.DateTimeFormat(currentLang, { hour: '2-digit', minute: '2-digit' });
const calendar = () => new Intl.DateTimeFormat(currentLang, { day: 'numeric', month: 'short' });

function when(at) {
  const then = new Date(at * 1000);
  const today = new Date().toDateString() === then.toDateString();
  return today ? clock().format(then) : `${calendar().format(then)} ${clock().format(then)}`;
}

function Story({ story, except }) {
  if (!story?.length) return null;
  const latest = story[story.length - 1];
  const repeat = except && latest.said === except;
  return html`<${Fragment}>
    ${!repeat && html`
      <p class="latest" id="story-latest" title=${latest.why}>${latest.said}</p>`}
    ${story.length > 1 && html`
      <details class="story-fold">
        <summary>${T.storySummary}</summary>
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

function spokenIn(film) {
  const tracks = film.languages || {};
  const audio = tracks.audio_languages || [];
  const subtitles = tracks.subtitle_languages || [];
  if (!audio.length && !subtitles.length) return '';

  const parts = [];
  if (audio.length) {
    parts.push(audio.every((code) => code === 'und')
      ? T.audioUntagged
      : T.audioIn(named(audio.filter((code) => code !== 'und')).join(', ')));
  }
  if (subtitles.length) parts.push(T.subtitlesIn(named(subtitles).join(', ')));
  if (![...audio, ...subtitles].some((code) => code !== 'und')) parts.push(T.unknownLanguage);
  return parts.join(' · ');
}

function timeWords(minutes) {
  if (!minutes) return '';
  return T.takesAbout(minutes);
}

const LOW_SPACE = 40 * 1024 ** 3;
const NEARLY_FULL = 20 * 1024 ** 3;

const lowOnSpace = (progress) => progress.free_bytes > 0
  && progress.free_bytes < LOW_SPACE;

function Meter({ free, total, slice, low }) {
  if (!total) return null;
  const used = Math.max(0, total - free);
  const usedShare = Math.min(100, (used / total) * 100);
  const sliceShare = slice
    ? Math.min(100 - usedShare, Math.max(0.8, (slice / total) * 100))
    : 0;
  const percent = Math.round((free / total) * 100);
  return html`<span class="meter ${low ? 'low' : ''}"
        title=${T.percentFree(percent)} role="img" aria-label=${T.percentFree(percent)}>
    <i class="used" style=${`width: ${usedShare}%`}></i>
    ${sliceShare > 0 && html`<i class="slice" style=${`width: ${sliceShare}%`}></i>`}
  </span>`;
}

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
        ? html`<span class="word">${T.pillFailed}</span>`
        : html`<${Fragment}>
            <span class="mini"><i style=${`width: ${percent}%`}></i></span>
            <span class="word">${settled ? T.pillDone : `${percent} %`}</span>
          <//>`}
      ${others > 0 && html`<span class="badge" id="pill-count">+${others}</span>`}
    </button>`;
}

const posters = new Map();

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

function Problem({ problem, actions }) {
  if (!problem) return null;
  return html`<div id="problem" class="problem">
    ${problem}
    <button type="button" class="link" id="problem-log" onClick=${actions.openLogFile}>
      ${T.openLog}
    </button>
  </div>`;
}

function Search({ state, actions }) {
  const results = [
    ...state.films.map((item) => ({ item, series: false })),
    ...state.seasons.map((item) => ({ item, series: true })),
  ].sort((a, b) => (b.item.relevance ?? 0) - (a.item.relevance ?? 0));
  return html`
    <section class="screen" id="screen-search">
      <h1 id="search-title">${T.searchTitle}</h1>
      <p class="lead" id="search-lead">${T.searchLead}</p>

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
                      ${[title.year, title.series ? T.series : ''].filter(Boolean).join(' · ')}
                    </span>
                  </span>
                </button>`)}
            </div>`}
        </div>
        <button class="primary" type="submit" disabled=${state.searching}>${T.searchButton}</button>
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
                ${state.searchedExact ? T.emptyExact : T.emptyNoMatch}
              </div>`}
      </div>
    </section>`;
}

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

function Copies({ versions, open, verb, onPick, confirming, onConfirm, onDismiss,
                  loading, problem }) {
  if (!open) return null;
  return html`
    <div class="versions" id="copies">
      ${loading && html`<p class="factline" id="copies-waiting">${T.copiesWaiting}</p>`}
      ${problem && html`<${Notice} notice=${{ text: problem, bad: true }} />`}
      ${(versions || []).map((version) => html`
        <div class="version" key=${version.index} title=${version.name}>
          <span class="what">${version.quality} · ${version.size}</span>
          <span class="who">${version.language} · ${T.grabs(version.grabs)}</span>
          ${version.chosen && html`<span class="mark">${T.chosenMark}</span>`}
          ${confirming === version.index
            ? html`<span class="confirm">
                <span class="word">${T.swapQuestion}</span>
                <button class="quiet bad" onClick=${() => onConfirm(version)}>${T.yesSwap}</button>
                <button class="quiet" onClick=${onDismiss}>${T.no}</button>
              </span>`
            : html`<button class="primary pick" onClick=${() => onPick(version)}>${verb}</button>`}
        </div>`)}
    </div>`;
}

function Detail({ state, actions }) {
  const item = state.detail;
  if (!item) return null;
  const series = state.detailSeries;
  const chosen = (state.versions || []).find((version) => version.chosen);
  const have = state.have;
  const thing = series ? T.thisSeason : T.thisFilm;
  const disk = state.progress;
  const episodes = series ? (state.seasonEpisodes || []) : [];
  const oneSeason = new Set(episodes.map((episode) => episode.season)).size === 1;
  const named = oneSeason ? episodes.filter((episode) => episode.title) : [];
  const coming = comingDown(state);
  const failed = coming?.status === 'failed';
  const shown = coming?.status === 'retrying' && state.problem ? 'waiting' : coming?.status;

  const facts = have
    ? html`<p class="chosen" id="already">${T.alreadyHave(thing)}</p>`
    : coming
    ? html`<${Progress} coming=${coming} shown=${shown} failed=${failed} id="detail-status" />`
    : state.downloadingId
    ? html`<p class="chosen" id="already-downloading">${T.alreadyDownloading}</p>`
    : html`<${Fragment}>
        ${chosen && html`
          <div class="fact-row">
            <p class="room" id="room">
              ${[T.occupies(chosen.size), timeWords(chosen.minutes)].filter(Boolean).join(' · ')}
            </p>
            ${disk.total_bytes > 0 && html`
              <p class="room-disk" id="room-disk">
                <${Meter} free=${disk.free_bytes} total=${disk.total_bytes}
                          slice=${chosen.size_bytes} low=${chosen.room !== 'fits'} />
                <span>${T.freeLeft(disk.free_space)}</span>
              </p>`}
          </div>`}
        ${chosen && chosen.room !== 'fits' && html`
          <p class="room bad" id="room-warning">
            ${chosen.room === 'no' ? T.notEnoughRoom : T.mayNotFit}:
            ${T.needsWhilePreparing(chosen.needs)}
          </p>`}
        ${chosen && html`
          <p class="chosen" id="what-comes">
            ${T.willArriveIn(`${midSentence(chosen.quality)}, ${midSentence(chosen.language)}`)}
          </p>`}
      <//>`;

  const buttons = have
    ? html`<${Fragment}>
        ${!series && html`
          <button class="primary" onClick=${() => invoke('play', { id: have }).catch(actions.tell('notice'))}>
            ${T.watchFilm}
          </button>`}
        ${series && html`
          <button class="primary" onClick=${() => actions.openOwned(have)}>
            ${T.watchEpisodes}
          </button>`}
      <//>`
    : coming
    ? html`<${DownloadActions} coming=${coming} failed=${failed} actions=${actions} />`
    : state.downloadingId
    ? html`
      <button class="primary" id="watch-download"
              onClick=${() => actions.watchDownload(state.downloadingId)}>
        ${T.watchProgress}
      </button>`
    : html`
      <button class="primary" id="download" onClick=${() => actions.download()}>
        ${T.download}
      </button>`;

  return html`
    <${FilmPage}
      screenId="screen-detail" titleId="detail-title" synopsisId="synopsis" bandId="detail-band"
      cover=${item.cover_url} title=${series ? `${item.show} · ${item.label}` : item.title}
      factline=${series ? T.completeSeason
                        : [item.year, item.about].filter(Boolean).join(' · ')}
      synopsis=${state.synopsis} bad=${failed} facts=${facts} actions=${buttons}
      copies=${copiesProps(state, actions, state.versions, have)}>
      ${series && !have && html`<p class="factline">
        ${episodes.length > 0 ? T.episodeCount(episodes.length) : T.severalEpisodes}
        ${T.afterDownload}</p>`}
      ${named.length > 0 && html`
        <ol class="episode-names" id="episode-names">
          ${named.map((episode) => html`
            <li key=${`${episode.season}-${episode.number}`}>
              <span class="which">${episode.number}</span> ${episode.title}
            </li>`)}
        </ol>`}
      ${item.imdb && html`
        <button class="quiet" onClick=${() => invoke(series ? 'open_imdb_season' : 'open_imdb', { index: item.index }).catch(actions.tell('notice'))}>
          ${series ? T.imdbEpisodes : T.imdbPage}
        </button>`}
      <${Notice} notice=${state.notice} />
    <//>`;
}

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
        ${T.statusWords[shown] || T.working}
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
          ${T.tryMore(coming.untried)}
        </button>`}
      ${!failed && coming.id && html`
        <button class="quiet" onClick=${() => actions.cancel(coming.id)}>
          ${T.cancelDownload}
        </button>`}
    <//>`;
}

function copiesProps(state, actions, versions, owned) {
  const list = versions || [];
  if (!list.length) return null;
  return {
    versions: state.showVersions ? list : [],
    open: state.showVersions,
    onToggle: actions.toggleVersions,
    verb: owned ? T.swapToThisCopy : T.downloadThisCopy,
    onPick: (version) => (owned
      ? actions.confirmSwap(version.index)
      : actions.download(version.index)),
    confirming: state.confirmSwap,
    onConfirm: (version) => actions.download(version.index, owned),
    onDismiss: () => actions.confirmSwap(null),
    more: state.showVersions ? T.hideCopies : T.otherCopiesCount(list.length),
  };
}

function Now({ state, actions }) {
  const film = state.watching;
  if (!film) {
    return html`
      <section class="screen" id="screen-film">
        <div class="empty now-empty" id="now-empty">
          <p>${T.nothingDownloading}</p>
          <button class="primary" onClick=${() => actions.show('search')}>${T.searchSomething}</button>
        </div>
      </section>`;
  }

  const settled = film.status === 'done';
  const failed = film.status === 'failed';
  const paused = film.status === 'paused';
  const shown = film.status === 'retrying' && state.problem ? 'waiting' : film.status;
  const others = state.progress.active.filter((active) => active.id !== film.id);
  const beneath = [film.detail || '', film.beneath, film.speed].filter(Boolean).join(' · ');
  return html`
    <section class="screen split" id="screen-film">
      <div class="scroll">
        ${others.length > 0 && html`
          <div class="also" id="also-downloading">
            <span>${others.length === 1 ? T.alsoDownloadingOne : T.alsoDownloadingMany}</span>
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
          ${T.statusWords[shown] || T.working}
          ${film.status === 'downloading' && ` ${Math.round(film.percent || 0)} %`}
        </p>
        <div class="bar"><i style=${`width: ${settled ? 100 : film.percent || 0}%`}></i></div>
        <p class="beneath">${paused && state.progress.free_bytes > 0
          && state.progress.free_bytes < NEARLY_FULL
          ? T.diskAlmostFull(state.progress.free_space)
          : beneath}</p>

        <div class="actions">
          ${settled && html`
            <${Fragment}>
              ${!film.series && html`
                <button class="primary" onClick=${() => invoke('play', { id: film.id }).catch(actions.tell('notice'))}>
                  ${T.watchFilm}
                </button>`}
              ${film.series && html`
                <button class="primary" onClick=${() => actions.openOwned(film.id)}>
                  ${T.watchEpisodes}
                </button>`}
              <button class="quiet" onClick=${() => actions.show('search')}>${T.searchAnother}</button>
            <//>`}
          ${failed && html`
            <${Fragment}>
              ${film.untried > 0 && html`
                <button class="primary" id="try-more" onClick=${() => actions.tryMore(film.id)}>
                  ${T.tryMore(film.untried)}
                </button>`}
              <button class=${film.untried > 0 ? 'quiet' : 'primary'}
                      onClick=${() => actions.show('search')}>${T.searchAnother}</button>
            <//>`}
          ${!settled && !failed && html`
            <button class="quiet" onClick=${() => actions.cancel(film.id)}>
              ${T.cancelDownload}
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
      <h1>${T.myFilms}</h1>
      <p class="lead">${T.myFilmsLead}</p>
      ${state.progress.total_bytes > 0 && html`
        <p class="room-disk" id="shelf-disk">
          <${Meter} free=${state.progress.free_bytes} total=${state.progress.total_bytes}
                    low=${lowOnSpace(state.progress)} />
          <span>${T.freeLeft(state.progress.free_space)}</span>
        </p>`}
      <${Notice} notice=${state.shelfNotice} />
      <div class="films" id="shelf">
        ${films.length
          ? films.map((film) => html`
              <${Card} key=${film.id}
                title=${film.title}
                cover=${film.cover_url}
                lines=${[film.series ? T.series : film.year]}
                onOpen=${() => actions.openOwned(film.id)} />`)
          : html`<div class="empty">${T.emptyShelf}</div>`}
      </div>
    </section>`;
}

function subtitleState(film) {
  if (film.subtitle_note) return film.subtitle_note;
  const found = (film.languages?.subtitle_languages || []).some(inSpanish);
  return found ? T.subtitlesReady : T.subtitlesMissing;
}

function hasSpanishSubtitles(film) {
  if (film.subtitle_note) return !T.subtitleNoteReadsBad.test(film.subtitle_note);
  return (film.languages?.subtitle_languages || []).some(inSpanish);
}

function Subtitles({ said, ok, working, onFind }) {
  return html`
    <p class="subtitles ${ok ? 'ok' : 'bad'}" id="subtitle-state">
      <span class="what">${said}</span>
      <button class="find" id="find-subtitles" disabled=${Boolean(working)}
              title=${T.findSubtitlesAgain} onClick=${onFind}>
        <span aria-hidden="true">⟳</span> ${working ? T.findingShort : T.findAgain}
      </button>
    </p>`;
}

function Owned({ state, actions }) {
  const film = (state.progress.shelf || []).find((item) => item.id === state.ownedId);
  if (!film) {
    return html`
      <section class="screen" id="screen-owned">
        <div class="empty" id="owned-gone">${T.ownedGone}</div>
      </section>`;
  }
  const episodes = state.ownedEpisodes || [];
  const first = episodes[0];
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
          <span aria-hidden="true">▶</span> ${T.watchFilm}
        </button>`}
      ${film.series && first && html`
        <button class="primary play" id="play-first" onClick=${() => actions.openEpisode(0)}>
          <span aria-hidden="true">▶</span> ${first.number
            ? T.startWithEpisode(first.number)
            : T.startWithFirst}
        </button>`}
      <button class="quiet" onClick=${() => actions.reveal(film)}>${T.openFolder}</button>
      ${state.confirmRemove === film.id
        ? html`<span class="confirm">
            <span class="word">${T.areYouSure}</span>
            <button class="quiet bad" onClick=${() => actions.remove(film)}>${T.yesDelete}</button>
            <button class="quiet" onClick=${() => actions.confirmRemove(null)}>${T.no}</button>
          </span>`
        : html`<button class="quiet" id="remove"
                       onClick=${() => actions.confirmRemove(film.id)}>${T.deleteWord}</button>`}
    <//>`;

  return html`
    <${FilmPage}
      screenId="screen-owned" titleId="owned-title" synopsisId="owned-synopsis"
      bandId="owned-band"
      cover=${film.cover_url} title=${film.title}
      factline=${[film.series ? T.completeSeason : film.year, spokenIn(film)]
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
              ${!episode.subtitles && html`<span class="without">${T.withoutSubtitles}</span>`}
              <span class="go" aria-hidden="true">→</span>
            </button>`)}
          ${state.ownedEpisodes === null && html`
            <p class="factline" id="episodes-waiting">${T.episodesWaiting}</p>`}
          ${episodes.length === 0 && state.ownedEpisodes !== null && html`
            <p class="factline">${T.emptySeasonFolder}</p>`}
        </div>`}
      <${Notice} notice=${state.shelfNotice} />
    <//>`;
}

function ownedCopies(state, actions) {
  const copies = state.copies;
  return {
    versions: copies?.versions || [],
    open: state.showCopies,
    onToggle: actions.toggleCopies,
    verb: T.swapToThisCopy,
    onPick: (version) => actions.confirmSwap(version.index),
    confirming: state.confirmSwap,
    onConfirm: (version) => actions.swapCopy(version.index),
    onDismiss: () => actions.confirmSwap(null),
    loading: state.loadingCopies,
    problem: state.copiesProblem,
    more: state.showCopies ? T.hideCopies : T.otherCopies,
  };
}

function EpisodeScreen({ state, actions }) {
  const film = (state.progress.shelf || []).find((item) => item.id === state.ownedId);
  const episode = (state.ownedEpisodes || [])[state.episodeAt];
  if (!film || !episode) {
    return html`
      <section class="screen" id="screen-episode">
        <div class="empty" id="episode-gone">${T.episodeGone}</div>
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
            said=${episode.subtitles ? T.subtitlesReady : T.episodeNoSubtitles}
            ok=${episode.subtitles}
            working=${state.findingSubtitles}
            onFind=${() => actions.refetchSubtitles(film)} />
        </div>
        <div class="actions">
          <button class="primary play" id="play-episode"
                  onClick=${() => actions.playEpisode(film, state.episodeAt)}>
            <span aria-hidden="true">▶</span> ${T.watchThisEpisode}
          </button>
        </div>
      </div>
    </section>`;
}

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
      <h1>${T.settingsTitle}</h1>
      <p class="lead">${T.settingsLead}</p>

      <h2>${T.whereFilmsGo}</h2>
      <label>${T.folder}
        <span class="folder">
          <input readonly placeholder=${T.unchosen} value=${settings.destination ?? ''} />
          <button type="button" onClick=${actions.chooseFolder}>${T.choose}</button>
        </span>
      </label>

      <h2>${T.inWhatLanguage}</h2>
      <div class="chips" id="language">
        ${[['any', T.languageAny], ['es', T.languageSpanish], ['original', T.languageOriginal]]
          .map(([code, label]) => html`
            <button type="button" class="chip ${settings.language === code ? 'on' : ''}"
                    data-lang=${code}
                    onClick=${() => actions.setSetting('language', code)}>${label}</button>`)}
      </div>

      <h2>${T.appLanguage}</h2>
      <div class="chips" id="ui-language">
        ${[['', T.appLanguageAuto], ['es', 'Español'], ['en', 'English']]
          .map(([code, label]) => html`
            <button type="button"
                    class="chip ${(settings.ui_language ?? '') === code ? 'on' : ''}"
                    data-ui-lang=${code}
                    onClick=${() => actions.setUiLanguage(code)}>${label}</button>`)}
      </div>

      <h2>${T.startAndClose}</h2>
      <label class="switch">
        <input type="checkbox" name="autostart" checked=${settings.autostart === true}
               onChange=${(event) => actions.setSetting('autostart', event.target.checked)} />
        ${T.autostartLabel}
      </label>
      <label class="switch">
        <input type="checkbox" name="keep_running" checked=${settings.keep_running !== false}
               onChange=${(event) => actions.setSetting('keep_running', event.target.checked)} />
        ${T.keepRunningLabel}
      </label>
      <p class="hint">${T.trayHint}</p>

      <details class="technical" id="technical">
        <summary>${T.technical}</summary>

        <h2>${T.whereToSearch}</h2>
        <p class="hint">${T.indexersHint}</p>
        <div id="indexers">
          ${(settings.indexers || []).map((indexer, position) => html`
            <div class="indexer" key=${position}>
              <label>${T.nameLabel}
                <input value=${indexer.name} placeholder="NZBGeek"
                       onInput=${(event) => actions.setIndexer(position, 'name', event.target.value)} /></label>
              <label>${T.addressLabel}
                <input value=${indexer.url} placeholder="https://api.nzbgeek.info"
                       onInput=${(event) => actions.setIndexer(position, 'url', event.target.value)} /></label>
              <label>${T.keyLabel}
                <input type="password" value=${indexer.key}
                       onInput=${(event) => actions.setIndexer(position, 'key', event.target.value)} /></label>
              <div class="indexer-actions">
                <label class="switch">
                  <input type="checkbox" checked=${indexer.enabled}
                         onChange=${(event) => actions.setIndexer(position, 'enabled', event.target.checked)} />
                  ${T.inUse}
                </label>
                <button type="button" class="link" onClick=${() => actions.removeIndexer(position)}>
                  ${T.removeIndexer}
                </button>
              </div>
            </div>`)}
        </div>
        <button type="button" class="quiet" id="add-indexer" onClick=${actions.addIndexer}>
          ${T.addIndexer}
        </button>

        <h2>${T.newsServer}</h2>
        ${field('news_host', T.serverLabel)}
        <div class="pair">
          ${field('news_port', T.portLabel, { type: 'number' })}
          ${field('news_connections', T.connectionsLabel, { type: 'number' })}
        </div>
        ${field('news_user', T.userLabel)}
        ${field('news_password', T.passwordLabel, { type: 'password', placeholder: T.unchangedPlaceholder })}
        <label class="switch">
          <input type="checkbox" name="news_encrypted" checked=${settings.news_encrypted !== false}
                 onChange=${(event) => actions.setSetting('news_encrypted', event.target.checked)} />
          ${T.encryptedLabel}
        </label>

        <h2>${T.filmPages}</h2>
        <p class="hint">${T.tmdbHint}</p>
        ${field('tmdb_key', T.tmdbKeyLabel, { type: 'password' })}

        <h2>${T.subtitlesHeading}</h2>
        ${field('subtitles_key', T.keyLabel, { type: 'password' })}
        ${field('subtitles_agent', T.agentLabel)}
        ${field('subtitles_user', T.userLabel, { placeholder: T.userNotEmail })}
        ${field('subtitles_password', T.passwordLabel, { type: 'password', placeholder: T.unchangedPlaceholder })}

        <h2>${T.settingsFileHeading}</h2>
        <p class="hint">${T.settingsFileHint}</p>
        <p class="path">${settings.settings_path}</p>
        <button type="button" class="quiet" id="open-settings-file"
                onClick=${actions.openSettingsFile}>
          ${T.openSettingsFile}
        </button>

        <h2>${T.logHeading}</h2>
        <p class="hint">${T.logHint}</p>
        <p class="path">${settings.log_path}</p>
        <div class="two-buttons">
          <button type="button" class="quiet" id="open-log-file" onClick=${actions.openLogFile}>
            ${T.openLog}
          </button>
          <button type="button" class="quiet" id="open-log-folder"
                  onClick=${actions.openLogFolder}>
            ${T.openLogFolder}
          </button>
        </div>
      </details>

      <div class="actions">
        <button class="primary" onClick=${actions.saveSettings}>${T.save}</button>
        <button class="quiet" onClick=${actions.checkSettings}>${T.check}</button>
      </div>
      <${Notice} notice=${state.settingsNotice} />
      <p class="path" id="app-version">Mamá Cine ${settings.version}</p>
      </div>
    </section>`;
}

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
    copies: null,
    showCopies: false,
    loadingCopies: false,
    copiesProblem: null,
    confirmSwap: null,
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
    lang: initialLang,
    updating: false,
    progress: { active: [], finished: [], shelf: [], free_space: '', free_bytes: 0 },
  });

  currentLang = STRINGS[state.lang] ? state.lang : 'es';
  T = STRINGS[currentLang];
  document.documentElement.lang = currentLang;

  const change = (fields) => setState((current) => ({ ...current, ...fields }));
  const edit = (make) => setState((current) => ({ ...current, ...make(current) }));
  const latest = useRef(state);
  latest.current = state;
  const suggestDelay = useRef(null);
  const suggestGeneration = useRef(0);

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
        lang: settings.app_language || initialLang,
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
        let landed = progress.finished.find((film) => film.id === watching.id);
        while (landed?.next_id) {
          if (downloadingId === watching.id) downloadingId = landed.next_id;
          watching = { ...watching, id: landed.next_id, status: 'retrying', detail: '', percent: 0 };
          landed = progress.finished.find((film) => film.id === landed.next_id);
        }
        const running = progress.active.find((film) => film.id === watching.id);
        if (running) {
          watching = { ...watching, detail: '', ...running };
        } else if (landed) {
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
          const chasing = progress.finished.find((film) => film.retrying && !film.next_id);
          if (chasing) watching = { ...chasing, status: 'retrying', percent: 0 };
        }
      }
      if (watching?.status === 'done') {
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
      suggestGeneration.current += 1;
      if (suggestDelay.current) clearTimeout(suggestDelay.current);
      change({ suggestions: [] });
    },

    show: (screen) => {
      actions.hideSuggestions();
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

    typed: (query) => {
      change({ query });
      if (suggestDelay.current) clearTimeout(suggestDelay.current);
      suggestGeneration.current += 1;
      const generation = suggestGeneration.current;
      const enoughToSuggestOn = 3;
      if (query.trim().length < enoughToSuggestOn) {
        change({ suggestions: [] });
        return;
      }
      const doneTyping = 300;
      suggestDelay.current = setTimeout(async () => {
        try {
          const suggestions = await invoke('suggest', { text: query });
          if (generation === suggestGeneration.current) change({ suggestions });
        } catch (error) {
          if (generation === suggestGeneration.current) change({ suggestions: [] });
        }
      }, doneTyping);
    },

    pickSuggestion: async (position) => {
      const title = latest.current.suggestions[position];
      if (!title) return;
      actions.hideSuggestions();
      change({ query: title.title, searching: true });
      try {
        const picked = await invoke('pick_suggestion', { index: position });
        await actions.runSearch(picked.query, picked.series ? 'series' : 'film', picked.title);
      } catch (error) {
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

    openDetail: async (item, series) => {
      change({
        detail: item, detailSeries: series, have: null, downloadingId: null,
        versions: null, showVersions: false, synopsis: '', seasonEpisodes: [],
        copies: null, showCopies: false, copiesProblem: null, confirmSwap: null,
        starting: false, screen: 'detail', notice: null,
      });
      if (!series) {
        invoke('synopsis', { index: item.index })
          .then((synopsis) => edit((state) => (
            state.detail === item ? { synopsis: synopsis || '' } : {})))
          .catch(() => {});
      }
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
      try {
        const owned = await invoke('have', { index: item.index, series });
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
      change({
        watching, notice: null, starting: true, confirmSwap: null, showVersions: false,
      });
      try {
        const grabbed = await invoke('grab', {
          index: detail.index, version, series: detailSeries, replacing,
        });
        if (grabbed.already) {
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

    tryMore: async (id) => {
      try {
        const grabbed = await invoke('try_more', { id });
        change({
          watching: {
            ...latest.current.watching,
            id: grabbed.id, status: 'retrying', detail: '', percent: 0,
          },
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

    tell: (field) => (error) => change({ [field]: { text: String(error), bad: true } }),

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
          shelfNotice: { text: T.sentToBin(film.title) },
        });
      } catch (error) {
        change({ shelfNotice: { text: String(error), bad: true } });
      }
    },

    refetchSubtitles: async (film) => {
      change({ findingSubtitles: true, shelfNotice: { text: T.findingSubtitles } });
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

    setUiLanguage: (code) => edit((state) => ({
      settings: { ...state.settings, ui_language: code },
      lang: STRINGS[code] ? code : state.lang,
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

    openLogFile: async () => {
      try {
        await invoke('open_log_file');
      } catch (error) {
        change({ notice: { text: String(error), bad: true },
                 settingsNotice: { text: String(error), bad: true } });
      }
    },

    installUpdate: async () => {
      change({ updating: true });
      try {
        await invoke('open_update');
      } catch (error) {
        change({ notice: { text: String(error), bad: true } });
      } finally {
        change({ updating: false });
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
      change({ settingsNotice: { text: T.saving } });
      try {
        const saved = await invoke('save_settings', { incoming: latest.current.settings });
        change({
          settings: withoutPasswords(saved),
          lang: saved.app_language || latest.current.lang,
          settingsNotice: {
            text: saved.ready ? T.savedReady : T.savedMissing,
          },
        });
      } catch (error) {
        change({ settingsNotice: { text: String(error), bad: true } });
      }
    },

    checkSettings: async () => {
      change({ settingsNotice: { text: T.checking } });
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

  return html`
    <header>
      <button class="brand" title=${T.backHome} onClick=${() => actions.show('search')}>
        <img class="mark" src="icon.svg" alt="" />
        <span>Mamá Cine</span>
      </button>
      <nav>
        ${BACK_TO[state.screen] && html`
          <button class="back" onClick=${() => actions.show(BACK_TO[state.screen])}>
            <span aria-hidden="true">←</span> ${T.back}</button>`}
        ${[['search', T.navSearch], ['library', T.navLibrary]].map(([screen, label]) => html`
          <button key=${screen} data-screen=${screen}
                  class=${state.screen === screen ? 'on' : ''}
                  onClick=${() => actions.show(screen)}>${label}</button>`)}
        <span class="grow"></span>
        ${following && html`
          <${Pill} film=${following} others=${otherDownloads}
                   on=${state.screen === 'film'}
                   onOpen=${() => actions.show('film')} />`}
        <button class=${`gear ${state.screen === 'settings' ? 'on' : ''}`} data-screen="settings"
                title=${T.navSettings} aria-label=${T.navSettings}
                onClick=${() => actions.show('settings')}>⚙</button>
      </nav>
    </header>
    ${lowOnSpace(state.progress) && html`<div id="space" class="space">
      ${T.lowSpace(state.progress.free_space)}
    </div>`}
    ${state.progress.update && html`<div id="update" class="space">
      ${state.progress.update.installed
        ? T.updateInstalled(state.progress.update.version)
        : html`<${Fragment}>
            ${T.updateAvailable(state.progress.update.version)}
            <button type="button" class="link" id="install-update" disabled=${state.updating}
                    onClick=${actions.installUpdate}>
              ${state.updating ? T.updating : T.installUpdate}
            </button>
          <//>`}
    </div>`}
    <${Problem} problem=${state.problem} actions=${actions} />
    <main>
      <${Screen} state=${state} actions=${actions} />
    </main>`;
}

render(html`<${App} />`, document.getElementById('app'));
