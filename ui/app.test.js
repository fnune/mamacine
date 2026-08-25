
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { JSDOM } from 'jsdom';

const FILMS = [
  { index: 0, title: 'El Sur', year: '1983', about: '★8.0', quality: 'Alta definición',
    size: '3.1 GB', size_bytes: 3_328_599_654, cover_url: null, imdb: '0086010', room: 'fits',
    relevance: 3.0 },
  { index: 1, title: 'Volver', year: '2006', about: '★7.6', quality: 'Alta definición',
    size: '2.2 GB', size_bytes: 2_362_232_012, cover_url: null, imdb: null, room: 'fits',
    relevance: 0.5 },
];

const SEASONS = [
  { index: 0, show: 'Cuéntame', label: 'Temporada 1', size: '8.0 GB',
    size_bytes: 8_589_934_592, quality: 'Alta definición', cover_url: null, room: 'fits',
    relevance: 2.0 },
];

const VERSIONS = [
  { index: 0, quality: 'Alta definición (1080p)', size: '1,7 GB', size_bytes: 1_825_361_100,
    grabs: 528, language: 'Versión original', chosen: true, name: 'a', room: 'fits',
    needs: '3,7 GB', minutes: 12 },
  { index: 1, quality: 'Alta definición (1080p)', size: '5,5 GB', size_bytes: 5_905_580_032,
    grabs: 1305, language: 'Versión original', chosen: false, name: 'b', room: 'fits',
    needs: '12,1 GB', minutes: 40 },
];

const SETTINGS = {
  ready: true,
  indexers: [{ name: 'NZBGeek', url: 'https://api.nzbgeek.info', key: 'una-clave', enabled: true }],
  news_host: 'news.eweka.nl', news_port: 563, news_user: 'reader', news_password_set: true,
  news_connections: 8, news_encrypted: true, subtitles_key: 'otra-clave',
  subtitles_agent: 'mamacine v1.0', subtitles_user: 'fnune', subtitles_password_set: true,
  destination: '/home/fausto/Descargas', language: 'any', tmdb_key: '',
  ui_language: '', app_language: 'es', version: '0.4.0',
  autostart: false, keep_running: true,
  settings_path: '/home/fausto/.config/mamacine/settings.json',
  log_path: '/home/fausto/.local/share/mamacine/mamacine.log',
};

function start({
  films = FILMS, seasons = SEASONS, finished = [], active = [], shelf = [],
  have = null, downloading = null, grabbed = { id: 7, already: false }, settings = SETTINGS,
  copies = { index: 0, series: false, versions: VERSIONS },
  fail = null, holdSuggest = false, update = null,
  versions = VERSIONS, suggestions = [], free_bytes = 442_000_000_000, free_space = '412 GB',
  problem = null, searchNotice = null, searchExact = null, synopsis = '', seasonEpisodes = [],
  episodeRows = [
    { label: 'Episodio 1', subtitles: true },
    { label: 'Episodio 2', subtitles: true },
    { label: 'Episodio 3', subtitles: true },
  ],
} = {}) {
  const calls = [];
  let releaseSuggest = null;
  const live = { active, finished, shelf, free_space, free_bytes, problem };
  const invoke = async (command, args) => {
    calls.push({ command, args });
    if (fail && fail === command) throw new Error('el buscador no responde');
    switch (command) {
      case 'search':
        return { films, seasons, notice: searchNotice,
                 exact: searchExact ?? Boolean(args.kind) };
      case 'suggest':
        if (holdSuggest) {
          return new Promise((resolve) => { releaseSuggest = () => resolve(suggestions); });
        }
        return suggestions;
      case 'pick_suggestion': {
        const picked = suggestions[args.index];
        return { query: picked.series ? picked.title : `tt${picked.id}`,
                 series: picked.series, title: picked.title };
      }
      case 'have': return { have, downloading };
      case 'versions': return versions;
      case 'copies': return copies;
      case 'grab': return grabbed;
      case 'progress':
        return { ...live, update, total_space: '953 GB', total_bytes: 1_023_000_000_000 };
      case 'synopsis': return synopsis;
      case 'library_synopsis': return synopsis;
      case 'read_settings': return settings;
      case 'save_settings': return { ...args.incoming, app_language: 'es', ready: true };
      case 'check_settings': return 'NZBGeek: funciona.\nServidor de descargas: funciona.';
      case 'choose_folder': return '/home/fausto/Vídeos';
      case 'cover': return 'data:image/jpeg;base64,AAA';
      case 'episodes': return episodeRows;
      case 'season_episodes': return seasonEpisodes;
      case 'fetch_subtitles': return 'Subtítulos en español añadidos (2)';
      case 'try_more': return { id: 99, already: false };
      default: return null;
    }
  };

  const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  const dom = new JSDOM(html, { url: 'http://localhost/', runScripts: 'outside-only' });
  dom.window.__TAURI__ = { core: { invoke } };
  dom.window.requestAnimationFrame = (draw) => { draw(); return 0; };
  dom.window.cancelAnimationFrame = () => {};
  const beats = [];
  dom.window.setInterval = (beat) => beats.push(beat);
  dom.window.clearInterval = () => {};
  dom.window.setTimeout = (fn) => { fn(); return 0; };
  dom.window.clearTimeout = () => {};
  dom.window.eval(readFileSync(new URL('./vendor/htm-preact.js', import.meta.url), 'utf8'));
  dom.window.eval(readFileSync(new URL('./app.js', import.meta.url), 'utf8'));
  const poll = async () => {
    for (const beat of beats) await beat();
    await settle();
  };
  return {
    window: dom.window, document: dom.window.document, calls, poll, live,
    releaseSuggest: () => releaseSuggest && releaseSuggest(),
  };
}

const settle = () => new Promise((done) => setTimeout(done, 0));

const click = (app, target) =>
  target.dispatchEvent(new app.window.Event('click', { bubbles: true }));

async function openOwned(app, position = 0) {
  click(app, app.document.querySelector('nav button[data-screen="library"]'));
  await settle();
  click(app, app.document.querySelectorAll('#shelf .film')[position]);
  await settle();
  await settle();
}

async function searchFor(app, text) {
  const field = app.document.getElementById('query');
  field.value = text;
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  app.document.getElementById('search').dispatchEvent(
    new app.window.Event('submit', { bubbles: true, cancelable: true }),
  );
  await settle();
  await settle();
}

test('the page renders as a page', async () => {
  const app = start();
  await settle();

  const root = app.document.getElementById('app');
  assert.deepEqual([...root.children].map((node) => node.tagName), ['HEADER', 'MAIN']);
  assert.ok(app.document.getElementById('screen-search'), 'the search screen is a section');
  assert.equal(app.document.querySelector('#search button[type="submit"]').textContent.trim(), 'Buscar');
  assert.match(app.document.querySelector('header .brand').textContent, /Mamá Cine/,
    'the app is called by its name on screen');
  const tabs = [...app.document.querySelectorAll('nav button')];
  assert.deepEqual(tabs.map((tab) => tab.dataset.screen), ['search', 'library', 'settings'],
    'two places and a gear; the download appears only while there is one');
});

test('no element leaks its own tag name into the page', async () => {
  const app = start();
  await searchFor(app, 'el sur');

  const text = app.document.body.textContent;
  for (const tag of ['section', 'img', 'input', 'footer', 'nav']) {
    assert.ok(!new RegExp(`(^|\\s)${tag}(\\s|$)`).test(text), `the word "${tag}" is on screen`);
  }
});

test('a poster is an image inside its frame', async () => {
  const app = start({ films: [{ ...FILMS[0], cover_url: 'https://indexer.test/a.jpg' }] });
  await searchFor(app, 'el sur');
  await settle();

  const poster = app.document.querySelector('#results .poster');
  assert.ok(poster, 'every card has a poster frame');
  assert.equal(poster.querySelector('img')?.getAttribute('src'), 'data:image/jpeg;base64,AAA');
});

test('one search answers films and seasons together, best match first', async () => {
  const app = start();
  await searchFor(app, 'el sur');

  const names = [...app.document.querySelectorAll('#results .name')].map((node) => node.textContent);
  assert.deepEqual(names, ['El Sur', 'Cuéntame', 'Volver'], 'ordered by how well they match');
  const asked = app.calls.filter((call) => call.command === 'search');
  assert.equal(asked.length, 1, 'one question, not one per kind');
  assert.equal(asked[0].args.kind, null, 'a typed name says nothing about its kind');
});

test('a result card carries the name and the year, not the machinery', async () => {
  const app = start();
  await searchFor(app, 'el sur');

  const card = app.document.querySelector('#results .film');
  assert.match(card.textContent, /El Sur/);
  assert.match(card.textContent, /1983/);
  assert.ok(!/GB/.test(card.textContent), card.textContent);
  assert.ok(!/definición/.test(card.textContent), card.textContent);
});

test('typing offers titles, and picking one searches by its exact identity', async () => {
  const suggestions = [
    { id: '0086010', title: 'El Sur', original: null, year: '1983', series: false,
      poster_url: null },
    { id: '0106004', title: 'El Sur (serie)', original: null, year: '2018', series: true,
      poster_url: null },
  ];
  const app = start({ suggestions });
  await settle();

  const field = app.document.getElementById('query');
  field.value = 'el su';
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  await settle();

  const offered = [...app.document.querySelectorAll('.suggestion .name')]
    .map((node) => node.textContent);
  assert.deepEqual(offered, ['El Sur', 'El Sur (serie)'], 'both kinds are offered');
  assert.match(app.document.querySelectorAll('.suggestion .meta')[1].textContent, /Serie/);

  click(app, app.document.querySelector('.suggestion'));
  await settle();
  await settle();

  const search = app.calls.findLast((call) => call.command === 'search');
  assert.equal(search.args.query, 'tt0086010', 'a film is asked for by its id');
  assert.equal(search.args.kind, 'film');
  assert.equal(search.args.shown, 'El Sur', 'the picked name follows the film');
  assert.equal(app.document.getElementById('query').value, 'El Sur', 'the box shows the name');
  assert.ok(!app.document.querySelector('.suggestion'), 'the popover is gone');
});

test('a suggestion shows the localized title with the original beside it', async () => {
  const suggestions = [
    { id: '432787', title: 'El hoyo', original: 'The Platform', year: '2019', series: false,
      poster_url: null },
    { id: '2380307', title: 'Coco', original: null, year: '2017', series: false,
      poster_url: null },
  ];
  const app = start({ suggestions });
  await settle();
  const field = app.document.getElementById('query');
  field.value = 'el hoyo';
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  await settle();

  const names = [...app.document.querySelectorAll('.suggestion .name')]
    .map((node) => node.textContent);
  assert.equal(names[0], 'El hoyo (The Platform)');
  assert.equal(names[1], 'Coco', 'no parentheses when the original is the same or unknown');
});

test('the suggestions close when she clicks anywhere else, without searching', async () => {
  const suggestions = [
    { id: '0086010', title: 'El Sur', original: null, year: '1983', series: false,
      poster_url: null },
  ];
  const app = start({ suggestions });
  await settle();
  const field = app.document.getElementById('query');
  field.value = 'el sur';
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  await settle();
  assert.ok(app.document.querySelector('.suggestion'), 'the popover is open');

  click(app, app.document.querySelector('h1'));
  await settle();
  assert.ok(!app.document.querySelector('.suggestion'), 'a click elsewhere closes it');
  assert.ok(!app.calls.some((call) => call.command === 'search'), 'and starts nothing');
  assert.equal(app.document.getElementById('query').value, 'el sur', 'her words stay put');
});

test('nothing under the pointer gains or loses a border', () => {
  const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
  const hovers = css.match(/[^{}]*:hover[^{]*\{[^}]*\}/g) || [];
  assert.ok(hovers.length > 0, 'the hover rules are found');
  for (const rule of hovers) {
    assert.ok(!/border/.test(rule), `a hover rule touches a border: ${rule.trim()}`);
  }
  const suggestion = css.match(/\.suggestion:hover[^{]*\{[^}]*\}/s)?.[0] || '';
  assert.match(suggestion, /background/, 'a row is marked by its ground');
});

test('no button is naked', () => {
  const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
  const quiet = css.match(/button\.quiet \{[^}]*\}/s)?.[0] || '';
  assert.match(quiet, /background: color-mix/, 'quiet buttons sit on a ground of their own');
  const link = css.match(/button\.link \{[^}]*\}/s)?.[0] || '';
  assert.match(link, /border: 1px solid/, 'card actions are real buttons');
});

test('good news is never dressed in the failure color', () => {
  const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
  const note = css.match(/\.note \{[^}]*\}/s)?.[0] || '';
  assert.ok(note, 'the notice rule is found');
  assert.ok(!/--accent|--bad/.test(note), `a neutral notice borrows an alarming color: ${note}`);
});

test('the wordmark is the way home', async () => {
  const app = start();
  await settle();
  click(app, app.document.querySelector('nav button[data-screen="library"]'));
  await settle();
  assert.ok(!app.document.getElementById('screen-search'), 'she is elsewhere');
  click(app, app.document.querySelector('header .brand'));
  await settle();
  assert.ok(app.document.getElementById('screen-search'));
});

test('a series suggestion searches by its proper name', async () => {
  const suggestions = [
    { id: '0106004', title: 'Cuéntame cómo pasó', original: null, year: '2001', series: true,
      poster_url: null },
  ];
  const app = start({ suggestions });
  await settle();
  const field = app.document.getElementById('query');
  field.value = 'cuentame';
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  await settle();
  click(app, app.document.querySelector('.suggestion'));
  await settle();

  const search = app.calls.findLast((call) => call.command === 'search');
  assert.equal(search.args.query, 'Cuéntame cómo pasó');
  assert.equal(search.args.kind, 'series',
    'she said it is a series, so films are not even asked for');
});

test('an empty answer to a name that was identified never blames her spelling', async () => {
  const app = start({ films: [], seasons: [], searchExact: true });
  await searchFor(app, 'el castillo ambulante');
  assert.match(app.document.querySelector('#results .empty').textContent,
    /Existe, pero ahora mismo no la encuentro/);
});

test('picking a title leaves her own language in the box', async () => {
  const suggestions = [
    { id: '0347149', title: 'El castillo ambulante', original: null, year: '2004', series: false,
      poster_url: null },
  ];
  const app = start({ suggestions, films: [], seasons: [] });
  await settle();
  const field = app.document.getElementById('query');
  field.value = 'el castillo';
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  await settle();
  click(app, app.document.querySelector('.suggestion'));
  await settle();
  await settle();
  assert.equal(app.document.getElementById('query').value, 'El castillo ambulante');
});

test('nothing is called missing while the suggestions are showing it', async () => {
  const suggestions = [
    { id: '0347149', title: 'El castillo ambulante', original: null, year: '2004', series: false,
      poster_url: null },
  ];
  const app = start({ suggestions, films: [], seasons: [] });
  await searchFor(app, 'el castillo ambulante');
  app.document.getElementById('query').dispatchEvent(
    new app.window.Event('input', { bubbles: true }),
  );
  await settle();
  await settle();
  assert.ok(app.document.querySelector('#suggestions'), 'the list is up');
  assert.equal(app.document.querySelector('#results .empty'), null);
});

test('an empty answer to a picked title never blames her spelling', async () => {
  const suggestions = [
    { id: '0302447', title: 'Cuéntame cómo pasó', original: null, year: '2001', series: true,
      poster_url: null },
  ];
  const app = start({ suggestions, films: [], seasons: [] });
  await settle();
  const field = app.document.getElementById('query');
  field.value = 'cuentame';
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  await settle();
  click(app, app.document.querySelector('.suggestion'));
  await settle();
  await settle();

  const empty = app.document.querySelector('#results .empty');
  assert.match(empty.textContent, /Existe, pero ahora mismo no la encuentro/);
  assert.ok(!/escribirlo de otra manera/.test(empty.textContent), 'her spelling was fine');

  const typed = start({ films: [], seasons: [] });
  await searchFor(typed, 'cuentame commo passo');
  assert.match(typed.document.querySelector('#results .empty').textContent,
    /elige uno de los títulos/);
});

test('a suggestion answer that arrives after she searched is thrown away', async () => {
  const suggestions = [
    { id: '0086010', title: 'El Sur', original: null, year: '1983', series: false,
      poster_url: null },
  ];
  const app = start({ suggestions, holdSuggest: true });
  await settle();
  const field = app.document.getElementById('query');
  field.value = 'el sur';
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
  await settle();
  app.document.getElementById('search').dispatchEvent(
    new app.window.Event('submit', { bubbles: true, cancelable: true }),
  );
  await settle();
  await settle();
  assert.ok(app.document.querySelector('#results .film'), 'her results arrived');

  app.releaseSuggest();
  await settle();
  assert.ok(!app.document.querySelector('.suggestion'),
    'and it never opens the popover on top of her results');
});

test('a fresh window adopts a film that is mid-chase instead of claiming idleness', async () => {
  const finished = [
    { id: 7, title: 'El Sur', ok: false, retrying: true, next_id: null, detail: '',
      subtitle_note: '', cover_url: null, year: '1983', languages: {}, series: false,
      attempt: 1, attempts_total: 3, untried: 0 },
  ];
  const app = start({ finished });
  await app.poll();
  click(app, app.document.querySelector('nav button[data-screen="film"]'));
  await settle();

  assert.ok(!app.document.getElementById('now-empty'), 'not idle');
  assert.equal(app.document.getElementById('now-title').textContent, 'El Sur');
  assert.match(app.document.getElementById('now-status').textContent, /Buscando otra copia/);
});

test('a film already on its way is shown as such, never offered again', async () => {
  const app = start({ downloading: 9 });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();

  assert.ok(!app.document.getElementById('download'), 'no second Descargar');
  assert.match(app.document.getElementById('already-downloading').textContent,
    /Ya se está descargando/);
  click(app, app.document.getElementById('watch-download'));
  await settle();
  assert.equal(app.document.getElementById('screen-film').hidden, false);
  assert.equal(app.document.getElementById('now-title').textContent, 'El Sur');
});

test('opening a card shows the film rather than downloading it', async () => {
  const app = start();
  await searchFor(app, 'el sur');

  click(app, app.document.querySelector('#results .film'));
  await settle();

  assert.equal(app.document.getElementById('detail-title').textContent, 'El Sur');
  assert.ok(!app.calls.some((call) => call.command === 'grab'), 'nothing may start on its own');
  assert.ok(app.document.getElementById('download'), 'downloading is its own button');
});

test('downloading asks for the film that was opened', async () => {
  const app = start();
  await searchFor(app, 'el sur');

  click(app, [...app.document.querySelectorAll('#results .film')]
    .find((node) => /Volver/.test(node.textContent)));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();

  const grab = app.calls.find((call) => call.command === 'grab');
  assert.equal(grab.args.index, 1, 'the card carries the film it was made from');
  assert.equal(grab.args.series, false);
});

test('starting a download leaves her on the ficha, with the copy coming down beneath it', async () => {
  const app = start({ synopsis: 'Una niña y su padre en el norte.' });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();

  const screen = app.document.getElementById('screen-detail');
  assert.ok(screen, 'she is still on the ficha');
  assert.match(app.document.getElementById('synopsis').textContent, /Una niña y su padre/,
    'and what the film is about is still on it');
  assert.ok(app.document.getElementById('detail-status'),
    'the band below says what the copy is doing');
  assert.ok([...screen.querySelectorAll('button')]
    .some((button) => button.textContent.includes('Cancelar')),
  'and offers the only decision left');
});

test('a search that fails says so instead of throwing', async () => {
  const app = start({ fail: 'search' });
  await searchFor(app, 'el sur');

  const notice = app.document.querySelector('#notice .note');
  assert.ok(notice, 'a failure should be shown');
  assert.match(notice.textContent, /no responde/);
});

test('a place that could not answer is a calm note beside the results', async () => {
  const app = start({ searchNotice: 'No he podido preguntar a NZBGeek.' });
  await searchFor(app, 'el sur');

  const notice = app.document.querySelector('#notice .note');
  assert.match(notice.textContent, /NZBGeek/);
  assert.ok(!notice.classList.contains('bad'), 'the results still stand');
  assert.ok(app.document.querySelector('#results .film'), 'and are still shown');
});

test('a season opens as a season and downloads as one', async () => {
  const app = start();
  await searchFor(app, 'cuéntame');

  const card = [...app.document.querySelectorAll('#results .film')]
    .find((node) => /Cuéntame/.test(node.textContent));
  assert.match(card.textContent, /Temporada 1/);
  click(app, card);
  await settle();

  assert.match(app.document.getElementById('detail-title').textContent, /Cuéntame · Temporada 1/);
  assert.match(app.document.getElementById('screen-detail').textContent, /episodios/);
  click(app, app.document.getElementById('download'));
  await settle();
  const grab = app.calls.find((call) => call.command === 'grab');
  assert.equal(grab.args.series, true);
});

test('a season says how many episodes it holds, and names them', async () => {
  const app = start({
    seasons: [{ ...SEASONS[0], imdb: 'tt0302447' }],
    seasonEpisodes: [
      { season: 1, number: 1, title: 'El retorno del fugitivo' },
      { season: 1, number: 2, title: 'Un cero a la izquierda' },
      { season: 1, number: 3, title: null },
    ],
  });
  await searchFor(app, 'cuéntame');
  click(app, [...app.document.querySelectorAll('#results .film')]
    .find((node) => /Cuéntame/.test(node.textContent)));
  await settle();

  const detail = app.document.getElementById('screen-detail');
  assert.match(detail.textContent, /Son 3 episodios/);
  const named = app.document.getElementById('episode-names');
  assert.equal(named.querySelectorAll('li').length, 2, 'an unnamed episode is not invented');
  assert.match(named.textContent, /El retorno del fugitivo/);
  assert.match(detail.textContent, /Ver los episodios en IMDb/);
});

test('a pack of several seasons counts its episodes without listing them all', async () => {
  const app = start({
    seasons: [{ ...SEASONS[0], label: 'Temporadas 1 a 5', imdb: 'tt0302447' }],
    seasonEpisodes: [
      { season: 1, number: 1, title: 'Uno' },
      { season: 1, number: 2, title: 'Dos' },
      { season: 2, number: 1, title: 'Tres' },
    ],
  });
  await searchFor(app, 'cuéntame');
  click(app, [...app.document.querySelectorAll('#results .film')]
    .find((node) => /Cuéntame/.test(node.textContent)));
  await settle();

  assert.match(app.document.getElementById('screen-detail').textContent, /Son 3 episodios/);
  assert.equal(app.document.getElementById('episode-names'), null);
});

test('a season nobody could identify still opens, without an episode list', async () => {
  const app = start();
  await searchFor(app, 'cuéntame');
  click(app, [...app.document.querySelectorAll('#results .film')]
    .find((node) => /Cuéntame/.test(node.textContent)));
  await settle();

  const detail = app.document.getElementById('screen-detail');
  assert.match(detail.textContent, /Son varios episodios/);
  assert.equal(app.document.getElementById('episode-names'), null);
  assert.doesNotMatch(detail.textContent, /IMDb/);
});

// A new release is a quiet banner, not a modal: one line, one button. Once the AppImage has
// replaced itself, the banner only says when the new version starts.
test('a new version is offered with one button, and an installed one just says so', async () => {
  const app = start({ update: { version: '0.3.0', installed: false } });
  await settle();
  const banner = app.document.getElementById('update');
  assert.match(banner.textContent, /versión nueva de Mamá Cine \(0\.3\.0\)/);
  click(app, app.document.getElementById('install-update'));
  await settle();
  assert.ok(app.calls.some((call) => call.command === 'open_update'), 'the button installs');

  const installed = start({ update: { version: '0.3.0', installed: true } });
  await settle();
  const done = installed.document.getElementById('update');
  assert.match(done.textContent, /ya está instalada/);
  assert.equal(installed.document.getElementById('install-update'), null, 'nothing left to press');
});

// The one place an update can be seen to have happened: Ajustes names the running version.
test('the settings screen says which version is running', async () => {
  const app = start();
  await settle();
  click(app, app.document.querySelector('[data-screen="settings"]'));
  await settle();
  assert.match(app.document.getElementById('app-version').textContent, /Mamá Cine 0\.4\.0/);
});

// The interface speaks the language the backend resolved: Spanish by default, English when
// the setting or the computer says so. One switch drives every word on screen.
test('the interface speaks the language the settings resolve', async () => {
  const app = start({ settings: { ...SETTINGS, app_language: 'en' } });
  await settle();
  const title = app.document.getElementById('search-title');
  assert.match(title.textContent, /What do you feel like watching today/);
  assert.equal(app.document.documentElement.lang, 'en');

  const spanish = start({ settings: { ...SETTINGS, app_language: 'es' } });
  await settle();
  assert.match(spanish.document.getElementById('search-title').textContent,
    /Qué te apetece ver hoy/);
  assert.equal(spanish.document.documentElement.lang, 'es');
});

test('a film she owns opens its own page, and plays by id from there', async () => {
  const shelf = [
    { id: 42, title: 'El Sur', subtitle_note: '', cover_url: null, year: '1983',
      languages: { audio_languages: ['spa'] }, series: false },
  ];
  const app = start({ shelf, synopsis: 'Una niña y su padre.' });
  await settle();
  await openOwned(app);

  assert.ok(!app.calls.some((call) => call.command === 'play'), 'opening is not playing');
  const page = app.document.getElementById('screen-owned');
  assert.match(page.querySelector('h1').textContent, /El Sur/);
  assert.match(page.textContent, /Una niña y su padre/, 'what it is about, on its own page');

  click(app, app.document.getElementById('play'));
  await settle();
  assert.equal(app.calls.find((call) => call.command === 'play').args.id, 42);
});

test('a track that names its regional variety is repeated, not flattened', async () => {
  const shelf = [
    { id: 7, title: 'Roma', subtitle_note: '', cover_url: null, year: '2018',
      languages: { audio_languages: ['es-MX'], subtitle_languages: ['es-419'] }, series: false },
  ];
  const app = start({ shelf });
  await settle();
  await openOwned(app);

  const page = app.document.getElementById('screen-owned');
  assert.match(page.textContent, /audio en español \(MX\)/);
  assert.match(page.textContent, /subtítulos en español latinoamericano/);
  assert.match(page.textContent, /Subtítulos en español/,
    'regional Spanish subtitles still count as Spanish');
});

test('a season on the shelf opens as episodes, each with a page of its own', async () => {
  const shelf = [
    { id: 9, title: 'Cuéntame · Temporada 1', subtitle_note: '', cover_url: null, year: '',
      languages: {}, series: true },
  ];
  const app = start({
    shelf,
    episodeRows: [
      { label: 'Episodio 1', subtitles: true, number: 1, title: 'El retorno del fugitivo',
        overview: 'Los Alcántara estrenan televisión.' },
      { label: 'Episodio 2', subtitles: true, number: 2, title: null, overview: null },
      { label: 'Episodio 3', subtitles: true, number: 3, title: null, overview: null },
    ],
  });
  await settle();
  await openOwned(app);

  assert.ok(app.document.getElementById('screen-owned'), 'its own screen');
  const rows = [...app.document.querySelectorAll('.episode')];
  assert.equal(rows.length, 3);
  assert.match(rows[0].textContent, /El retorno del fugitivo/, 'named where the database names it');
  assert.match(rows[1].textContent, /Episodio 2/, 'and numbered where it does not');
  assert.ok(!app.calls.some((call) => call.command === 'reveal'), 'no file manager');

  click(app, rows[1]);
  await settle();
  assert.ok(!app.calls.some((call) => call.command === 'play_episode'), 'opening is not playing');
  assert.ok(app.document.getElementById('screen-episode'), 'the episode has a page');
  assert.match(app.document.getElementById('episode-of').textContent, /Cuéntame/);

  click(app, app.document.getElementById('play-episode'));
  await settle();
  const played = app.calls.find((call) => call.command === 'play_episode');
  assert.equal(played.args.id, 9);
  assert.equal(played.args.position, 1);
});

test('an episode page says what happens in it, where the database says so', async () => {
  const shelf = [
    { id: 9, title: 'Cuéntame · Temporada 1', subtitle_note: '', cover_url: null, year: '',
      languages: {}, series: true },
  ];
  const app = start({
    shelf,
    episodeRows: [
      { label: 'Episodio 1', subtitles: true, number: 1, title: 'El retorno del fugitivo',
        overview: 'Los Alcántara estrenan televisión.' },
    ],
  });
  await settle();
  await openOwned(app);
  click(app, app.document.querySelector('.episode'));
  await settle();

  const page = app.document.getElementById('screen-episode');
  assert.match(page.querySelector('h1').textContent, /El retorno del fugitivo/);
  assert.match(page.textContent, /Los Alcántara estrenan televisión/);
  assert.match(page.textContent, /Episodio 1/, 'which episode it is, still said');
});

test('the episode without subtitles is the one that says so', async () => {
  const shelf = [
    { id: 9, title: 'Gomorrah · Temporada 1', cover_url: null, year: '', languages: {},
      series: true, subtitle_note: 'Faltan los subtítulos del episodio 2' },
  ];
  const app = start({
    shelf,
    episodeRows: [
      { label: 'Episodio 1', subtitles: true },
      { label: 'Episodio 2', subtitles: false },
      { label: 'Episodio 3', subtitles: true },
    ],
  });
  await settle();
  await openOwned(app);
  assert.match(app.document.getElementById('screen-owned').textContent,
               /Faltan los subtítulos del episodio 2/);

  const rows = [...app.document.querySelectorAll('.episode')];
  assert.equal(rows.filter((row) => row.querySelector('.without')).length, 1);
  assert.match(rows[1].textContent, /sin subtítulos/);

  click(app, rows[1]);
  await settle();
  assert.match(app.document.getElementById('subtitle-state').textContent,
               /no tiene subtítulos en español/, 'and says it again where she presses play');
});

test('a film can be removed from its page, with one chance to change her mind', async () => {
  const shelf = [
    { id: 42, title: 'El Sur', subtitle_note: '', cover_url: null, year: '1983',
      languages: {}, series: false },
  ];
  const app = start({ shelf });
  await settle();
  await openOwned(app);

  assert.equal(app.document.querySelector('#shelf button.link'), null,
    'the grid carries no errands');
  click(app, app.document.getElementById('remove'));
  await settle();
  assert.ok(!app.calls.some((call) => call.command === 'remove_film'), 'not yet');
  assert.match(app.document.getElementById('screen-owned').textContent, /¿Seguro?/);

  const confirm = [...app.document.querySelectorAll('#screen-owned button')]
    .find((button) => button.textContent.includes('Sí, borrar'));
  click(app, confirm);
  await settle();
  assert.equal(app.calls.find((call) => call.command === 'remove_film').args.id, 42);
  assert.ok(app.document.getElementById('screen-library'), 'and she lands back among her films');
  assert.match(app.document.getElementById('screen-library').textContent, /papelera/);
  assert.match(app.document.getElementById('screen-library').textContent, /todavía la puedes recuperar/,
    'recoverable is a fact she should have');
});

test('the page says which subtitles are there, and offers to look again', async () => {
  const ready = start({ shelf: [
    { id: 42, title: 'El Sur', subtitle_note: 'Subtítulos en español listos', cover_url: null,
      year: '1983', languages: {}, series: false },
  ] });
  await settle();
  await openOwned(ready);
  const state = ready.document.getElementById('subtitle-state');
  assert.match(state.textContent, /Subtítulos en español listos/);
  assert.ok(state.classList.contains('ok'), 'said as a state, not as a warning');

  const missing = start({ shelf: [
    { id: 42, title: 'El Sur', subtitle_note: 'No hay subtítulos en español para esta copia',
      cover_url: null, year: '1983', languages: {}, series: false },
  ] });
  await settle();
  await openOwned(missing);
  assert.ok(missing.document.getElementById('subtitle-state').classList.contains('bad'));

  click(missing, missing.document.getElementById('find-subtitles'));
  await settle();
  await settle();
  assert.equal(missing.calls.find((call) => call.command === 'fetch_subtitles').args.id, 42);
  assert.match(missing.document.getElementById('screen-owned').textContent, /añadidos/);
});

test('the download follows her to every screen, and only exists while there is one', async () => {
  const idle = start();
  await idle.poll();
  assert.ok(!idle.document.querySelector('nav button[data-screen="film"]'),
    'nothing coming down, nothing to follow');

  const active = [
    { id: 8, title: 'El Sur', status: 'downloading', percent: 62, cover_url: null,
      beneath: '', speed: '', year: '1983', attempt: 1, attempts_total: 1, series: false },
    { id: 9, title: 'Volver', status: 'downloading', percent: 10, cover_url: null,
      beneath: '', speed: '', year: '2006', attempt: 1, attempts_total: 1, series: false },
  ];
  const app = start({ active });
  await app.poll();

  const pill = app.document.querySelector('nav button[data-screen="film"]');
  assert.match(pill.textContent, /El Sur/, 'it names the film the download screen headlines');
  assert.match(pill.textContent, /62 %/);
  assert.match(app.document.getElementById('pill-count').textContent, /\+1/,
    'the second download is a count, not an averaged bar');

  click(app, pill);
  await settle();
  assert.equal(app.document.getElementById('screen-film').hidden, false);
});

test('a settled download is retired by walking away from it', async () => {
  const app = start({
    active: [{ id: 7, title: 'El Sur', status: 'downloading', percent: 90, cover_url: null,
               beneath: '', speed: '', year: '1983', series: false }],
  });
  await app.poll();
  click(app, app.document.querySelector('nav button[data-screen="film"]'));
  await settle();

  click(app, app.document.querySelector('nav button[data-screen="search"]'));
  await settle();
  assert.ok(app.document.querySelector('nav button[data-screen="film"]'),
    'walking away from a download still going does not abandon it');

  click(app, app.document.querySelector('nav button[data-screen="film"]'));
  await settle();
  app.live.active = [];
  app.live.finished = [
    { id: 7, title: 'El Sur', ok: true, retrying: false, next_id: null, detail: '',
      subtitle_note: '', cover_url: null, year: '1983', languages: {}, series: false,
      attempt: 1, attempts_total: 1, untried: 0, story: [] },
  ];
  await app.poll();
  assert.ok(!app.document.getElementById('screen-film'),
    'the screen goes where the film now is');
  assert.equal(app.calls.filter((call) => call.command === 'library_synopsis').length, 1,
    'and that is her own copy of it, opened');
  assert.ok(!app.document.querySelector('nav button[data-screen="film"]'),
    'the pill retires with it, rather than riding the masthead finished');
});

test('a swap ends on the copy that landed, not on the one it replaced', async () => {
  const app = start({
    shelf: [{ id: 4, title: 'La virgen roja', year: '2024', cover_url: null,
              subtitle_note: '', languages: {}, series: false }],
    copies: { index: 0, series: false, versions: VERSIONS },
    grabbed: { id: 11, already: false },
  });
  await app.poll();
  await openOwned(app);

  click(app, app.document.getElementById('show-copies'));
  await settle();
  click(app, app.document.querySelector('.version button.pick'));
  await settle();
  click(app, [...app.document.querySelectorAll('.version button')]
    .find((button) => button.textContent.includes('Sí, cambiar')));
  await settle();

  const grab = app.calls.find((call) => call.command === 'grab');
  assert.equal(grab.args.replacing, 4, 'the copy she has is what it replaces');

  app.live.active = [];
  app.live.finished = [
    { id: 11, title: 'La virgen roja', ok: true, retrying: false, next_id: null, detail: '',
      subtitle_note: '', cover_url: null, year: '2024', languages: {}, series: false,
      attempt: 1, attempts_total: 1, untried: 0, story: [] },
  ];
  await app.poll();

  const opened = app.calls.filter((call) => call.command === 'library_synopsis');
  assert.equal(opened[opened.length - 1].args.id, 11, 'the page follows the copy that landed');
});

test('a download that lands while she is elsewhere does not take the screen', async () => {
  const app = start({
    active: [{ id: 7, title: 'El Sur', status: 'downloading', percent: 90, cover_url: null,
               beneath: '', speed: '', year: '1983', series: false }],
  });
  await app.poll();
  click(app, app.document.querySelector('nav button[data-screen="library"]'));
  await settle();

  app.live.active = [];
  app.live.finished = [
    { id: 7, title: 'El Sur', ok: true, retrying: false, next_id: null, detail: '',
      subtitle_note: '', cover_url: null, year: '1983', languages: {}, series: false,
      attempt: 1, attempts_total: 1, untried: 0, story: [] },
  ];
  await app.poll();

  assert.ok(app.document.getElementById('screen-library'), 'she is where she was');
  assert.equal(app.calls.filter((call) => call.command === 'library_synopsis').length, 0);
});

test('the film page tells what it is about when the film database knows', async () => {
  const words = 'Un maestro republicano enseña a un niño lo que es la libertad.';
  const app = start({ synopsis: words });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  await settle();

  assert.match(app.document.getElementById('synopsis').textContent, new RegExp('maestro'));
  assert.match(app.document.getElementById('screen-detail').textContent, /Ver la ficha en IMDb/);

  const bare = start();
  await searchFor(bare, 'el sur');
  click(bare, bare.document.querySelector('#results .film'));
  await settle();
  await settle();
  assert.ok(!bare.document.getElementById('synopsis'), 'no ficha, no empty paragraph');
});

test('free space is drawn on the shelf, not on a strip of its own', async () => {
  const plenty = start();
  await plenty.poll();
  assert.ok(!plenty.document.querySelector('footer'), 'no strip');
  assert.ok(!plenty.document.getElementById('space'), 'nothing to warn about');
  click(plenty, plenty.document.querySelector('nav button[data-screen="library"]'));
  await settle();
  const disk = plenty.document.getElementById('shelf-disk');
  assert.match(disk.textContent, /412 GB libres/);
  assert.ok(!/de 953 GB/.test(disk.textContent), 'the division is drawn, not narrated');
  const meter = disk.querySelector('.meter');
  assert.match(meter.getAttribute('title'), /43 % libre/);
  assert.match(meter.querySelector('.used').getAttribute('style'), /width: 56\./);
});

test('a disk getting tight becomes a banner naming the way out', async () => {
  const tight = start({ free_bytes: 15 * 1024 ** 3, free_space: '15 GB' });
  await tight.poll();
  const banner = tight.document.getElementById('space');
  assert.match(banner.textContent, /Queda poco sitio/);
  assert.match(banner.textContent, /15 GB libres/);
  assert.match(banner.textContent, /Borra alguna película/, 'the action is named beside it');
  click(tight, tight.document.querySelector('nav button[data-screen="library"]'));
  await settle();
  assert.ok(tight.document.querySelector('#shelf-disk .meter.low'),
    'the shelf bar turns urgent too');
});

test('"ya la tienes" is never said about a film that is not on the shelf', async () => {
  const app = start({ shelf: [], have: null });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();

  assert.ok(!app.document.getElementById('already'), 'nothing here is hers yet');
  assert.ok(app.document.getElementById('download'), 'so downloading is what is offered');
});

test('a film she already has offers to play it rather than to download it again', async () => {
  const app = start({ have: 42 });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();

  const detail = app.document.getElementById('screen-detail');
  assert.match(detail.textContent, /Ya tienes esta película/);
  assert.ok(!app.document.getElementById('download'), 'downloading it twice is not offered');

  click(app, [...detail.querySelectorAll('button')]
    .find((button) => button.textContent.includes('Ver la película')));
  await settle();
  assert.equal(app.calls.find((call) => call.command === 'play').args.id, 42);
});

test('a season she already has is called a season and opens its episodes', async () => {
  const app = start({ have: 9 });
  await searchFor(app, 'cuéntame');
  click(app, [...app.document.querySelectorAll('#results .film')]
    .find((node) => /Cuéntame/.test(node.textContent)));
  await settle();

  const detail = app.document.getElementById('screen-detail');
  assert.match(detail.textContent, /Ya tienes esta temporada/);
  assert.ok(!/Ya tienes esta película/.test(detail.textContent), 'a season is not a película');
  assert.ok([...detail.querySelectorAll('button')]
    .some((button) => button.textContent.includes('Ver los episodios')));
});

test('a copy that turns out to be dead is followed to the next one', async () => {
  const finished = [
    { id: 7, title: 'El Sur', ok: false, subtitle_note: '', cover_url: null, detail: '',
      retrying: false, year: '1983', languages: {}, series: false, next_id: 8,
      attempt: 1, attempts_total: 3 },
  ];
  const active = [
    { id: 8, title: 'El Sur', status: 'downloading', percent: 12, cover_url: null,
      beneath: 'Unos 40 minutos', speed: '25 MB/s',
      year: '1983', attempt: 2, attempts_total: 3, series: false },
  ];
  const app = start({ finished, active });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();
  await app.poll();

  const screen = app.document.getElementById('screen-detail');
  assert.match(screen.textContent, /Descargando/, 'it is still going');
  assert.ok(!/no he podido/i.test(screen.textContent), 'nothing has failed yet');
  assert.ok(!/Copia \d/.test(screen.textContent), 'she is not made to count copies');
  assert.ok(!/faltaban/.test(screen.textContent), 'the dead copy stops talking once it is dead');
  assert.match(screen.textContent, /Unos 40 minutos/, 'how long it has left');
  assert.match(screen.textContent, /25 MB\/s/, 'and how fast it is going');
});

test('a failure the app has not answered yet reads as searching, not as the end', async () => {
  const finished = [
    { id: 7, title: 'El Sur', ok: false, retrying: true, next_id: null, detail: '',
      subtitle_note: '', cover_url: null, year: '1983', languages: {}, series: false,
      attempt: 1, attempts_total: 3 },
  ];
  const app = start({ finished });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();
  await app.poll();

  const status = app.document.getElementById('detail-status').textContent;
  assert.match(status, /Buscando otra copia/, status);
  assert.ok(!/No he podido/.test(status), 'the flash of false failure is the old bug');
});

test('a dead copy empties the bar once, as the words change', async () => {
  const finished = [
    { id: 7, title: 'El Sur', ok: false, retrying: true, next_id: null, detail: '',
      subtitle_note: '', cover_url: null, year: '1983', languages: {}, series: false,
      attempt: 1, attempts_total: 3, untried: 0 },
  ];
  const active = [
    { id: 7, title: 'El Sur', status: 'downloading', percent: 43, cover_url: null,
      beneath: '', speed: '', year: '1983', attempt: 1, attempts_total: 3, series: false },
  ];
  const app = start({ active, finished: [] });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();
  await app.poll();
  assert.match(app.document.querySelector('.bar i').getAttribute('style'), /43/);

  const dead = start({ active: [], finished });
  await searchFor(dead, 'el sur');
  click(dead, dead.document.querySelector('#results .film'));
  await settle();
  click(dead, dead.document.getElementById('download'));
  await settle();
  await dead.poll();

  assert.match(dead.document.getElementById('detail-status').textContent, /Buscando otra copia/);
  assert.match(dead.document.querySelector('.bar i').getAttribute('style'), /width: 0%/,
    'the bar empties with the words, not later');
});

test('a retry that is waiting for the server says so', async () => {
  const finished = [
    { id: 7, title: 'El Sur', ok: false, retrying: true, next_id: null, detail: '',
      subtitle_note: '', cover_url: null, year: '1983', languages: {}, series: false,
      attempt: 1, attempts_total: 3, untried: 0 },
  ];
  const app = start({ finished,
    problem: 'No consigo conectarme al servidor de descargas. Lo sigo intentando yo solo.' });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();
  await app.poll();

  assert.match(app.document.getElementById('detail-status').textContent, /Esperando/);
  assert.match(app.document.getElementById('problem').textContent, /sigo intentando/);
});

test('the give-up sentence is not said twice above the fold', async () => {
  const said = 'No he podido conseguir esta temporada: la única copia que había estaba estropeada.';
  const finished = [
    { id: 7, title: 'GoT', ok: false, retrying: false, series: true, subtitle_note: '',
      cover_url: null, year: '', languages: {}, next_id: null, attempt: 1, attempts_total: 1,
      untried: 0, detail: said,
      story: [{ at: 1_755_713_263, said, why: 'no copies left to try' }] },
  ];
  const app = start({ finished, grabbed: { id: 7, already: false } });
  await searchFor(app, 'got');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();
  await app.poll();

  const screen = app.document.getElementById('screen-detail');
  const outsideTheFold = screen.textContent.replace(
    app.document.querySelector('.story')?.textContent ?? '', '');
  const times = outsideTheFold.split('la única copia que había').length - 1;
  assert.equal(times, 1, 'said once in plain sight; the fold keeps the record');
});

test('when every copy has been tried it says so once, and stops', async () => {
  const finished = [
    { id: 7, title: 'El Sur', ok: false, retrying: false, series: false, subtitle_note: '',
      cover_url: null, year: '1983', languages: {}, next_id: null, attempt: 3, attempts_total: 3,
      untried: 0,
      detail: 'No he podido conseguir esta película: he probado las 3 copias que había y todas estaban estropeadas.' },
  ];
  const app = start({ finished });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();
  await app.poll();

  const screen = app.document.getElementById('screen-detail');
  assert.match(screen.textContent, /No he podido conseguirla/);
  assert.match(screen.textContent, /estaban estropeadas/);
  assert.ok(!app.document.getElementById('try-more'),
    'nothing left to try, so no button pretends otherwise');
});

test('a give-up with copies left offers to try them, and follows the new attempt', async () => {
  const finished = [
    { id: 7, title: 'El Sur', ok: false, retrying: false, series: false, subtitle_note: '',
      cover_url: null, year: '1983', languages: {}, next_id: null, attempt: 3, attempts_total: 8,
      untried: 5,
      detail: 'No he podido conseguir esta película: he probado 3 copias y todas estaban estropeadas; quedan 5 sin probar.' },
  ];
  const app = start({ finished });
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('download'));
  await settle();
  await app.poll();

  const button = app.document.getElementById('try-more');
  assert.match(button.textContent, /quedan 5/, 'the button says what it would spend');
  click(app, button);
  await settle();

  assert.equal(app.calls.find((call) => call.command === 'try_more').args.id, 7);
  assert.match(app.document.getElementById('detail-status').textContent, /Buscando otra copia/,
    'the screen follows the new attempt instead of staying on the corpse');
});

test('a paused download says it is paused, and why when the disk is the reason', async () => {
  const active = [
    { id: 8, title: 'El Sur', status: 'paused', percent: 40, cover_url: null,
      beneath: '', speed: '', year: '1983', attempt: 1, attempts_total: 1, series: false },
  ];
  const app = start({ active, free_bytes: 3 * 1024 ** 3, free_space: '3 GB' });
  await app.poll();
  click(app, app.document.querySelector('nav button[data-screen="film"]'));
  await settle();

  const screen = app.document.getElementById('screen-film');
  assert.match(app.document.getElementById('now-status').textContent, /En pausa/);
  assert.match(screen.textContent, /disco está casi lleno/);
});

test('a downloader that stopped answering is said plainly on screen', async () => {
  const app = start({ problem: 'No consigo conectarme con el descargador.' });
  await app.poll();

  const banner = app.document.getElementById('problem');
  assert.match(banner.textContent, /descargador/);

  click(app, app.document.getElementById('problem-log'));
  await settle();
  assert.ok(app.calls.some((call) => call.command === 'open_log_file'));
});

test('every running download is reachable from the download screen', async () => {
  const active = [
    { id: 8, title: 'El Sur', status: 'downloading', percent: 12, cover_url: null,
      beneath: '', speed: '', year: '1983', attempt: 1, attempts_total: 1, series: false },
    { id: 9, title: 'Volver', status: 'downloading', percent: 60, cover_url: null,
      beneath: '', speed: '', year: '2006', attempt: 1, attempts_total: 1, series: false },
  ];
  const app = start({ active });
  await app.poll();
  click(app, app.document.querySelector('nav button[data-screen="film"]'));
  await settle();

  assert.equal(app.document.getElementById('now-title').textContent, 'El Sur');
  const other = [...app.document.querySelectorAll('#also-downloading button')]
    .find((button) => button.textContent.includes('Volver'));
  click(app, other);
  await settle();
  assert.equal(app.document.getElementById('now-title').textContent, 'Volver');
});

test('the shelf is her disk, not what the downloader happens to remember', async () => {
  const finished = [
    { id: 1, title: 'El Sur', ok: true, detail: '', retrying: false, subtitle_note: '',
      cover_url: null, year: '1983', languages: {}, series: false, next_id: null,
      attempt: 1, attempts_total: 1 },
  ];
  const app = start({ finished, shelf: [] });
  await settle();
  click(app, app.document.querySelector('nav button[data-screen="library"]'));
  await settle();

  assert.match(app.document.getElementById('shelf').textContent, /Todavía no hay nada aquí/);
});

const openSettings = async (app) => {
  await settle();
  click(app, app.document.querySelector('nav button[data-screen="settings"]'));
  await settle();
};

const press = (app, within, label) =>
  click(app, [...app.document.querySelectorAll(within)]
    .find((button) => button.textContent.includes(label)));

const type = (app, field, value) => {
  field.value = value;
  field.dispatchEvent(new app.window.Event('input', { bubbles: true }));
};

test('two settings typed one after the other are both kept', async () => {
  const app = start();
  await openSettings(app);
  press(app, '#screen-settings button', 'Añadir otro buscador');
  await settle();

  const fields = app.document.querySelectorAll('.indexer')[1].querySelectorAll('input');
  type(app, fields[0], 'Otro');
  type(app, fields[1], 'https://otro.test');
  type(app, fields[2], 'su-clave');
  await settle();
  press(app, '#screen-settings .actions button', 'Guardar');
  await settle();
  await settle();

  const sent = app.calls.find((call) => call.command === 'save_settings').args.incoming.indexers;
  assert.equal(sent.length, 2);
  assert.equal(sent[1].name, 'Otro');
  assert.equal(sent[1].url, 'https://otro.test');
  assert.equal(sent[1].key, 'su-clave');
});

test('an indexer can be turned off without being thrown away', async () => {
  const app = start();
  await openSettings(app);
  const box = app.document.querySelector('.indexer input[type="checkbox"]');
  box.checked = false;
  box.dispatchEvent(new app.window.Event('change', { bubbles: true }));
  await settle();
  press(app, '#screen-settings .actions button', 'Guardar');
  await settle();
  await settle();

  const sent = app.calls.find((call) => call.command === 'save_settings').args.incoming.indexers;
  assert.equal(sent.length, 1, 'it is still there');
  assert.equal(sent[0].enabled, false);
});

test('a stored password is never sent back to the screen', async () => {
  const app = start({ settings: { ...SETTINGS, news_password: 'la-de-su-cuenta' } });
  await openSettings(app);
  const values = [...app.document.querySelectorAll('#screen-settings input')]
    .map((field) => field.value);
  assert.ok(!values.includes('la-de-su-cuenta'), 'nothing that looks like a password is filled in');
  const password = app.document.querySelector('#screen-settings input[name="news_password"]');
  assert.equal(password.value, '');
  assert.equal(password.getAttribute('placeholder'), 'sin cambios');
});

test('a password can be typed into, letter by letter', async () => {
  const app = start();
  await openSettings(app);
  const field = app.document.querySelector('#screen-settings input[name="news_password"]');
  for (const letter of 'secreto') {
    type(app, field, field.value + letter);
    await settle();
  }
  assert.equal(
    app.document.querySelector('#screen-settings input[name="news_password"]').value,
    'secreto',
    'every letter stayed where she put it',
  );

  press(app, '#screen-settings .actions button', 'Guardar');
  await settle();
  await settle();
  const sent = app.calls.find((call) => call.command === 'save_settings').args.incoming;
  assert.equal(sent.news_password, 'secreto');
});

test('a saved password is not left on the screen afterwards', async () => {
  const app = start();
  await openSettings(app);
  type(app, app.document.querySelector('#screen-settings input[name="subtitles_password"]'), 'x');
  await settle();
  press(app, '#screen-settings .actions button', 'Guardar');
  await settle();
  await settle();
  assert.equal(
    app.document.querySelector('#screen-settings input[name="subtitles_password"]').value,
    '',
    'the answer to the save carries no password back to the window',
  );
});

test('the settings file can be opened from the screen that writes it', async () => {
  const app = start();
  await openSettings(app);
  const path = app.document.querySelector('#screen-settings .path');
  assert.equal(path.textContent, '/home/fausto/.config/mamacine/settings.json');
  const button = app.document.getElementById('open-settings-file');
  assert.ok(button.closest('#technical'), 'she has no business here; it is for whoever set it up');
  click(app, button);
  await settle();
  assert.ok(app.calls.some((call) => call.command === 'open_settings_file'));
});

test('the log can be opened from the settings screen', async () => {
  const app = start();
  await openSettings(app);
  const paths = [...app.document.querySelectorAll('#screen-settings .path')]
    .map((element) => element.textContent);
  assert.ok(paths.includes('/home/fausto/.local/share/mamacine/mamacine.log'),
    'the screen names the log, so it can be found without the app');

  const file = app.document.getElementById('open-log-file');
  assert.ok(file.closest('#technical'), 'it is for whoever set the app up, not for her');
  click(app, file);
  await settle();
  assert.ok(app.calls.some((call) => call.command === 'open_log_file'));

  click(app, app.document.getElementById('open-log-folder'));
  await settle();
  assert.ok(app.calls.some((call) => call.command === 'open_log_folder'),
    'the folder too: sending the log to someone means finding it, not reading it');
});

test('a log the computer refuses to open says so', async () => {
  const app = start({ fail: 'open_log_file' });
  await openSettings(app);
  click(app, app.document.getElementById('open-log-file'));
  await settle();
  const notice = app.document.querySelector('#screen-settings .note.bad');
  assert.ok(notice, 'the refusal is on the screen, not only in a console nobody opens');
});

test('a settings file the computer refuses to open says so', async () => {
  const app = start({ fail: 'open_settings_file' });
  await openSettings(app);
  click(app, app.document.getElementById('open-settings-file'));
  await settle();
  const notice = app.document.querySelector('#screen-settings .note.bad');
  assert.ok(notice, 'the refusal is on the screen, not only in a console nobody opens');
  assert.match(notice.textContent, /no responde/);
});

test('where films are saved is chosen, never typed', async () => {
  const app = start();
  await openSettings(app);
  const field = app.document.querySelector('.folder input');
  assert.ok(field.readOnly, 'a path typed by hand is a path to get wrong');
  press(app, '.folder button', 'Elegir');
  await settle();
  assert.equal(app.document.querySelector('.folder input').value, '/home/fausto/Vídeos');
});

test('the settings screen is where she lands when nothing is filled in yet', async () => {
  const app = start({ settings: { ready: false, indexers: [], news_port: 563, language: 'any' } });
  await settle();
  assert.ok(app.document.getElementById('screen-settings'), 'there is nowhere else to go yet');
});

test('comprobar checks the values as typed, not as last saved', async () => {
  const app = start();
  await openSettings(app);
  const key = app.document.querySelectorAll('.indexer input')[2];
  type(app, key, 'clave-nueva');
  await settle();
  press(app, '#screen-settings .actions button', 'Comprobar');
  await settle();
  await settle();

  const checked = app.calls.find((call) => call.command === 'check_settings');
  assert.equal(checked.args.incoming.indexers[0].key, 'clave-nueva');
  assert.match(app.document.getElementById('screen-settings').textContent, /funciona/);
});

test('her language lives in the settings and travels with the save', async () => {
  const app = start();
  await openSettings(app);
  click(app, app.document.querySelector('#language .chip[data-lang="es"]'));
  await settle();
  press(app, '#screen-settings .actions button', 'Guardar');
  await settle();
  await settle();

  const sent = app.calls.find((call) => call.command === 'save_settings').args.incoming;
  assert.equal(sent.language, 'es');
  assert.ok(!app.document.querySelector('#screen-search .chip'), 'no language chips while searching');
});

test('the optional film database key lives with the technical settings', async () => {
  const app = start();
  await openSettings(app);
  const field = app.document.querySelector('#technical input[name="tmdb_key"]');
  assert.ok(field, 'the TMDB key is configurable');
  type(app, field, 'una-clave-tmdb');
  await settle();
  press(app, '#screen-settings .actions button', 'Guardar');
  await settle();
  await settle();
  const sent = app.calls.find((call) => call.command === 'save_settings').args.incoming;
  assert.equal(sent.tmdb_key, 'una-clave-tmdb');
});

test('the startup and tray switches are mom-level settings', async () => {
  const app = start();
  await openSettings(app);
  const autostart = app.document.querySelector('input[name="autostart"]');
  const keep = app.document.querySelector('input[name="keep_running"]');
  assert.ok(!autostart.closest('#technical'), 'not hidden among the technical settings');
  assert.ok(!keep.checked === false && keep.checked, 'keeping downloads alive is the default');

  autostart.checked = true;
  autostart.dispatchEvent(new app.window.Event('change', { bubbles: true }));
  await settle();
  press(app, '#screen-settings .actions button', 'Guardar');
  await settle();
  await settle();
  const sent = app.calls.find((call) => call.command === 'save_settings').args.incoming;
  assert.equal(sent.autostart, true);
  assert.equal(sent.keep_running, true);
});

test('the technical settings wait behind a fold', async () => {
  const app = start();
  await openSettings(app);
  const technical = app.document.getElementById('technical');
  assert.equal(technical.tagName, 'DETAILS');
  assert.ok(technical.querySelector('#indexers'), 'the indexers are inside it');
  assert.ok(technical.querySelector('input[name="news_host"]'), 'so is the news server');
  const simple = app.document.querySelector('#screen-settings .folder');
  assert.ok(simple, 'the folder chooser is not');
});

test('going back from a film returns to the results she came from', async () => {
  const app = start();
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  press(app, '.back', 'Volver');
  await settle();
  assert.equal(app.document.querySelectorAll('#results .film').length, 3);
});

test('the way to other copies sits with the other buttons, at their size', async () => {
  const app = start();
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();

  const toggle = app.document.getElementById('show-copies');
  assert.ok(toggle.closest('.actions'), 'it stands in the row of buttons, not below it');
  assert.ok(app.document.getElementById('download').closest('.actions'));

  click(app, toggle);
  await settle();
  const row = app.document.querySelector('.version');
  assert.equal(row.tagName, 'DIV', 'a row of facts, not one big button');
  assert.ok(row.querySelector('button.pick'), 'the button that starts it lives inside the row');
});

test('choosing another copy downloads that copy', async () => {
  const app = start();
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  click(app, app.document.getElementById('show-copies'));
  await settle();

  const copies = app.document.querySelectorAll('.version');
  assert.ok(copies.length > 1, 'there is something to choose between');
  const pick = copies[1].querySelector('button.pick');
  assert.ok(pick, 'each copy carries the button that starts it');
  assert.match(pick.textContent, /Descargar esta copia/);
  click(app, pick);
  await settle();
  assert.equal(app.calls.find((call) => call.command === 'grab').args.version, 1);
});

test('every id the stylesheet styles is an id the app actually renders', async () => {
  const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
  const selectors = css.split('}').map((rule) => rule.split('{')[0]).join(' ');
  const styled = new Set(selectors.match(/#[a-zA-Z][\w-]*/g) || []);

  const app = start({ settings: SETTINGS });
  await settle();
  const rendered = new Set();
  const collect = () => {
    for (const node of app.document.querySelectorAll('[id]')) rendered.add(`#${node.id}`);
  };
  for (const screen of ['search', 'library', 'settings']) {
    click(app, app.document.querySelector(`nav button[data-screen="${screen}"]`));
    await settle();
    collect();
  }
  click(app, app.document.querySelector('nav button[data-screen="search"]'));
  await settle();
  await searchFor(app, 'el sur');
  collect();
  click(app, app.document.querySelector('#results .film'));
  await settle();
  await settle();
  collect();

  for (const id of styled) assert.ok(rendered.has(id), `${id} is styled but never rendered`);
});

test('the settings fields are stacked, not run together on one line', async () => {
  const app = start({ settings: SETTINGS });
  await settle();
  click(app, app.document.querySelector('nav button[data-screen="settings"]'));
  await settle();

  const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
  assert.match(css, /#screen-settings label \{[^}]*display: block/, 'labels own their line');
  assert.match(css, /#screen-settings input \{[^}]*width: 100%/, 'boxes fill that line');
  assert.match(css, /#screen-settings \.switch input \{[^}]*width: auto/);
  assert.ok(app.document.querySelector('#screen-settings .switch input[type="checkbox"]'));
});

test('the shelf says it holds series, not only films', async () => {
  const shelf = [
    { id: 3, title: 'Cuéntame · Temporada 1', year: null, cover_url: null, subtitle_note: '',
      languages: {}, series: true },
  ];
  const app = start({ shelf });
  await settle();
  click(app, app.document.querySelector('nav button[data-screen="library"]'));
  await settle();

  const screen = app.document.getElementById('screen-library');
  assert.match(screen.querySelector('h1').textContent, /Mis películas y series/);
  assert.match(app.document.getElementById('shelf').textContent, /Serie/);
});

test('everything the app does on its own is written down where she can read it', async () => {
  const story = [
    { at: 1_755_712_940, said: 'Empieza la descarga.', why: 'copia 1 de 3' },
    { at: 1_755_713_260, said: 'Esa copia estaba estropeada, así que la he descartado.',
      why: 'FAILURE/HEALTH: faltaban 829 de 14368 partes, salud 93.8%' },
    { at: 1_755_713_263, said: 'Empieza la descarga de otra versión.', why: 'copia 2 de 3' },
  ];
  const active = [
    { id: 8, title: 'El Sur', status: 'downloading', percent: 3, cover_url: null, year: '1983',
      beneath: 'Unos 40 minutos', speed: '25 MB/s', attempt: 2, attempts_total: 3, series: false,
      story },
  ];
  const app = start({ active });
  await settle();
  await app.poll();
  click(app, app.document.querySelector('nav button[data-screen="film"]'));
  await settle();

  const latest = app.document.getElementById('story-latest');
  assert.match(latest.textContent, /otra versión/, 'the latest line is in plain sight');
  const lines = [...app.document.querySelectorAll('#screen-film .story li')];
  assert.equal(lines.length, 3, 'the whole story, not just the latest line');
  assert.match(lines[1].textContent, /estaba estropeada/);
  assert.match(lines[1].getAttribute('title'), /FAILURE\/HEALTH/);
  assert.ok(app.document.querySelector('.story-fold summary'), 'and it waits behind a fold');
});

test('the download screen never says the words copia, partes or a status code', async () => {
  const active = [
    { id: 8, title: 'El Sur', status: 'downloading', percent: 3, cover_url: null, year: '1983',
      beneath: 'Unos 40 minutos', speed: '25 MB/s', attempt: 3, attempts_total: 3, series: false,
      story: [{ at: 1_755_713_260, said: 'Esa copia estaba estropeada, así que la he descartado.',
                why: 'FAILURE/HEALTH: faltaban 71 de 6257 partes' }] },
  ];
  const app = start({ active });
  await settle();
  await app.poll();
  click(app, app.document.querySelector('nav button[data-screen="film"]'));
  await settle();

  const spoken = app.document.getElementById('screen-film').textContent;
  for (const jargon of [/\bCopia \d/, /\bpartes\b/, /FAILURE/, /HEALTH/, /\bpar2\b/, /\bnzb/i]) {
    assert.ok(!jargon.test(spoken), `"${jargon}" is on her screen`);
  }
  assert.match(spoken, /Unos 40 minutos/, 'what she actually wants: how long');
});

test('the decision screen shows the size, the free space and the whole disk', async () => {
  const app = start();
  await settle();
  await app.poll();
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();

  const room = app.document.getElementById('room');
  assert.match(room.textContent, /Ocupa 1,7 GB/);
  assert.match(room.textContent, /12 minutos/, 'the time it will actually take');
  const diskLine = app.document.getElementById('room-disk');
  assert.match(diskLine.textContent, /quedan 412 GB libres/);
  const meter = diskLine.querySelector('.meter');
  assert.ok(meter, 'the disk is drawn where she decides');
  assert.ok(meter.querySelector('.slice'), 'with this download\'s own share on it');
  assert.ok(!app.document.getElementById('room-warning'), 'nothing to warn about');
});

test('a film that may not fit warns her with the number behind the warning', async () => {
  const tight = start({ versions: [{ ...VERSIONS[0], room: 'tight', needs: '3,7 GB' }] });
  await tight.poll();
  await searchFor(tight, 'el sur');
  click(tight, tight.document.querySelector('#results .film'));
  await settle();
  assert.match(tight.document.getElementById('room').textContent, /Ocupa 1,7 GB/,
    'the facts stay');
  const warning = tight.document.getElementById('room-warning');
  assert.match(warning.textContent, /Puede que no quepa/);
  assert.match(warning.textContent, /necesita unos 3,7 GB/);
  assert.ok(!tight.document.getElementById('download').disabled, 'she decides');

  const never = start({ versions: [{ ...VERSIONS[0], room: 'no', needs: '3,7 GB' }] });
  await never.poll();
  await searchFor(never, 'el sur');
  click(never, never.document.querySelector('#results .film'));
  await settle();
  assert.match(never.document.getElementById('room-warning').textContent,
    /No hay sitio suficiente/);
  assert.ok(!never.document.getElementById('download').disabled,
    'still her call: the backend refuses with numbers if it truly cannot');
});

test('a film with no cover gets a drawn frame, not a hole', async () => {
  const app = start();
  await searchFor(app, 'el sur');
  await settle();

  const poster = app.document.querySelector('#results .poster');
  assert.ok(poster.classList.contains('none'), 'the frame says the cover is missing');
  assert.ok(poster.querySelector('svg'), 'and draws a filmstrip where the poster would be');

  const drawn = start({ films: [{ ...FILMS[0], cover_url: 'https://indexer.test/a.jpg' }] });
  await searchFor(drawn, 'el sur');
  await settle();
  assert.ok(!drawn.document.querySelector('#results .poster').classList.contains('none'),
    'a film that has a cover is never given the drawn one');
});

test('the decision band reads across the window, not down it', async () => {
  const css = readFileSync(new URL('./styles.css', import.meta.url), 'utf8');
  assert.match(css, /\.band\.decision \{[^}]*display: flex/);
  assert.match(css, /\.band\.decision \.facts \{[^}]*flex: 1/,
    'the facts take the room that is left');
  assert.match(css, /\.band\.decision \.actions \{[^}]*flex: none/,
    'the button keeps its own size');

  const app = start();
  await app.poll();
  await searchFor(app, 'el sur');
  click(app, app.document.querySelector('#results .film'));
  await settle();
  const facts = app.document.querySelector('#detail-band .facts');
  assert.ok(facts, 'the facts are gathered into one column');
  assert.ok(facts.contains(app.document.getElementById('room')));
  assert.ok(facts.contains(app.document.getElementById('room-disk')));
  assert.ok(facts.contains(app.document.getElementById('what-comes')));
  assert.ok(!facts.querySelector('#download'), 'the button is not among them');
});
