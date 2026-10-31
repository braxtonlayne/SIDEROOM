// @ts-check
// Land.io client: joins the arena of this Activity instance, sends inputs, plays out the server's ticks.
// It knows the host only through platform/platform.js and the server only through net/transport.js.

import { getPlatform } from './platform/platform.js';
import { createTransport } from './net/transport.js';
import { World } from './world.js';
import { Renderer } from './render.js';
import { Hud, pct } from './hud.js';
import { bindInput, firstKey, keysOf, hintText } from './input.js';
import { lookColor } from '../shared/look.js';
import { Predictor } from './predict.js';
import { FLAG_STOPPED } from './world.js';
import { ACT, CARDS, MAX_EXTRA_MOVES } from '../shared/cards.js';
import { FLAG_TRAILING } from '../shared/codec.js';
import { sfx } from './audio.js';
import { FX } from './fx/index.js';
import { MODE_BY_ID } from '../shared/modes/index.js';
import { MAP_BY_ID } from '../shared/maps/index.js';
import { Menu } from './menu.js';
import { loadPrefs } from './prefs.js';
import { normalizeCode } from '../shared/room-code.js';

const NET_DOWN_MS = 2500;
/** The sound a card makes when someone uses it (default 'itemUse'). @type {Record<string, string>} */
const USE_SOUND = {
  dash: 'dash',
  grenade: 'grenadeThrow',
  snare: 'snare',
  wall: 'wall',
  emp: 'emp',
  rewind: 'rewind',
  phase: 'phase',
  mirage: 'mirage',
  claimflag: 'claimflag',
};
/** Danger glow: full when an enemy head is this close to your trail, none from DANGER_FAR on. */
const DANGER_NEAR = 1;
const DANGER_FAR = 6;
const SUMMARY_DELAY_MS = 900;
const PRESENCE_MS = 5000;

// Personal settings (the menu's Me tab). The renderer reads `prefs` every frame, so changes apply at once.
const userPrefs = loadPrefs();
const systemReduced = window.matchMedia('(prefers-reduced-motion: reduce)');
/** @type {{reducedMotion: boolean, shake: boolean, names: boolean, quality: 'auto' | 'high' | 'low'}} */
const prefs = { reducedMotion: false, shake: true, names: true, quality: 'auto' };
/** @param {import('./prefs.js').Prefs} p */
function applyPrefs(p) {
  prefs.reducedMotion = p.motion === 'on' || (p.motion === 'system' && systemReduced.matches);
  prefs.shake = p.shake;
  prefs.names = p.names;
  prefs.quality = p.quality;
  document.body.classList.toggle('no-minimap', !p.minimap);
}
applyPrefs(userPrefs);
systemReduced.addEventListener?.('change', () => applyPrefs(userPrefs));

const canvas = /** @type {HTMLCanvasElement} */ (document.getElementById('game'));
const world = new World();
const renderer = new Renderer(canvas, prefs);
const hud = new Hud();
const platform = getPlatform(window);
const transport = createTransport();
const predictor = new Predictor(world);
// `?predict=0` turns own-head prediction off (for measuring what it buys).
const predictOn = new URLSearchParams(location.search).get('predict') !== '0';

/** 'boot' | 'start' | 'playing' | 'dead' */
let screen = 'boot';
let myPid = 0;
let spawnTick = 0;
let lastReplyAt = performance.now();
let deathShownFor = 0;

// Measurements the browser checks read (see tools/perf-browser.mjs, test/browser-smoke.mjs).
const metrics = {
  /** ms from a key press to the frame where the head starts moving the new way */
  inputLatencyMs: /** @type {number[]} */ ([]),
  /** tiles between where the head was heading at the press and where it actually turned */
  lateTiles: /** @type {number[]} */ ([]),
  frameMs: /** @type {number[]} */ ([]),
  renderMs: /** @type {number[]} */ ([]),
  cspViolations: /** @type {string[]} */ ([]),
  /** frames where the drawn own head jumped more than ~1 tile (a prediction correction the player sees) */
  snaps: 0,
  /** per frame: ticks between the server's now and what is drawn for everyone else (the playout delay) */
  behindTicks: /** @type {number[]} */ ([]),
  /**
   * Hiccups while visible: a frame over HICCUP_MS (with what the frame before it spent its time on) or a gap
   * over STALL_MS between ticks arriving. Reported to the server with the rest (see report()).
   * @type {Record<string, number|string>[]}
   */
  hiccups: [],
};
const HICCUP_MS = 40;
const STALL_MS = 300;
const REPORT_MS = 5000;
/** What the last frame spent its time on (ms), to explain the gap before the next one. */
let prevWork = { play: 0, pred: 0, draw: 0, hud: 0, recs: 0, chunks: 0, wave: 0 };
let lastTickArrival = 0;
let reported = 0;
let lastReport = 0;
/** @type {{seq: number, dir: number, t: number, expect: {x: number, y: number} | null, seen: boolean, late: boolean}[]} */
let presses = [];

/**
 * The own head as drawn this frame: the tick segment it is on (a -> b) and how far along. Predicted when
 * prediction is on, else the plain playout.
 * @type {{a: {x: number, y: number, dir: number, stopped: boolean}, b: {x: number, y: number, dir: number, stopped: boolean}, f: number, P: number, path?: Map<number, {x: number, y: number}>} | null}
 */
let own = null;

document.addEventListener('securitypolicyviolation', (e) => {
  metrics.cspViolations.push(`${e.violatedDirective} ${e.blockedURI}`);
});

world.onCapture = (/** @type {number} */ owner, /** @type {number[]} */ tiles, /** @type {{x: number, y: number} | undefined} */ head) => {
  renderer.captureWave(tiles, world.w, head, performance.now());
  if (owner === myPid && screen === 'playing' && world.appliedTick + 1 > spawnTick) {
    const n = tiles.length;
    sfx.play(n > 150 ? 'bigCapture' : 'capture', { size: Math.min(1, n / 150) });
    hud.landDelta(n, world.w * world.h);
  }
};

/** Tiles between the local head and (x, y), for distance-faded sounds. @param {number} x @param {number} y */
function distFromMe(x, y) {
  const h = lastOwnHead ?? renderer.cam;
  return Math.abs(h.x - x) + Math.abs(h.y - y);
}
world.onEvent = (/** @type {any} */ e) => {
  if (e.k === 'fx') {
    FX[e.card]?.onEvent?.(e, renderer, world);
    const dist = e.x !== undefined ? distFromMe(e.x, e.y) : 0;
    if (e.card === 'grenade' && e.phase === 'boom') sfx.play('grenadeBoom', { dist, size: 1 });
    else if (e.card === 'shield') sfx.play('shieldBlock', { dist });
    else if (e.card === 'secondwind') sfx.play('secondWind', { dist });
    else if (e.card === 'snare' && e.v !== undefined) sfx.play('snare', { dist });
    return;
  }
  if (e.k === 'use') {
    // Everyone sees which card someone used, above their head.
    const at = world.heads.get(world.appliedTick)?.get(e.id);
    if (at) renderer.floatText(at.x, at.y, CARDS[e.card]?.name ?? e.card, '#ffffff');
    sfx.play(USE_SOUND[e.card] ?? 'itemUse', { dist: e.id === myPid ? 0 : at ? distFromMe(at.x, at.y) : 20 });
    return;
  }
  if (e.k === 'settings') {
    if (e.mode) hud.toast(`${e.name} switched to ${MODE_BY_ID[e.mode]?.name ?? e.mode} on ${MAP_BY_ID[e.map]?.name ?? e.map}`);
    else hud.toast(`${e.name} changed the game settings`);
    return;
  }
  if (e.k === 'host') {
    hud.toast(e.phase === 'restart' ? `${e.name} restarted the round` : `${e.name} is the host now`);
    return;
  }
  // People coming and going (D-61); your own arrival needs no toast.
  if (e.k === 'joined') {
    if (e.name !== platform.identity().name) hud.toast(e.via === 'code' ? `${e.name} joined by code` : `${e.name} joined`);
    return;
  }
  if (e.k === 'left') {
    hud.toast(e.why === 'kick' ? `${e.name} was removed` : `${e.name} left`);
    return;
  }
  if (e.k === 'round' && e.phase === 'start' && e.n > 1) {
    hud.toast(`Round ${e.n}`, 'good');
    return;
  }
  if (e.k === 'card') {
    if (e.id === myPid && screen === 'playing') {
      sfx.play('cardPick');
      const c = CARDS[e.card];
      if (c?.kind === 'item') hud.tip('items', `Items: ${firstKey('item0')} and ${firstKey('item1')} use them, once each.`);
      else if (c?.kind === 'active') hud.tip('ability', `Abilities: ${keysOf('ability')} uses yours; it recharges.`);
    }
    return;
  }
  if (e.k !== 'kill') return;
  // Player ids are reused: an event from before our spawn is about the id's previous owner.
  const me = myPid > 0 && world.appliedTick + 1 > spawnTick ? myPid : -1;
  hud.kill(e, me);
  const look = world.roster.get(e.v)?.look ?? e.v;
  // Events play while their tick is applied: the victim was last seen on the tick before.
  const at = world.heads.get(world.appliedTick)?.get(e.v);
  if (at) renderer.burst(at.x, at.y, lookColor(look), e.v === me ? 28 : 12);
  if (e.v === me && screen === 'playing') {
    renderer.shake(0.9);
    sfx.play('death');
    dead();
  } else if (e.id === me) {
    renderer.shake(0.25);
    sfx.play('kill');
    hud.toast(`You got ${e.vn}`, 'good');
  } else if (at) sfx.play('enemyDeath', { dist: distFromMe(at.x, at.y) });
};

function dead() {
  screen = 'dead';
  deathShownFor = myPid;
  setTimeout(() => {
    const d = world.you?.death;
    if (screen === 'dead' && d) {
      hud.showSummary(d, world.w * world.h, 1000 / world.tickMs, summaryExtra(d));
      const respawn = MODE_BY_ID[world.round?.mode ?? 'classic']?.respawn ?? true;
      /** @type {HTMLElement} */ (document.getElementById('again')).textContent = respawn ? 'Play again' : 'Join the next round';
    }
  }, SUMMARY_DELAY_MS);
}

/**
 * The server put us in the game without a Play press: a new round revived us, or a queued press came true.
 * @param {number} pid @param {number} tick the tick we spawned on
 */
function enter(pid, tick) {
  stopWatching();
  myPid = pid;
  spawnTick = tick;
  presses = [];
  predictor.acts = [];
  deathShownFor = 0;
  screen = 'playing';
  cameraCut = true;
  lastCards = [];
  hud.playMessage('');
  hud.hideOverlays();
  canvas.focus();
}

/** Results between rounds, the next round's start, and being put back in (runs a few times a second). @param {number} now */
function rounds(now) {
  const rd = world.round;
  if (!rd || screen === 'boot') return;
  const hz = 1000 / world.tickMs;
  const tick = Math.floor(predictor.serverNow(now));
  hud.modeLine(world);
  if (rd.phase === 'over') {
    if (menu.isOpen) return;
    if (screen !== 'results') {
      stopWatching();
      screen = 'results';
      sfx.play('bigCapture', { size: 1 });
    }
    hud.showResults(world, myPid, hz, tick);
    return;
  }
  if (world.you?.alive && world.you.pid && screen !== 'playing' && !joining && world.contig >= rd.start) {
    enter(world.you.pid, world.you.spawn ?? rd.start);
    return;
  }
  if (screen === 'results') {
    hud.hideResults();
    screen = 'start';
    hud.showStart(platform.identity().name);
  }
}

/** Whom the camera follows while watching (a player id; -1: the leader). */
let watchId = -1;
/** Living players in leaderboard order (score, then id), the local player left out. */
function watchable() {
  return [...world.stats.entries()]
    .filter(([id, s]) => s.alive && id !== myPid)
    .sort((a, b) => b[1].score - a[1].score || a[0] - b[0])
    .map(([id]) => id);
}
/** @param {number} id */
function setWatch(id) {
  watchId = id;
  const name = world.roster.get(id)?.name ?? '…';
  const el = /** @type {HTMLElement} */ (document.getElementById('watch-name'));
  if (el.textContent !== name) el.textContent = name;
}
/** Switch whom to watch, through the leaderboard order. @param {number} by +1 or -1 */
function cycleWatch(by) {
  const ids = watchable();
  if (!ids.length) return;
  const at = ids.indexOf(watchId);
  setWatch(ids[(at < 0 ? 0 : at + by + ids.length) % ids.length]);
  cameraCut = true;
  sfx.play('uiClick');
}
/** Watch the game instead of playing (from the start or death card). */
function watch() {
  sfx.unlock();
  hud.hideOverlays();
  screen = 'watch';
  setWatch(watchable()[0] ?? -1);
  cameraCut = true;
  /** @type {HTMLElement} */ (document.getElementById('watch')).hidden = false;
  /** @type {HTMLElement} */ (document.getElementById('watch-play')).focus();
}
function stopWatching() {
  /** @type {HTMLElement} */ (document.getElementById('watch')).hidden = true;
}
for (const b of document.querySelectorAll('.watch-btn')) b.addEventListener('click', () => watch());
/** @type {HTMLElement} */ (document.getElementById('watch-prev')).addEventListener('click', () => cycleWatch(-1));
/** @type {HTMLElement} */ (document.getElementById('watch-next')).addEventListener('click', () => cycleWatch(1));
/** @type {HTMLElement} */ (document.getElementById('watch-play')).addEventListener('click', () => play());

const menu = new Menu({
  transport,
  prefs: userPrefs,
  onPrefs: applyPrefs,
  audio: sfx,
  toast: (t) => hud.toast(t),
  onKeys: () => hud.keysChanged(),
  // Switching leaves this room first; a code that does not work shows why, with the way back (roomFailed).
  async onJoinRoom(code) {
    await switchRoom(code);
    return '';
  },
  // In a browser tab there is no channel: leaving goes back to the lobby (and the address drops the room).
  onBackToChannel: () => {
    if (platform.kind === 'web') {
      const q = new URLSearchParams(location.search);
      q.delete('room');
      history.replaceState(null, '', `${location.pathname}${q.size ? `?${q}` : ''}`);
    }
    return switchRoom(null);
  },
});
hud.keysChanged();
/** @param {string} [tab] */
function openMenu(tab) {
  sfx.unlock();
  menu.update(world);
  // Filled now, not on the HUD's next refresh, so the code is there the moment the tab opens.
  menu.roomInfo(world, roomInfo && { ...roomInfo, canGoBack: viaCode || platform.kind === 'web', leave: platform.kind === 'web' });
  menu.open(tab);
}
for (const b of document.querySelectorAll('.settings-btn')) b.addEventListener('click', () => openMenu('game'));
for (const b of document.querySelectorAll('.invite-btn')) b.addEventListener('click', () => openMenu('room'));
/** @type {HTMLElement} */ (document.getElementById('menu-btn')).addEventListener('click', () => openMenu());

/** Who got you (with their colour) and the cards you held. @param {import('../shared/types.js').DeathInfo} d */
function summaryExtra(d) {
  let killer = null;
  if (d.killer) for (const p of world.roster.values()) if (p.name === d.killer) killer = { name: p.name, look: p.look };
  if (d.killer && !killer) killer = { name: d.killer, look: 0 };
  return { killer, cards: lastCards };
}

let joining = false;
/** A direction pressed while the spawn request is on its way, applied once the player exists. */
let earlyDir = -1;
async function play() {
  // One request at a time, and the overlay goes at once: the reply can take a round trip or two.
  if (joining) return;
  joining = true;
  sfx.unlock();
  sfx.play('uiClick');
  const wasDead = screen === 'dead';
  stopWatching();
  hud.hideOverlays();
  /** @type {{ok: boolean, pid?: number, tick?: number, error?: string}} */
  const r = await transport.play().catch(() => ({ ok: false, error: 'network' }));
  joining = false;
  hud.playMessage(
    r.ok && r.pid
      ? ''
      : r.error === 'full'
        ? 'No room to start right now. Try again in a moment.'
        : r.error === 'wait'
          ? "You're in: the next round starts soon."
          : r.error === 'no-seat'
          ? 'You were away too long. Reconnecting…'
            : 'Could not reach the host of this game. Try again.',
  );
  if (!r.ok || !r.pid) {
    if (r.error === 'replaced') {
      replaced();
      return;
    }
    if (r.error === 'kicked') {
      kicked();
      return;
    }
    if (r.error === 'host-left') {
      hostLeft();
      return;
    }
    if (r.error === 'no-seat') {
      transport.stop();
      boot();
      return;
    }
    const d = world.you?.death;
    if (wasDead && d) hud.showSummary(d, world.w * world.h, 1000 / world.tickMs, summaryExtra(d));
    else hud.showStart(platform.identity().name);
    return;
  }
  myPid = r.pid;
  spawnTick = r.tick ?? world.recvTick;
  presses = [];
  predictor.acts = [];
  screen = 'playing';
  cameraCut = true;
  lastCards = [];
  hud.tip('move', `Steer with ${hintText().split(' to turn')[0]}. Leave your land to draw a trail.`);
  hud.hideOverlays();
  canvas.focus();
  if (earlyDir >= 0) {
    const d = earlyDir;
    earlyDir = -1;
    turn(d);
  }
}

/** Send an action (a direction, or -1 for stop/go), aimed at the tile the head is shown heading into. @param {number} d */
function act(d) {
  if (screen !== 'playing') return -1;
  const moving = !!own && !own.a.stopped && own.a.dir >= 0;
  if (!predictOn) return transport.input(d === -1 ? { stop: true } : { turns: [d] });
  const plan = predictor.plan(myPid, d, own ? own.P : world.recvTick, moving);
  if (!plan) return -1; // changes nothing: not sent, does not take a slot
  const seq = transport.input(d === -1 ? { stop: true, at: plan.at } : { turns: [d], at: plan.at });
  predictor.add(seq, d, plan.at, plan.replaces);
  return seq;
}

/** @param {number} dir */
function turn(dir) {
  // Arrow keys move through the menu's controls while it is open, not the head.
  if (menu.isOpen) return;
  if (screen === 'watch') {
    if (dir === 1 || dir === 3) cycleWatch(dir === 1 ? 1 : -1);
    return;
  }
  if (joining) {
    earlyDir = dir;
    return;
  }
  const before = own;
  const seq = act(dir);
  if (seq < 0 || !before) return;
  // Measure only real turns (not the current direction, not an ignored reversal).
  const cur = before.b.dir;
  if (dir === cur || (dir === ((cur + 2) & 3) && !world.isHome(before.b.y * world.w + before.b.x, myPid))) return;
  sfx.play('turn');
  renderer.pop();
  const moving = !before.a.stopped && before.a.dir >= 0;
  // The tile the turn was aimed at: the one being entered, or (predicting, just arrived) the one reached.
  const early = predictOn && before.f < 0.2;
  presses.push({ seq, dir, t: performance.now(), expect: moving && !early ? before.b : before.a, seen: false, late: false });
  if (presses.length > 20) presses.shift();
}

function stop() {
  if (menu.isOpen) return;
  if (screen === 'watch') {
    play();
    return;
  }
  act(-1);
}

/** A card key or click: pick from the draft, use an item, fire the ability. @param {number} code */
function card(code) {
  if (screen !== 'playing' || menu.isOpen) return;
  const hand = world.you?.hand;
  const nowTick = predictor.serverNow(performance.now());
  if (code <= ACT.choose0) {
    if (!hand?.offers.length) return; // nothing to pick: digits are free for other things
    act(code);
    return;
  }
  // Say no right away (a buzz and a shake) when it cannot work, instead of sending it and nothing happening.
  const slot = code === ACT.item0 ? 0 : code === ACT.item1 ? 1 : -1;
  const empty = slot >= 0 ? !hand?.items[slot] : !hand?.active;
  const recharging = slot < 0 && !!hand?.active && hand.active[2] > nowTick;
  const emped = (hand?.fx.emp ?? 0) > nowTick;
  if (empty || recharging || emped) {
    sfx.play('denied');
    hud.denied(code);
    if (emped) hud.toast('EMP: no cards for a moment', 'bad');
    return;
  }
  act(code);
}

/** Log a hiccup (kept to the last 100). @param {Record<string, number|string>} h */
function hiccup(h) {
  metrics.hiccups.push({ at: Math.round(performance.now()), ...h });
  if (metrics.hiccups.length > 100) metrics.hiccups.shift();
}

/** Send a diagnostics summary and the hiccups since the last one to the server. @param {number} now */
function report(now) {
  const q = (/** @type {number[]} */ a, /** @type {number} */ p) => {
    if (!a.length) return 0;
    const s = [...a].sort((x, y) => x - y);
    return +s[Math.min(s.length - 1, Math.floor(p * s.length))].toFixed(1);
  };
  const fresh = metrics.hiccups.filter((h) => /** @type {number} */ (h.at) > reported);
  reported = Math.round(now);
  transport.report({
    screen,
    visible: !document.hidden,
    fps: +(1000 / (metrics.frameMs.reduce((a, b) => a + b, 0) / Math.max(1, metrics.frameMs.length))).toFixed(1),
    frameP95: q(metrics.frameMs, 0.95),
    frameMax: q(metrics.frameMs, 1),
    renderP95: q(metrics.renderMs, 0.95),
    renderMax: q(metrics.renderMs, 1),
    behindP95: q(metrics.behindTicks, 0.95),
    underruns: world.underruns,
    snaps: metrics.snaps,
    rttMs: transport.stats().rttMs,
    // Press to visible turn (ms), over the presses since the last report.
    inputP50: q(metrics.inputLatencyMs.slice(-50), 0.5),
    inputP95: q(metrics.inputLatencyMs.slice(-50), 0.95),
    platform: platform.kind,
    hiccups: fresh.slice(-20),
    ua: navigator.userAgent.slice(0, 120),
    dpr: devicePixelRatio,
    canvas: `${canvas.width}x${canvas.height}`,
  });
}

/**
 * Settle presses: latency = press to the first frame the drawn head moves the new way; lateness = tiles
 * between where the head was shown heading at the press and where the server actually turned it.
 * @param {number} now
 */
function measure(now) {
  if (!presses.length) return;
  const [seq, tick] = world.you?.ack ?? [0, 0];
  for (const p of presses) {
    if (!p.seen && own && own.b.dir === p.dir && !(own.a.x === own.b.x && own.a.y === own.b.y)) {
      p.seen = true;
      metrics.inputLatencyMs.push(Math.round(now - p.t));
    }
    if (!p.late && seq >= p.seq && tick) {
      const before = world.heads.get(tick - 1)?.get(myPid);
      const after = world.heads.get(tick)?.get(myPid);
      if (before && after) {
        p.late = true;
        if (after.dir === p.dir && before.dir !== p.dir && p.expect)
          metrics.lateTiles.push(Math.abs(before.x - p.expect.x) + Math.abs(before.y - p.expect.y));
      }
    }
  }
  presses = presses.filter((p) => !(p.seen && p.late) && now - p.t < 4000);
}

/** The own head segment for this frame. @param {number} r @param {number} now */
function ownSegment(r, now) {
  if (!myPid || screen !== 'playing') return null;
  /** @param {any} h */
  const conv = (h) => (h ? { x: h.x, y: h.y, dir: h.dir, stopped: 'stopped' in h ? h.stopped : !!(h.flags & FLAG_STOPPED) } : null);
  if (!predictOn) {
    const k = Math.floor(r);
    const a = conv(world.heads.get(k)?.get(myPid));
    const b = conv(world.heads.get(k + 1)?.get(myPid)) ?? a;
    return a && b ? { a, b, f: r - k, P: r } : null;
  }
  predictor.prune(world.you?.ack?.[0] ?? 0, world.you?.ack?.[1] ?? 0);
  const P = predictor.clock(now, transport.stats().rttMs, r);
  const k = Math.floor(P);
  const path = predictor.path(myPid, k + 1);
  if (!path) return null;
  const at = (/** @type {number} */ t) => conv(path.get(t) ?? world.heads.get(t)?.get(myPid));
  const a = at(k);
  const b = at(k + 1) ?? a;
  return a && b ? { a, b, f: P - k, P, path } : null;
}

let lastR = 0;
/** @type {{x: number, y: number, dir: number, flags: number} | null} */
let lastOwnHead = null;
let lastFrame = performance.now();
let lastHud = 0;
let lastFeel = 0;
let cameraCut = false;
let wasTrailing = false;
let lastOfferCount = 0;
let lastDanger = -1;
/** The card ids held at the last check while alive (shown on the death summary). @type {string[]} */
let lastCards = [];
const vignette = /** @type {HTMLElement} */ (document.getElementById('vignette'));

/**
 * 10x a second: the danger glow (an enemy head near your trail), leaving your land, new drafts, the cards
 * you hold.
 */
function feel() {
  const playing = screen === 'playing' && !!world.you?.alive;
  const hand = world.you?.hand;
  if (playing && hand) {
    lastCards = [...hand.items.map((c) => c[0]), ...(hand.active ? [hand.active[0]] : []), ...hand.passives.map((c) => c[0])];
    if (hand.offers.length > lastOfferCount) {
      sfx.play('cardOffer');
      hud.tip('cards', `You earned cards! Press ${firstKey('pick1')}, ${firstKey('pick2')} or ${firstKey('pick3')} (or click) to pick one.`);
    }
    lastOfferCount = hand.offers.length;
  } else lastOfferCount = 0;

  const me = playing ? world.heads.get(world.appliedTick)?.get(myPid) : undefined;
  const trailing = !!me && (me.flags & FLAG_TRAILING) !== 0;
  if (trailing && !wasTrailing) {
    sfx.play('leave');
    hud.tip('capture', 'Now get back to your land: the loop and everything inside it becomes yours.');
  }
  wasTrailing = trailing;

  // Danger: the nearest enemy head to any tile of your trail (around your head, where it matters).
  let danger = 0;
  if (trailing && me) {
    const { w, h } = world;
    let best = Infinity;
    const R = 24;
    /** @type {{x: number, y: number}[]} */
    const enemies = [];
    for (const id of world.headIds(lastR)) {
      if (id === myPid) continue;
      const e = world.headAt(id, lastR);
      if (e && Math.abs(e.x - me.x) + Math.abs(e.y - me.y) < R + DANGER_FAR) enemies.push(e);
    }
    if (enemies.length)
      for (let y = Math.max(0, me.y - R); y <= Math.min(h - 1, me.y + R); y++)
        for (let x = Math.max(0, me.x - R); x <= Math.min(w - 1, me.x + R); x++) {
          if (world.trail[y * w + x] !== myPid) continue;
          for (const e of enemies) {
            const d = Math.abs(e.x - x) + Math.abs(e.y - y);
            if (d < best) best = d;
          }
        }
    danger = Math.max(0, Math.min(1, (DANGER_FAR - best) / (DANGER_FAR - DANGER_NEAR)));
  }
  const q = Math.round(danger * 10) / 10;
  if (q !== lastDanger) {
    lastDanger = q;
    vignette.style.setProperty('--danger', String(q));
  }
}
let lastAdapt = 0;
/** @type {number[]} frame times since the last adapt() */
let adaptFrames = [];
/** What the last HUD update spent, by part (ms): leaderboard/stats, cards, minimap. */
let hudParts = { lb: 0, cards: 0, mm: 0 };
let lastPresence = 0;
let pollGamepad = () => {};

/** @param {number} now */
function frame(now) {
  const rawDt = now - lastFrame;
  const dt = Math.min(100, rawDt);
  lastFrame = now;
  if (rawDt > HICCUP_MS && !document.hidden && screen !== 'boot')
    hiccup({ kind: 'frame', ms: Math.round(rawDt), ...prevWork, behind: +(predictor.serverNow(now) - lastR).toFixed(2) });
  const t0 = performance.now();
  pollGamepad();
  const r = world.playout(now);
  lastR = r;
  const applied0 = world.appliedTick;
  world.advance(Math.floor(r));
  const t1 = performance.now();
  own = ownSegment(r, now);
  if (!own) lastOwnHead = null;
  measure(now);
  const t2 = performance.now();

  let follow = null;
  /** @type {{head: {x: number, y: number, dir: number, flags: number}, trail: number[]} | null} */
  let ownView = null;
  if (own) {
    const { a, b, f } = own;
    // More tiles than a tick can move is a teleport (respawn, Rewind): no slide.
    const jump = Math.abs(a.x - b.x) + Math.abs(a.y - b.y) > 1 + MAX_EXTRA_MOVES;
    const head = {
      x: jump ? b.x : a.x + (b.x - a.x) * f,
      y: jump ? b.y : a.y + (b.y - a.y) * f,
      dir: b.dir,
      flags: b.stopped ? FLAG_STOPPED : 0,
    };
    // Tiles the drawn head has passed that the playout has not applied yet are drawn as trail.
    /** @type {number[]} */
    const trail = [];
    if (predictOn)
      for (let t = Math.floor(r) + 1; t <= Math.floor(own.P); t++) {
        const p = own.path?.get(t) ?? world.heads.get(t)?.get(myPid);
        if (!p) continue;
        const i = p.y * world.w + p.x;
        if (!world.isHome(i, myPid)) trail.push(i);
      }
    ownView = { head, trail };
    if (lastOwnHead && Math.abs(head.x - lastOwnHead.x) + Math.abs(head.y - lastOwnHead.y) > 1.2) metrics.snaps++;
    lastOwnHead = head;
    follow = head;
  } else if (myPid && world.headAt(myPid, r)) follow = world.headAt(myPid, r);
  else {
    // Spectating: whoever was picked to watch, else the leader.
    const id = screen === 'watch' && world.headAt(watchId, r) ? watchId : watchable()[0] ?? -1;
    if (screen === 'watch' && id !== watchId) setWatch(id);
    if (id > 0) follow = world.headAt(id, r);
  }
  renderer.resize();
  const t3 = performance.now();
  renderer.draw(world, r, { follow, me: myPid, now, dt, own: ownView, cut: cameraCut });
  cameraCut = false;
  const t4 = performance.now();
  if (world.samples.length) metrics.behindTicks.push(predictor.serverNow(now) - r);
  if (metrics.behindTicks.length > 600) metrics.behindTicks.shift();
  metrics.frameMs.push(dt);
  metrics.renderMs.push(renderer.frameMs);
  if (metrics.frameMs.length > 600) {
    metrics.frameMs.shift();
    metrics.renderMs.shift();
  }

  if (now - lastHud > 250) {
    lastHud = now;
    const alive = screen === 'playing' ? world.appliedTick - spawnTick : 0;
    const h0 = performance.now();
    hud.update(world, myPid, alive, 1000 / world.tickMs);
    const h1 = performance.now();
    hud.cards(screen === 'playing' ? (world.you?.hand ?? null) : null, predictor.serverNow(now), card);
    const h2 = performance.now();
    hud.minimap(world, myPid, r, renderer.view);
    hud.roundChip(world, myPid, 1000 / world.tickMs, Math.floor(predictor.serverNow(now)));
    rounds(now);
    menu.update(world);
    hud.roomLine(world);
    menu.roomInfo(world, roomInfo && { ...roomInfo, canGoBack: viaCode || platform.kind === 'web', leave: platform.kind === 'web' });
    hudParts = { lb: +(h1 - h0).toFixed(1), cards: +(h2 - h1).toFixed(1), mm: +(performance.now() - h2).toFixed(1) };
    hud.netDown(now - lastReplyAt > NET_DOWN_MS);
  }
  // Once a second, let the renderer adapt its resolution to this machine's frame times.
  if (!document.hidden) adaptFrames.push(rawDt);
  if (now - lastAdapt > 1000) {
    lastAdapt = now;
    renderer.adapt(adaptFrames);
    adaptFrames = [];
  }
  prevWork = {
    play: +(t1 - t0).toFixed(1),
    pred: +(t2 - t1).toFixed(1),
    draw: +(t4 - t3).toFixed(1),
    hud: +(performance.now() - t4).toFixed(1),
    recs: world.appliedTick - applied0,
    chunks: renderer.painted,
    ...renderer.parts,
    wave: renderer.wave.size,
    ...hudParts,
  };
  hudParts = { lb: 0, cards: 0, mm: 0 };
  if (now - lastReport > REPORT_MS && screen !== 'boot') {
    lastReport = now;
    report(now);
  }
  if (now - lastFeel > 100) {
    lastFeel = now;
    feel();
  }
  if (now - lastPresence > PRESENCE_MS) {
    lastPresence = now;
    const s = world.stats.get(myPid);
    platform.setActivity(screen === 'playing' && s ? `${pct(s.land, world.w * world.h)} land` : '');
  }
  // A death we did not see play out (e.g. after a reload) still ends the round.
  if (screen === 'playing' && world.you && world.you.pid === myPid && !world.you.alive && deathShownFor !== myPid) {
    const d = world.you.death;
    if (d && world.appliedTick >= d.tick) dead();
  }
  requestAnimationFrame(frame);
}

let platformReady = false;
/** Names of the people in the voice channel, from the platform. @type {string[]} */
let channelPeople = [];
/**
 * The room to (re)join: a code from the page's link (`?room=`), or the one this page is in, so a rejoin after a
 * stall or "Play here" goes back to it. null: the channel's own room (D-59).
 * @type {string | null}
 */
let roomCode = normalizeCode(new URLSearchParams(location.search).get('room'));
/** What the server said about the room joined. @type {{code: string, instance: boolean, url?: string} | null} */
let roomInfo = null;
/** Whether this page came in by a code (then the channel's own room is a way back). */
let viaCode = false;
/** The next join opens a new room (the lobby's Create a room). */
let createNext = false;

/**
 * The lobby (a plain browser tab, D-60): a name, then create a room or join one by code. `code`: a room from
 * the page's link, offered first. `error`: why the last try did not work.
 * @param {string | null} code @param {string} [error]
 */
function showLobby(code, error = '') {
  screen = 'lobby';
  const web = /** @type {import('./platform/platform.js').Platform} */ (platform);
  /** @type {HTMLElement} */ (document.getElementById('boot')).hidden = true;
  hud.hideOverlays();
  const el = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
  const nameIn = /** @type {HTMLInputElement} */ (el('l-name'));
  if (!nameIn.value) nameIn.value = web.hasName?.() ? web.identity().name : '';
  el('l-join-this').hidden = !code;
  el('l-join-this').textContent = code ? `Join room ${code}` : 'Join room';
  el('l-join-this').dataset.code = code ?? '';
  // With a room from the link, joining it is the main action; creating one steps back.
  el('l-create').classList.toggle('ghost', !!code);
  el('l-msg').textContent = error;
  el('l-msg').hidden = !error;
  el('lobby').hidden = false;
  (nameIn.value ? el(code ? 'l-join-this' : 'l-create') : nameIn).focus();
}

/** From the lobby: take the name, then join `code` (or create a room when null). @param {string | null} code */
async function lobbyGo(code) {
  const el = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
  const name = /** @type {HTMLInputElement} */ (el('l-name')).value.trim();
  if (!name) {
    showLobby(code, 'Pick a name first.');
    return;
  }
  sfx.unlock();
  const err = (await platform.setName?.(name)) ?? '';
  if (err) {
    showLobby(code, err);
    return;
  }
  el('lobby').hidden = true;
  roomCode = code;
  createNext = !code;
  return boot();
}
/** @type {HTMLElement} */ (document.getElementById('l-create')).addEventListener('click', () => lobbyGo(null));
// Enter in the name field does the obvious thing: join the linked room, else create one.
/** @type {HTMLElement} */ (document.getElementById('l-name')).addEventListener('keydown', (e) => {
  if (e.key !== 'Enter') return;
  e.preventDefault();
  const joinThis = /** @type {HTMLElement} */ (document.getElementById('l-join-this'));
  lobbyGo(joinThis.hidden ? null : joinThis.dataset.code || null);
});
/** @type {HTMLElement} */ (document.getElementById('l-join-this')).addEventListener('click', (e) =>
  lobbyGo(/** @type {HTMLElement} */ (e.currentTarget).dataset.code || null),
);
/** @type {HTMLElement} */ (document.getElementById('l-join-form')).addEventListener('submit', (e) => {
  e.preventDefault();
  const typed = /** @type {HTMLInputElement} */ (document.getElementById('l-code')).value;
  const code = normalizeCode(typed);
  if (!code) showLobby(null, ROOM_ERRORS['bad-code']);
  else lobbyGo(code);
});

/** Messages for a code that did not work. @type {Record<string, string>} */
const ROOM_ERRORS = {
  'bad-code': 'That is not a room code: 6 letters and digits, never O, 0, I, L or 1.',
  'no-room': 'No room has that code (it may have closed).',
  'slow-down': 'Too many wrong codes. Wait a minute and try again.',
  'room-full': 'That room is full.',
  'too-many-rooms': 'The server has no room for more rooms right now.',
  banned: 'The host removed you from that room.',
  'no-host': 'Could not reach the host of this game. Is the activity still open for them?',
};
// Creating rooms has its own limit (D-61); the same error word means something else there.
const CREATE_SLOW = 'You made a lot of rooms just now. Wait a few minutes, or join one by code.';

/** Leave this room and start the page over in another (`code`), or in the channel's own room (null). @param {string | null} code */
async function switchRoom(code) {
  await transport.leave();
  stopWatching();
  hud.hideOverlays();
  world.reset();
  predictor.reset();
  myPid = 0;
  spawnTick = 0;
  deathShownFor = 0;
  presses = [];
  own = null;
  lastOwnHead = null;
  cameraCut = true;
  screen = 'boot';
  roomCode = code;
  const boot0 = /** @type {HTMLElement} */ (document.getElementById('boot'));
  boot0.replaceChildren(Object.assign(document.createElement('div'), { className: 'card', textContent: 'Joining the room…' }));
  boot0.hidden = false;
  return boot();
}

/**
 * A code that did not work at boot: say why, and offer the channel's room (or another try).
 * @param {string} error
 */
function roomFailed(error) {
  const el = /** @type {HTMLElement} */ (document.getElementById('boot'));
  const card = document.createElement('div');
  card.className = 'card';
  const p = document.createElement('p');
  p.textContent = ROOM_ERRORS[error] ?? `Could not join: ${error}`;
  const actions = document.createElement('div');
  actions.className = 'boot-actions';
  const back = document.createElement('button');
  back.type = 'button';
  back.className = 'btn';
  back.textContent = 'Play in this channel\'s room';
  back.addEventListener('click', () => {
    // Drop the code from the address too, so a reload does not try it again.
    const q = new URLSearchParams(location.search);
    q.delete('room');
    history.replaceState(null, '', `${location.pathname}${q.size ? `?${q}` : ''}`);
    switchRoom(null);
  });
  actions.append(back);
  card.append(p, actions);
  el.replaceChildren(card);
  el.hidden = false;
}
/**
 * Another window of the same person took the seat. Stop here and offer to take it back; rejoining on our own
 * would knock the other window off, which would rejoin too, and the two would fight over one player.
 */
function replaced() {
  transport.stop();
  screen = 'boot';
  hud.hideOverlays();
  const el = /** @type {HTMLElement} */ (document.getElementById('boot'));
  const card = document.createElement('div');
  card.className = 'card';
  const p = document.createElement('p');
  p.textContent = 'Land.io is open in another window.';
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = 'btn';
  btn.textContent = 'Play here';
  btn.addEventListener('click', () => {
    const wait = document.createElement('p');
    wait.textContent = 'Joining the arena…';
    card.replaceChildren(wait);
    boot();
  });
  card.append(p, btn);
  el.replaceChildren(card);
  el.hidden = false;
  btn.focus();
}

/** The host closed Land.io or left the call: the game ran in their browser and is gone. */
function hostLeft() {
  kicked('The host left, so this game has ended. Ask them to start Land.io again.');
}

/**
 * The host removed us (D-61), or the room could not be joined. Stop here and say so; outside an activity (the
 * old plain-browser build) offer a way on.
 * @param {string} [why] what the card says
 */
function kicked(why = 'The host removed you from this room.') {
  transport.stop();
  stopWatching();
  screen = 'boot';
  hud.hideOverlays();
  const web = platform.kind === 'web';
  const el = /** @type {HTMLElement} */ (document.getElementById('boot'));
  const card = document.createElement('div');
  card.className = 'card';
  const p = document.createElement('p');
  p.textContent = why;
  if (document.body.classList.contains('hosted')) {
    // One game per activity instance, run by its host: there is no other room to go to.
    card.append(p);
    el.replaceChildren(card);
    el.hidden = false;
    return;
  }
  const actions = document.createElement('div');
  actions.className = 'boot-actions';
  /** @param {string} text @param {() => void} go */
  const button = (text, go) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'btn';
    b.textContent = text;
    b.addEventListener('click', go);
    actions.append(b);
    return b;
  };
  /** Forget the room in the address, so a reload does not try it again. */
  const dropRoom = () => {
    const q = new URLSearchParams(location.search);
    q.delete('room');
    history.replaceState(null, '', `${location.pathname}${q.size ? `?${q}` : ''}`);
  };
  if (web) {
    button('Back to the lobby', () => {
      dropRoom();
      el.hidden = true;
      roomCode = null;
      showLobby(null);
    });
  } else {
    button('Start a new room', () => {
      dropRoom();
      createNext = true;
      switchRoom(null);
    });
    // Removed from a room we reached by code: this channel's own room is still open to us.
    if (viaCode)
      button("Back to this channel's room", () => {
        dropRoom();
        switchRoom(null);
      }).classList.add('ghost');
  }
  card.append(p, actions);
  el.replaceChildren(card);
  el.hidden = false;
  /** @type {HTMLElement} */ (actions.firstElementChild)?.focus();
}

async function boot() {
  if (!platformReady) {
    try {
      await platform.ready();
    } catch (e) {
      // Sign-in failed (declined, or the server is not set up for this platform): say so instead of hanging.
      const el = /** @type {HTMLElement} */ (document.getElementById('boot'));
      el.textContent = `Could not sign in: ${/** @type {Error} */ (e).message}`;
      return;
    }
    // The game runs in the browser of the member who launched it; this page reaches it through the SDK's data channel.
    if (platform.channel) {
      transport.attach(platform.channel);
      document.body.classList.add('hosted');
    }
    // Who is in the voice channel (the room's own list comes from the host: hud.roomLine).
    channelPeople = (await platform.participants()).map((p) => p.name);
    platform.onParticipantsChange((p) => (channelPeople = p.map((x) => x.name)));
    platformReady = true;
  }
  const me = platform.identity();

  // A plain browser tab has no channel: it needs a name, and a room to join or one to create (the lobby).
  const web = platform.kind === 'web';
  if (web && (!platform.hasName?.() || (!roomCode && !createNext))) {
    showLobby(roomCode);
    return;
  }
  const who = { uid: me.id, name: me.name, ticket: platform.ticket() };
  const reply = await transport.join(
    roomCode ? { ...who, room: roomCode } : createNext ? { ...who, create: true } : { ...who, instance: platform.instanceId() },
  );
  const creating = createNext;
  createNext = false;
  if (!reply.ok) {
    if (web) {
      showLobby(null, creating && reply.error === 'slow-down' ? CREATE_SLOW : ROOM_ERRORS[reply.error ?? ''] ?? `Could not join: ${reply.error ?? 'unknown error'}`);
      return;
    }
    if (reply.error === 'banned' && !roomCode) {
      // The channel's own room: its host removed us.
      kicked();
      return;
    }
    if (creating) {
      kicked(reply.error === 'slow-down' ? CREATE_SLOW : `Could not make a room: ${reply.error ?? 'unknown error'}`);
      return;
    }
    if (roomCode) {
      roomFailed(reply.error ?? 'unknown error');
      return;
    }
    const boot = /** @type {HTMLElement} */ (document.getElementById('boot'));
    boot.textContent = ROOM_ERRORS[reply.error ?? ''] ?? `Could not join: ${reply.error ?? 'unknown error'}`;
    return;
  }
  // Remember the room, so rejoins (a stall, "Play here") come back to it.
  roomInfo = reply.room ?? null;
  // Came in by a code: the channel's own room is a way back.
  viaCode = !!roomCode;
  if (roomInfo) roomCode = roomInfo.code;
  // A browser tab's address names its room, so a refresh (or a copied address) comes back to it.
  if (web && roomInfo) {
    const q = new URLSearchParams(location.search);
    q.set('room', roomInfo.code);
    history.replaceState(null, '', `${location.pathname}?${q}`);
  }
  world.ingest(reply, performance.now());
  transport.start(
    (r, at) => {
      if (!r.ok && r.error === 'replaced') {
        replaced();
        return;
      }
      if (!r.ok && r.error === 'kicked') {
        kicked();
        return;
      }
      if (!r.ok && r.error === 'host-left') {
        hostLeft();
        return;
      }
      if (!r.ok && r.error === 'no-seat') {
        // The server dropped us (a long stall): join again and carry on.
        transport.stop();
        boot();
        return;
      }
      lastReplyAt = at;
      const contig0 = world.contig;
      world.ingest(r, at);
      if (world.contig > contig0) {
        if (lastTickArrival && at - lastTickArrival > STALL_MS && !document.hidden)
          hiccup({ kind: 'stall', ms: Math.round(at - lastTickArrival), ticks: world.contig - contig0, inflight: transport.stats().inFlight });
        lastTickArrival = at;
      }
      if (r.ok) predictor.onReply(r, at);
    },
    () => world.cursor(),
  );

  hud.show();
  hud.hideOverlays();
  // A reload while alive goes straight back into the game.
  if (reply.you.pid && reply.you.alive) {
    myPid = reply.you.pid;
    spawnTick = world.recvTick;
    screen = 'playing';
  } else {
    screen = 'start';
    hud.showStart(me.name);
  }
}

// A hidden page gets no frames and slow timers (a docked Activity, another tab): stop the head so it does not
// run into a wall unseen, and on return jump the clocks to live instead of fast-forwarding through the gap.
document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    if (screen === 'playing' && own && !own.b.stopped && own.b.dir >= 0) stop();
  } else {
    world.offset = NaN;
    predictor.off = NaN;
  }
});

const bound = bindInput(canvas, window, { turn, stop, card });
window.addEventListener('keydown', (e) => {
  sfx.unlock();
  if (e.repeat) return;
  if (e.code === 'Escape') {
    e.preventDefault();
    if (menu.isOpen) menu.close();
    else if (screen !== 'boot') openMenu();
    return;
  }
  // Keys typed in the menu's controls, or in any text field (a name, a code), are theirs.
  if (menu.isOpen) return;
  const t = /** @type {HTMLElement | null} */ (e.target);
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
  if (e.code === 'KeyM') {
    const muted = sfx.toggleMute();
    hud.toast(muted ? 'Sound off (M)' : 'Sound on (M)');
    return;
  }
  // Enter or Space plays (again) from the start screen or the death summary.
  const summaryUp = !(/** @type {HTMLElement} */ (document.getElementById('summary')).hidden);
  const startUp = !(/** @type {HTMLElement} */ (document.getElementById('start')).hidden);
  if ((e.code === 'Enter' || e.code === 'Space') && (((startUp || summaryUp) && (screen === 'start' || screen === 'dead')) || screen === 'watch')) {
    e.preventDefault();
    play();
  }
});
window.addEventListener('pointerdown', () => sfx.unlock(), { passive: true });
pollGamepad = bound.pollGamepad;
canvas.tabIndex = 0;
/** @type {HTMLElement} */ (document.getElementById('play')).addEventListener('click', () => play());
/** @type {HTMLElement} */ (document.getElementById('again')).addEventListener('click', () => play());

// Test and measurement hooks (read-only views; the game does not depend on them).
Object.assign(window, {
  __landio: {
    metrics,
    world,
    transport,
    screen: () => screen,
    room: () => world.game?.code ?? null,
    channelPeople: () => channelPeople,
    // The own head as drawn (predicted when prediction is on), so tests steer what the player sees.
    me: () => {
      const h = myPid ? (lastOwnHead ?? world.headAt(myPid, lastR)) : null;
      const s = world.stats.get(myPid);
      return myPid ? { pid: myPid, alive: !!s?.alive, x: h?.x, y: h?.y, dir: h?.dir, flags: h?.flags ?? 0, land: s?.land ?? 0, kills: s?.kills ?? 0 } : null;
    },
    players: () =>
      [...world.roster.entries()].map(([id, p]) => ({ id, name: p.name, bot: p.bot, ...world.stats.get(id) })),
    head: (/** @type {number} */ id) => (id === myPid && lastOwnHead ? lastOwnHead : world.headAt(id, lastR)),
    predictorActs: () => predictor.acts.map((x) => ({ ...x })),
    predictorClock: () => (own ? own.P : world.recvTick),
    // Draw n frames back to back while panning the camera, for comparing renderer cost (ms per frame).
    benchDraw: (/** @type {number} */ n, /** @type {number} */ pan, /** @type {number | undefined} */ quality) => {
      if (quality) {
        renderer.quality = quality;
        renderer.resize();
      }
      const r = lastR;
      const t0 = performance.now();
      let painted = 0;
      for (let k = 0; k < n; k++) {
        const f = { x: (world.w / 4 + k * pan) % world.w, y: (world.h / 3 + k * pan * 0.5) % world.h };
        renderer.draw(world, r, { follow: f, me: myPid, now: performance.now(), dt: 16, own: null });
        painted += renderer.painted;
      }
      const v = renderer.view ?? { x0: 0, x1: 0, y0: 0, y1: 0 };
      return {
        msPerFrame: +((performance.now() - t0) / n).toFixed(3),
        canvasPx: canvas.width * canvas.height,
        tile: renderer.tile,
        visibleTiles: (v.x1 - v.x0 + 1) * (v.y1 - v.y0 + 1),
        chunkCanvases: renderer.canvases,
        cached: renderer.chunks.size,
        repaintsPerFrame: +(painted / n).toFixed(2),
      };
    },
    predictedPath: () => (own?.path ? [...own.path.entries()].map(([k, v]) => [k, v.x, v.y]) : []),
  },
});

requestAnimationFrame(frame);
boot();
