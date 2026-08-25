
const POSTER = 'data:image/svg+xml;utf8,' + encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="200" height="300">
     <rect width="200" height="300" fill="#232833"/>
     <circle cx="100" cy="120" r="42" fill="#7cb0ff" opacity="0.35"/>
     <rect x="40" y="196" width="120" height="10" rx="5" fill="#7cb0ff" opacity="0.3"/>
     <rect x="60" y="216" width="80" height="10" rx="5" fill="#7cb0ff" opacity="0.2"/>
   </svg>`);

const films = [
  { index: 0, title: 'Das Boot', year: '1981', quality: 'Alta definición', size: '5.5 GB',
    size_bytes: 5_905_580_032, about: '★8.4 · Drama · 149 min', cover_url: POSTER, room: 'fits' },
  { index: 1, title: 'Persépolis', year: '2007', quality: 'Alta definición', size: '6.0 GB',
    size_bytes: 6_442_450_944, about: '★8.0 · Animación · 96 min', cover_url: POSTER, room: 'fits' },
  { index: 2, title: 'El Sur', year: '1983', quality: 'Buena calidad', size: '3.1 GB',
    size_bytes: 3_328_599_654, about: '★8.0 · Drama · 95 min', cover_url: POSTER, room: 'fits' },
  { index: 3, title: 'El espíritu de la colmena', year: '1973', quality: 'Alta definición',
    size: '4.4 GB', size_bytes: 4_724_464_025, about: '★7.8 · Drama · 97 min', cover_url: null,
    room: 'tight' },
];

const seasons = [
  { index: 0, show: 'Cuéntame cómo pasó', label: 'Temporada 1', size: '8.0 GB',
    size_bytes: 8_589_934_592, quality: 'Alta definición', cover_url: POSTER, grabs: 120,
    room: 'fits', imdb: 'tt0302447' },
];

const seasonEpisodes = [
  'El retorno del fugitivo', 'Un cero a la izquierda', 'Ellas, las mujeres',
  'Los infiltrados', 'Un mundo mejor', 'La primera cita', 'El paseíllo',
  'Hoy no me puedo levantar', 'La cara oculta de la luna', 'El día que Franco tosió',
  'Los amigos de mis amigos', 'Una carta de Alemania', 'El baile de las debutantes',
].map((title, index) => ({ season: 1, number: index + 1, title }));

const story = (steps) => steps.map(([minutes, said, why]) => ({
  at: Math.floor(Date.now() / 1000) - minutes * 60, said, why,
}));

const kept = [
  { id: 101, title: 'Cuéntame cómo pasó · Temporada 1', series: true, cover_url: POSTER,
    year: null, subtitle_note: 'Faltan los subtítulos del episodio 3',
    languages: { audio_languages: ['spa'], subtitle_languages: [] } },
  { id: 102, title: 'El espíritu de la colmena', series: false, cover_url: null, year: '1973',
    subtitle_note: 'Subtítulos en español listos',
    languages: { audio_languages: ['spa'], subtitle_languages: ['spa'] } },
  { id: 103, title: 'The Red Turtle', series: false, cover_url: POSTER, year: '2016',
    subtitle_note: 'No hay subtítulos en español para esta versión',
    languages: { audio_languages: ['und'], subtitle_languages: ['eng'] } },
];

let started = null;
let ticks = 0;

window.__TAURI__ = {
  core: {
    invoke: async (command, args) => {
      await new Promise((resume) => setTimeout(resume, 120));
      switch (command) {
        case 'search':
          return { films, seasons, notice: null };
        case 'suggest':
          return [
            { id: '0082096', title: 'El submarino', original: 'Das Boot', year: '1981',
              series: false, poster_url: POSTER },
            { id: '0106004', title: 'Das Boot (serie)', original: null, year: '2018',
              series: true, poster_url: POSTER },
          ];
        case 'pick_suggestion':
          return { query: 'tt0082096', series: false, title: 'El submarino (Das Boot)' };
        case 'have':
          return { have: null, downloading: null };
        case 'synopsis':
          return 'Un submarino alemán patrulla el Atlántico en 1941. La guerra, vista desde '
            + 'dentro de un tubo de acero: el tedio, el miedo y la camaradería de una '
            + 'tripulación que ya no cree en ella.';
        case 'versions':
          return [
            { index: 0, quality: 'Alta definición (1080p)', size: '1,7 GB',
              size_bytes: 1_825_361_100, grabs: 528, language: 'Versión original', chosen: true,
              name: 'a', room: 'fits', needs: '3,7 GB', minutes: 12 },
            { index: 1, quality: 'Alta definición (1080p)', size: '5,5 GB',
              size_bytes: 5_905_580_032, grabs: 1305, language: 'Español', chosen: false,
              name: 'b', room: 'tight', needs: '12,1 GB', minutes: 40 },
          ];
        case 'grab':
          started = { id: 1, ...(args.series ? seasons[args.index] : films[args.index]) };
          ticks = 0;
          return { id: 1, already: false };
        case 'progress': {
          const base = { shelf: kept, free_space: '412 GB', free_bytes: 442_000_000_000,
                         total_space: '953 GB', total_bytes: 1_023_000_000_000, problem: null };
          if (!started) return { active: [], finished: [], ...base };
          ticks += 1;
          if (ticks < 14) {
            return {
              ...base,
              active: [{
                id: 1, title: started.title || started.show, year: started.year, cover_url: POSTER,
                status: ticks < 10 ? 'downloading' : 'unpacking',
                percent: Math.min(100, ticks * 8),
                beneath: 'Unos 4 minutos', speed: '25 MB/s', series: Boolean(started.show),
                story: story([
                  [9, 'Empieza la descarga.', 'copia 1 de 3'],
                  [5, 'Esa descarga venía dañada, así que la he descartado.',
                    'FAILURE/HEALTH: faltaban 71 de 6257 partes'],
                  [4, 'Empieza la descarga de otra versión.', 'copia 2 de 3'],
                ]),
              }],
              finished: [],
            };
          }
          const landed = {
            id: 1, title: started.title || started.show, year: started.year, ok: true, detail: '',
            retrying: false, cover_url: POSTER,
            subtitle_note: 'Subtítulos en español añadidos (3)',
            languages: { audio_languages: ['ger'], subtitle_languages: ['eng'] },
            next_id: null, attempt: 2, attempts_total: 3, untried: 0,
            series: Boolean(started.show),
            story: story([[1, 'Ya está lista para ver.', 'SUCCESS/UNPACK']]),
          };
          return { ...base, active: [], finished: [landed], shelf: [landed, ...kept] };
        }
        case 'read_settings':
          return { ready: true, indexers: [{ name: 'NZBGeek', url: 'https://api.nzbgeek.info',
                   key: 'una-clave', enabled: true }], news_port: 563, news_host: 'news.eweka.nl',
                   news_connections: 8, news_encrypted: true, subtitles_agent: 'mamacine v1.0',
                   language: 'any', destination: 'C:\\Películas', autostart: false,
                   keep_running: true, ui_language: '', app_language: 'es', version: '0.4.0',
                   settings_path: 'C:\\Users\\mama\\AppData\\Roaming\\mamacine\\settings.json',
                   log_path: 'C:\\Users\\mama\\AppData\\Roaming\\mamacine\\mamacine.log' };
        case 'open_settings_file':
        case 'open_log_file':
        case 'open_log_folder':
          return null;
        case 'save_settings':
          return { ...args.incoming, app_language: args.incoming.ui_language || 'es', ready: true };
        case 'check_settings':
          return 'NZBGeek: funciona.\nServidor de descargas: funciona.\nSubtítulos: funciona.';
        case 'episodes':
          return seasonEpisodes.slice(0, 6).map((episode, position) => ({
            label: `Episodio ${episode.number}`,
            subtitles: position !== 2,
            season: episode.season,
            number: episode.number,
            title: episode.title,
            overview: 'Los Alcántara, en el barrio, el año que la televisión llegó a casa. '
              + 'Antonio pluriemplea, Mercedes cose y Carlitos lo cuenta todo.',
          }));
        case 'season_episodes':
          return seasonEpisodes;
        case 'try_more':
          return { id: 2, already: false };
        case 'fetch_subtitles':
          return 'Subtítulos en español añadidos (2)';
        case 'library_synopsis':
          return 'Una familia de un barrio de Madrid, contada desde dentro, el año en que la '
            + 'televisión entró en las casas.';
        case 'cover':
          return args.url;
        default:
          return null;
      }
    },
  },
};
