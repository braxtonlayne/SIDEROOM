// @ts-check
// Sketch & Slander's page (ACT-SKS-001): lobby, drawing, guessing, the scoreboard and the end screen, for members
// in the call and for guests on a phone (/play/<token>, D-397). The game itself runs in the host's page
// (host/engine.js through net/session.js); this file only shows the state and sends what the player does.

import { getPlatform } from './platform/platform.js';
import { createSession } from './net/session.js';
import { createPad } from './pad.js';
import { parsePack, parseShelfRef } from '../shared/pack.js';
import { DRAW_CHOICES, ERASER, LIMITS, PALETTE, ROUND_CHOICES, SIZES } from '../shared/rules.js';

/** @typedef {import('../host/engine.js').PublicState} PublicState */

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
const el = (/** @type {string} */ tag, /** @type {string} */ cls = '', /** @type {string} */ text = '') => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text) n.textContent = text;
  return n;
};

const PACK_ERRORS = /** @type {Record<string, string>} */ ({
  empty: 'That file is empty.',
  'too-big': 'That file is over 64 KB. Packs are small text or JSON files.',
  'not-text': 'That file is not text.',
  'bad-json': 'That file looks like JSON but does not parse.',
  'no-prompts': 'That JSON has no "prompts" list.',
  'too-many': 'That pack has more than 1000 prompts.',
  'too-few': 'A pack needs at least 5 prompts of up to 48 characters.',
});

// ---- theme ------------------------------------------------------------------------------------------------------
const query = new URLSearchParams(location.search);
const theme = query.get('theme');
if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;

/** Storage may be missing (a guest's sandbox has none): every read and write is guarded. */
const store = {
  get(/** @type {string} */ k) {
    try {
      return window.localStorage?.getItem(k) ?? null;
    } catch {
      return null;
    }
  },
  set(/** @type {string} */ k, /** @type {string} */ v) {
    try {
      window.localStorage?.setItem(k, v);
    } catch {
      /* no storage */
    }
  },
};

const platform = getPlatform(window);
/** @type {import('./net/session.js').Session | null} */
let session = null;
let screen = 'loading';
let ended = '';
const tool = { c: 0, w: 1, eraser: false };
/** @type {PublicState | null} */
let lastState = null;
let lastFeedLen = -1;
let lastFeedVer = -1;
let lastHtmlVer = -1;
let lastTurn = -1;
let toastTimer = 0;

const pad = createPad(/** @type {HTMLCanvasElement} */ ($('pad')), {
  canDraw: () => isDrawing(),
  tool: () => ({ c: tool.eraser ? ERASER : tool.c, w: tool.w }),
  emit: (op) => session?.draw(op),
});

function isDrawing() {
  const s = session?.state();
  return !!s && s.phase === 'draw' && s.drawer === session?.self && !!session?.secret().prompt;
}

/** @param {string} text */
function toast(text) {
  const t = $('toast');
  t.textContent = text;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (t.hidden = true), 2600);
}

/** @param {string} name */
function show(name) {
  if (screen === name) return;
  screen = name;
  for (const s of ['loading', 'lobby', 'game', 'end', 'ended']) $(`scr-${s}`).hidden = s !== name;
  $('hud').hidden = name !== 'game';
  if (name === 'game') pad.invalidate();
}

/** @param {PublicState} s @param {string} id */
const nameOf = (s, id) => s.players.find((p) => p.id === id)?.name ?? 'Someone';

// ---- tools ------------------------------------------------------------------------------------------------------

function buildTools() {
  const colors = $('colors');
  PALETTE.forEach((hex, i) => {
    const b = /** @type {HTMLButtonElement} */ (el('button', 'swatch'));
    b.type = 'button';
    b.setAttribute('role', 'radio');
    b.setAttribute('aria-label', `Colour ${i + 1}`);
    b.style.background = hex;
    b.dataset.c = String(i);
    b.addEventListener('click', () => {
      tool.c = i;
      tool.eraser = false;
      syncTools();
    });
    colors.append(b);
  });
  const sizes = $('sizes');
  SIZES.forEach((px, i) => {
    const b = /** @type {HTMLButtonElement} */ (el('button', 'size'));
    b.type = 'button';
    b.setAttribute('role', 'radio');
    b.setAttribute('aria-label', ['Thin', 'Medium', 'Thick'][i] ?? `Size ${i + 1}`);
    const dot = el('span', 'dot');
    const d = Math.max(4, Math.round(px * 0.6));
    dot.style.inlineSize = `${d}px`;
    dot.style.blockSize = `${d}px`;
    b.append(dot);
    b.dataset.w = String(i);
    b.addEventListener('click', () => {
      tool.w = i;
      syncTools();
    });
    sizes.append(b);
  });
  $('eraser').addEventListener('click', () => {
    tool.eraser = !tool.eraser;
    syncTools();
  });
  $('undo').addEventListener('click', () => isDrawing() && session?.draw({ k: 'u' }));
  $('clear').addEventListener('click', () => isDrawing() && session?.draw({ k: 'c' }));
  syncTools();
}

function syncTools() {
  for (const b of $('colors').children) b.setAttribute('aria-checked', String(!tool.eraser && Number(/** @type {HTMLElement} */ (b).dataset.c) === tool.c));
  for (const b of $('sizes').children) b.setAttribute('aria-checked', String(Number(/** @type {HTMLElement} */ (b).dataset.w) === tool.w));
  $('eraser').setAttribute('aria-pressed', String(tool.eraser));
}

// ---- lobby ------------------------------------------------------------------------------------------------------

function buildLobby() {
  const rounds = /** @type {HTMLSelectElement} */ ($('set-rounds'));
  for (const r of ROUND_CHOICES) rounds.append(new Option(r === 1 ? '1 round' : `${r} rounds`, String(r)));
  const time = /** @type {HTMLSelectElement} */ ($('set-time'));
  for (const t of DRAW_CHOICES) time.append(new Option(`${t} seconds`, String(t)));
  const sendSettings = () =>
    session?.send({
      t: 'settings',
      rounds: Number(rounds.value),
      drawSecs: Number(time.value),
      builtIn: /** @type {HTMLInputElement} */ ($('set-builtin')).checked,
    });
  rounds.addEventListener('change', sendSettings);
  time.addEventListener('change', sendSettings);
  $('set-builtin').addEventListener('change', sendSettings);
  $('start').addEventListener('click', () => session?.send({ t: 'start' }));
  $('name-form').addEventListener('submit', (e) => {
    e.preventDefault();
    const name = /** @type {HTMLInputElement} */ ($('name-input')).value.trim().slice(0, LIMITS.nameChars);
    if (!name) return;
    store.set('sketch.name', name);
    session?.send({ t: 'name', name });
    toast('Nickname saved');
  });
  $('pack-form').addEventListener('submit', (e) => {
    e.preventDefault();
    void loadPack(/** @type {HTMLInputElement} */ ($('pack-ref')).value);
  });
  $('again').addEventListener('click', () => session?.send({ t: 'again' }));
  $('skip').addEventListener('click', () => session?.send({ t: 'skip' }));
}

/** @param {string} text @param {'ok' | 'bad' | ''} kind */
function packMsg(text, kind) {
  const m = $('pack-msg');
  m.textContent = text;
  m.className = `small ${kind}`;
}

/**
 * The host loads a Guild Prompt Pack from a shelf (D-389): one per-object URL through the SDK, fetched and checked
 * here; the prompts stay in this page. Guests never get here (the panel is the host's, and the platform refuses).
 * @param {string} text
 */
async function loadPack(text) {
  const engine = session?.engine;
  if (!engine || platform.guest || !platform.canLoadPacks) return;
  const ref = parseShelfRef(text);
  if (!ref) return packMsg('Paste a shelf id and a file hash (64 hex characters), for example 123456/ab12…', 'bad');
  packMsg('Loading…', '');
  $('pack-load').setAttribute('disabled', '');
  try {
    const bytes = await platform.loadPack(ref);
    const pack = parsePack(bytes, `Pack ${engine.packs.length + 1}`);
    if (!pack.ok) return packMsg(PACK_ERRORS[pack.error] ?? 'That pack could not be used.', 'bad');
    engine.addPack(pack);
    /** @type {HTMLInputElement} */ ($('pack-ref')).value = '';
    packMsg(`Loaded "${pack.name}": ${pack.prompts.length} prompts${pack.skipped ? ` (${pack.skipped} skipped)` : ''}.`, 'ok');
  } catch (err) {
    const why = String(/** @type {any} */ (err)?.message ?? err);
    packMsg(why === 'too-big' ? PACK_ERRORS['too-big'] : 'That file could not be opened. Check the shelf id and hash, and that you can open the shelf.', 'bad');
  } finally {
    $('pack-load').removeAttribute('disabled');
  }
}

/** @param {PublicState} s */
function renderLobby(s) {
  const me = session?.self ?? '';
  const host = s.host === me;
  const players = s.players.filter((p) => !p.left);
  const list = $('lobby-players');
  list.replaceChildren(
    ...players.map((p) => {
      const li = el('li', p.id === me ? 'me' : '');
      li.append(el('span', '', p.name));
      if (p.id === s.host) li.append(el('span', 'tag', 'host'));
      else if (p.guest) li.append(el('span', 'tag', 'phone'));
      return li;
    }),
  );
  $('lobby-count').textContent = String(players.length);
  $('host-panel').hidden = !host;
  // A guest is already on a phone: the tip is for the people in the call.
  $('join-tip').hidden = platform.guest;
  $('wait-panel').hidden = host;
  $('wait-host').textContent = nameOf(s, s.host);
  $('wait-settings').textContent = `${s.settings.rounds} ${s.settings.rounds === 1 ? 'round' : 'rounds'}, ${s.settings.drawSecs} seconds to draw${s.packs.length ? `, ${s.packs.map((p) => p.name).join(', ')}` : ''}`;
  if (host) {
    /** @type {HTMLSelectElement} */ ($('set-rounds')).value = String(s.settings.rounds);
    /** @type {HTMLSelectElement} */ ($('set-time')).value = String(s.settings.drawSecs);
    const builtIn = /** @type {HTMLInputElement} */ ($('set-builtin'));
    builtIn.checked = s.settings.builtIn || s.packs.length === 0;
    builtIn.disabled = s.packs.length === 0;
    $('pack-panel').hidden = !platform.canLoadPacks;
    const packs = $('pack-list');
    packs.replaceChildren(
      ...s.packs.map((p) => {
        const li = el('li');
        li.append(el('span', '', `${p.name} · ${p.count} prompts`));
        const rm = /** @type {HTMLButtonElement} */ (el('button', 'btn ghost small', 'Remove'));
        rm.type = 'button';
        rm.addEventListener('click', () => session?.engine?.removePack(p.name));
        li.append(rm);
        return li;
      }),
    );
    const ready = players.filter((p) => !p.away).length >= 2;
    $('start').toggleAttribute('disabled', !ready);
    $('start-hint').textContent = ready ? '' : 'You need at least 2 players. Invite people in the call, or share the link with a phone.';
  }
}

// ---- game -------------------------------------------------------------------------------------------------------

/** @param {PublicState} s */
function renderBoard(s) {
  const me = session?.self ?? '';
  const ranked = s.players.filter((p) => !p.left || p.score > 0).slice().sort((a, b) => b.score - a.score);
  $('board').replaceChildren(
    ...ranked.map((p, i) => {
      const cls = [p.id === s.drawer ? 'drawing' : '', s.guessed.includes(p.id) ? 'got' : '', p.away || p.left ? 'away' : ''].join(' ').trim();
      const li = el('li', cls);
      li.append(el('span', 'rank', `#${i + 1}`), el('span', 'name', p.id === me ? `${p.name} (you)` : p.name));
      if (p.id === s.drawer) li.append(el('span', 'badge', 'drawing'));
      li.append(el('span', 'score', String(p.score)));
      return li;
    }),
  );
  $('skip').hidden = !(s.host === me && (s.phase === 'pick' || s.phase === 'draw'));
}

/** @param {PublicState} s */
function renderFeed(s) {
  if (s.ver === lastFeedVer && s.feed.length === lastFeedLen) return;
  lastFeedVer = s.ver;
  lastFeedLen = s.feed.length;
  const feed = $('feed');
  feed.replaceChildren(
    ...s.feed.map((f) => {
      const li = el('li');
      if (f.k === 'guess') li.append(el('span', 'who', `${f.who}: `), document.createTextNode(f.text ?? ''));
      else if (f.k === 'hit') li.append(el('span', 'hit', `${f.who} guessed it!`));
      else if (f.k === 'word') li.append(el('span', 'sys', 'The prompt was '), el('span', 'word', f.text ?? ''));
      else if (f.k === 'skip') li.append(el('span', 'sys', 'Turn skipped.'));
      else if (f.k === 'join') li.append(el('span', 'sys', `${f.who} joined.`));
      else if (f.k === 'left') li.append(el('span', 'sys', `${f.who} left.`));
      else if (f.k === 'round') li.append(el('span', 'sys', `Round ${f.n}`));
      return li;
    }),
  );
  feed.scrollTop = feed.scrollHeight;
}

/** @param {PublicState} s */
function renderOverlay(s) {
  const me = session?.self ?? '';
  const secret = session?.secret();
  const o = $('overlay');
  const key = `${s.phase}:${s.turnNo}:${s.ver}:${secret?.choices.join('|')}`;
  if (o.dataset.key === key) return;
  o.dataset.key = key;
  o.replaceChildren();
  if (s.phase === 'pick') {
    o.hidden = false;
    const box = el('div');
    if (s.drawer === me && secret && secret.choices.length) {
      box.append(el('h2', '', 'Your turn! Pick something to draw'));
      const choices = el('div', 'choices');
      secret.choices.forEach((c, i) => {
        const b = /** @type {HTMLButtonElement} */ (el('button', 'btn', c));
        b.type = 'button';
        b.addEventListener('click', () => session?.send({ t: 'pick', i }));
        choices.append(b);
      });
      box.append(choices);
    } else {
      box.append(el('h2', '', `${nameOf(s, s.drawer ?? '')} is choosing a prompt…`));
      box.append(el('p', 'small', `Round ${s.round} of ${s.settings.rounds}`));
    }
    o.append(box);
  } else if (s.phase === 'reveal') {
    o.hidden = false;
    const box = el('div');
    box.append(el('h2', '', s.word ? 'The prompt was' : 'Turn skipped'));
    if (s.word) box.append(el('p', 'word', s.word));
    const scores = el('ul', 'turn-scores');
    const entries = Object.entries(s.turnScores).sort((a, b) => b[1] - a[1]);
    if (entries.length === 0) scores.append(el('li', 'small', 'Nobody got it this time.'));
    for (const [id, pts] of entries) {
      const li = el('li');
      li.append(document.createTextNode(`${nameOf(s, id)} `), el('span', 'pts', `+${pts}`));
      scores.append(li);
    }
    box.append(scores);
    o.append(box);
  } else if (s.phase === 'draw' && s.drawer === me && !secret?.prompt) {
    o.hidden = false;
    o.append(el('h2', '', 'Getting your prompt…'));
  } else {
    o.hidden = true;
  }
}

/** @param {PublicState} s */
function renderGame(s) {
  const me = session?.self ?? '';
  const drawing = s.drawer === me;
  const secret = session?.secret();
  $('hud-round').textContent = `Round ${s.round}/${s.settings.rounds}`;
  const hint = $('hud-hint');
  hint.classList.toggle('plain', drawing && !!secret?.prompt && s.phase === 'draw');
  if (drawing && secret?.prompt && s.phase === 'draw') hint.textContent = secret.prompt;
  else if (s.phase === 'draw' || s.phase === 'reveal') {
    hint.textContent = s.mask;
    const n = s.mask.replace(/[^_\p{L}\p{N}]/gu, '').length;
    if (s.phase === 'draw' && n) hint.append(el('span', 'len', String(n)));
  } else hint.textContent = '';
  $('tools').hidden = !(drawing && s.phase === 'draw');
  const you = $('you-draw');
  you.hidden = !(drawing && s.phase === 'draw' && !!secret?.prompt);
  if (!you.hidden) {
    you.replaceChildren(document.createTextNode('Draw: '), el('span', 'word', secret?.prompt ?? ''));
  }
  const guessing = s.phase === 'draw' && !drawing && !s.guessed.includes(me);
  const input = /** @type {HTMLInputElement} */ ($('guess'));
  input.disabled = !guessing;
  /** @type {HTMLButtonElement} */ ($('guess-form').querySelector('button')).disabled = !guessing;
  input.placeholder = drawing ? 'You are drawing' : s.guessed.includes(me) ? 'You got it!' : s.phase === 'draw' ? 'Type your guess' : 'Wait for the next turn';
  $('guess-form').hidden = drawing && (s.phase === 'draw' || s.phase === 'pick');
  /** @type {HTMLCanvasElement} */ ($('pad')).classList.toggle('can-draw', isDrawing());
  if (s.turnNo !== lastTurn) {
    lastTurn = s.turnNo;
    pad.cancel();
    $('guess-note').textContent = '';
    $('guess-note').className = 'small muted';
  }
  renderBoard(s);
  renderFeed(s);
  renderOverlay(s);
}

/** @param {PublicState} s */
function renderEnd(s) {
  const me = session?.self ?? '';
  const ranked = s.players.filter((p) => !p.left || p.score > 0).slice().sort((a, b) => b.score - a.score);
  $('podium').replaceChildren(
    ...ranked.slice(0, 3).map((p, i) => {
      const li = el('li', `p${i + 1}`);
      li.append(el('span', 'nm', p.name), el('span', 'sc', `${p.score} pts`));
      li.append(el('div', 'block', String(i + 1)));
      return li;
    }),
  );
  $('end-rest').replaceChildren(
    ...ranked.slice(3).map((p, i) => {
      const li = el('li');
      li.append(el('span', '', `${i + 4}. ${p.name}`), el('span', '', `${p.score} pts`));
      return li;
    }),
  );
  $('again').hidden = s.host !== me;
  $('again-wait').hidden = s.host === me;
}

function render() {
  if (ended) {
    show('ended');
    $('ended-text').textContent =
      ended === 'host-left' ? 'The host left, so the game stopped. Thanks for playing!' : ended === 'full' ? 'This game is full (16 players).' : ended;
    return;
  }
  const s = session?.state();
  if (!s) return show('loading');
  lastState = s;
  if (s.phase === 'lobby') {
    show('lobby');
    if (s.ver !== lastHtmlVer) renderLobby(s);
  } else if (s.phase === 'end') {
    show('end');
    if (s.ver !== lastHtmlVer) renderEnd(s);
  } else {
    show('game');
    if (s.ver !== lastHtmlVer) renderGame(s);
  }
  lastHtmlVer = s.ver;
  platform.setActivity(s.phase === 'lobby' ? '' : s.phase === 'end' ? 'Final scores' : `Round ${s.round} of ${s.settings.rounds}`);
}

function frame() {
  const s = session?.state();
  if (s && screen === 'game') {
    const secs = Math.ceil(s.left / 1000);
    const timer = $('hud-timer');
    timer.textContent = s.phase === 'draw' || s.phase === 'pick' ? String(secs) : '';
    timer.classList.toggle('low', s.phase === 'draw' && secs <= 10);
    pad.render();
  }
  requestAnimationFrame(frame);
}

// ---- start ------------------------------------------------------------------------------------------------------

async function main() {
  buildTools();
  buildLobby();
  requestAnimationFrame(frame);
  try {
    await platform.ready();
  } catch (err) {
    ended = String(/** @type {any} */ (err)?.message ?? 'Could not connect.');
    render();
    return;
  }
  const channel = platform.channel;
  if (!channel) {
    ended = 'There is no data channel.';
    render();
    return;
  }
  const name = (store.get('sketch.name') || platform.identity().name || 'Player').slice(0, LIMITS.nameChars);
  /** @type {HTMLInputElement} */ ($('name-input')).value = name;
  session = createSession({ channel, name, guest: platform.guest, engine: { instance: platform.instanceId() } });
  session.on((what, data) => {
    if (what === 'state') {
      lastHtmlVer = -1;
      render();
    } else if (what === 'ink') pad.setInk(session?.ink() ?? []);
    else if (what === 'secret') {
      lastHtmlVer = -1;
      render();
    } else if (what === 'close') {
      const n = $('guess-note');
      n.textContent = `"${data}" is close!`;
      n.className = 'small close';
    } else if (what === 'hit') {
      const n = $('guess-note');
      n.textContent = `You got it: ${data.prompt} (+${data.pts})`;
      n.className = 'small hit';
      toast(`+${data.pts} points!`);
    } else if (what === 'ended' || what === 'error') {
      ended = String(data);
      render();
    }
  });
  $('guess-form').addEventListener('submit', (e) => {
    e.preventDefault();
    const input = /** @type {HTMLInputElement} */ ($('guess'));
    const text = input.value.trim();
    if (!text) return;
    session?.send({ t: 'guess', text });
    input.value = '';
  });
  window.addEventListener('pagehide', () => session?.leave());
  window.addEventListener('resize', () => pad.invalidate());
  session.hello(name);
  render();
}

// Test hooks (the e2e gate and the visual check read and drive the game through these).
/** @type {any} */ (window).__sketch = {
  screen: () => screen,
  state: () => session?.state() ?? null,
  secret: () => session?.secret() ?? null,
  isHost: () => !!session?.isHost,
  guest: () => platform.guest,
  self: () => session?.self ?? '',
  send: (/** @type {object} */ m) => session?.send(m),
  /** Draw a line from (x0, y0) to (x1, y1) in logical units, as the drawer. */
  line(/** @type {number} */ x0, /** @type {number} */ y0, /** @type {number} */ x1, /** @type {number} */ y1, c = 0, w = 1) {
    const pts = [];
    for (let i = 0; i <= 20; i++) pts.push(Math.round(x0 + ((x1 - x0) * i) / 20), Math.round(y0 + ((y1 - y0) * i) / 20));
    session?.draw({ k: 'p', id: Math.floor(Math.random() * 1e9), c, w, pts });
  },
  loadPack: (/** @type {string} */ text) => loadPack(text),
  lastState: () => lastState,
};

void main();
