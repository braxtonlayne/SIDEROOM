// @ts-check
// The game of Sketch & Slander (ACT-SKS-001), run by the host: the member who launched the Activity, or its
// successor after a handoff (D-388). Every other page (members and guests on a /play/<token> link) sends what its
// player does and draws what the host publishes:
//
//   host:st   the public state: phase, players and scores, the turn, the hint mask, time left, the feed
//   host:ink  the newest ink ops of the turn ({ n, at, ops }: ops `at`.. of turn `n`); a page that missed some asks
//             for all of them ({ t: 'inksync' }) and gets an addressed { t: 'ink', n, at: 0, ops }
//
// Secrets never go into the shared state: the prompt and the drawer's choices are sent to the drawer alone, and are
// picked with the host's own randomness (not the public seed), so nobody can work them out from host:st. Guild
// packs stay in the host's page: only their names and sizes are public.
//
// `host:st` holds everything a successor needs to carry on (resume()): the drawing is in host:ink, which every page
// already has, and the drawer tells a new host its prompt again ({ t: 'reprompt' }).

import { BUILT_IN_PROMPTS } from '../shared/prompts.js';
import { judge, letterPositions, maskPrompt } from '../shared/match.js';
import {
  DEFAULT_SETTINGS,
  DRAWER_POINTS,
  DRAW_CHOICES,
  LIMITS,
  ROUND_CHOICES,
  cleanGuess,
  cleanInkOp,
  cleanName,
  guessPoints,
  rng,
  shuffle,
} from '../shared/rules.js';

export const ST_KEY = 'st';
export const INK_KEY = 'ink';
/** How often the host republishes the state while a timer runs (time left), and checks its timers. */
export const TICK_MS = 250;
const PUBLISH_EVERY_MS = 1000;
/** A page that has not been heard from in this long is away (its turn is skipped). */
export const AWAY_MS = 10_000;
/** After a handoff in a drawing turn, the drawer has this long to tell the new host its prompt. */
export const REPROMPT_MS = 4000;
/** Ink ops are batched into one host:ink write at most this often. */
const INK_FLUSH_MS = 80;

/**
 * @typedef {{ id: string, name: string, score: number, guest: boolean, away: boolean, left: boolean }} Player
 * @typedef {{ k: 'guess' | 'hit' | 'join' | 'left' | 'word' | 'skip' | 'round', who?: string, text?: string, n?: number }} FeedLine
 * @typedef {{
 *   v: 1, instance: string, seed: number, draws: number, phase: 'lobby' | 'pick' | 'draw' | 'reveal' | 'end',
 *   settings: Settings, players: Player[], order: string[], round: number, turn: number,
 *   turnNo: number, drawer: string | null, left: number, total: number, mask: string, shown: number[],
 *   guessed: string[], turnScores: Record<string, number>, word: string, feed: FeedLine[],
 *   packs: { name: string, count: number }[], host: string, ver: number }} PublicState
 * @typedef {{ rounds: number, drawSecs: number, pickSecs: number, revealSecs: number, builtIn: boolean }} Settings
 * @typedef {{ name: string, prompts: string[] }} Pack
 * @typedef {{ instance?: string, hostId: string, now?: () => number, secretRandom?: () => number, seed?: number,
 *   send: (to: string, msg: object) => void, publish: (key: string, value: object) => void }} EngineOptions
 */

/** The host's own randomness for secrets: crypto where there is one. */
function cryptoRandom() {
  const c = /** @type {any} */ (globalThis).crypto;
  if (c?.getRandomValues) {
    const a = new Uint32Array(1);
    c.getRandomValues(a);
    return a[0] / 4294967296;
  }
  return Math.random();
}

export class SketchHost {
  /** @param {EngineOptions} opt */
  constructor(opt) {
    this.opt = opt;
    this.now = opt.now ?? (() => Date.now());
    this.secretRandom = opt.secretRandom ?? cryptoRandom;
    /** @type {PublicState} */
    this.state = {
      v: 1,
      instance: opt.instance ?? 'local',
      seed: (opt.seed ?? Math.floor(this.secretRandom() * 2 ** 31)) >>> 0,
      draws: 0,
      phase: 'lobby',
      settings: { ...DEFAULT_SETTINGS },
      players: [],
      order: [],
      round: 0,
      turn: 0,
      turnNo: 0,
      drawer: null,
      left: 0,
      total: 0,
      mask: '',
      shown: [],
      guessed: [],
      turnScores: {},
      word: '',
      feed: [],
      packs: [],
      host: opt.hostId,
      ver: 0,
    };
    /** Phase deadline on this host's clock. */
    this.endsAt = 0;
    /** The secret prompt of the turn, and the drawer's choices while picking. */
    this.prompt = '';
    /** @type {string[]} */
    this.choices = [];
    /** @type {Pack[]} */
    this.packs = [];
    /** Prompts already played in this game (lower case), so a game does not repeat one. */
    this.used = new Set();
    /** @type {import('../shared/rules.js').InkOp[]} */
    this.ink = [];
    this.inkPoints = 0;
    this.inkFlushedAt = 0;
    this.inkPublished = 0;
    /** @type {Map<string, number>} last time each page was heard from */
    this.seen = new Map();
    /** @type {Map<string, number>} */
    this.lastGuess = new Map();
    this.lastPublish = 0;
    /** @type {ReturnType<typeof setInterval> | null} */
    this.timer = null;
    this.waitingReprompt = 0;
  }

  // ---- lifecycle ---------------------------------------------------------------------------------------------

  start() {
    if (this.timer) return;
    this.timer = setInterval(() => this.tick(), TICK_MS);
    this.publishState();
    this.publishInk(true);
  }

  close() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  /** Members in the call (from the platform): a member who left the call is away at once. @param {{ id: string }[]} list */
  setMembers(list) {
    const ids = new Set(list.map((m) => m.id));
    let changed = false;
    for (const p of this.state.players) {
      if (!p.guest && !ids.has(p.id) && !p.left && p.id !== this.opt.hostId) {
        this.markLeft(p);
        changed = true;
      }
    }
    if (changed) this.afterChange();
  }

  // ---- messages ----------------------------------------------------------------------------------------------

  /**
   * One message from a page (the host's own page too).
   * @param {string} from the sender's id (the data channel's, never one the message names)
   * @param {any} msg
   * @param {{ guest?: boolean }} [meta]
   */
  handle(from, msg, meta = {}) {
    if (!msg || typeof msg !== 'object' || typeof msg.t !== 'string') return;
    const t = this.now();
    this.seen.set(from, t);
    const me = this.player(from);
    if (me?.away && !me.left) {
      me.away = false;
      this.bump();
    }
    switch (msg.t) {
      case 'hello':
        return this.hello(from, msg, meta);
      case 'hb':
        return;
      case 'bye':
        if (me && !me.left) {
          this.markLeft(me);
          this.afterChange();
        }
        return;
      case 'name':
        if (me && this.state.phase === 'lobby') {
          me.name = this.uniqueName(cleanName(msg.name), from);
          this.bump();
        }
        return;
      case 'settings':
        if (from === this.opt.hostId && this.state.phase === 'lobby') this.applySettings(msg);
        return;
      case 'start':
        if (from === this.opt.hostId) this.startGame();
        return;
      case 'again':
        if (from === this.opt.hostId && this.state.phase === 'end') this.toLobby();
        return;
      case 'skip':
        if (from === this.opt.hostId && (this.state.phase === 'pick' || this.state.phase === 'draw')) this.endTurn('skip');
        return;
      case 'pick':
        if (from === this.state.drawer && this.state.phase === 'pick') this.pick(Number(msg.i));
        return;
      case 'reprompt':
        if (from === this.state.drawer && this.waitingReprompt && typeof msg.prompt === 'string') {
          this.prompt = String(msg.prompt).slice(0, 64);
          this.waitingReprompt = 0;
          this.sendSecret();
        }
        return;
      case 'ink':
        if (from === this.state.drawer && this.state.phase === 'draw') this.addInk(msg.op);
        return;
      case 'inksync':
        this.opt.send(from, { t: 'ink', n: this.state.turnNo, at: 0, ops: this.ink });
        return;
      case 'guess':
        if (me && this.state.phase === 'draw') this.guess(me, msg.text);
        return;
      default:
    }
  }

  /** @param {string} id */
  player(id) {
    return this.state.players.find((p) => p.id === id);
  }

  /** @param {string} from @param {any} msg @param {{ guest?: boolean }} meta */
  hello(from, msg, meta) {
    let p = this.player(from);
    const name = cleanName(msg.name);
    if (!p) {
      const seated = this.state.players.filter((x) => !x.left).length;
      if (seated >= LIMITS.players) {
        this.opt.send(from, { t: 'err', error: 'full' });
        return;
      }
      p = { id: from, name: this.uniqueName(name, from), score: 0, guest: meta.guest === true || msg.guest === true, away: false, left: false };
      this.state.players.push(p);
      this.feed({ k: 'join', who: p.name });
      // A player who arrives mid-round draws at the end of it.
      if (this.state.phase !== 'lobby' && this.state.phase !== 'end' && !this.state.order.includes(from)) this.state.order.push(from);
    } else if (p.left) {
      p.left = false;
      p.away = false;
      this.feed({ k: 'join', who: p.name });
      if (this.state.phase !== 'lobby' && this.state.phase !== 'end' && !this.state.order.includes(from)) this.state.order.push(from);
    }
    this.bump();
    this.opt.send(from, { t: 'welcome', you: from, host: this.opt.hostId });
    if (from === this.state.drawer) this.sendSecret();
    this.opt.send(from, { t: 'ink', n: this.state.turnNo, at: 0, ops: this.ink });
  }

  /** @param {string} name @param {string} id */
  uniqueName(name, id) {
    const taken = new Set(this.state.players.filter((p) => p.id !== id && !p.left).map((p) => p.name.toLowerCase()));
    if (!taken.has(name.toLowerCase())) return name;
    for (let i = 2; ; i++) {
      const n = `${name.slice(0, LIMITS.nameChars - 3)} ${i}`;
      if (!taken.has(n.toLowerCase())) return n;
    }
  }

  /** @param {any} msg */
  applySettings(msg) {
    const s = this.state.settings;
    if (ROUND_CHOICES.includes(msg.rounds)) s.rounds = msg.rounds;
    if (DRAW_CHOICES.includes(msg.drawSecs)) s.drawSecs = msg.drawSecs;
    if (typeof msg.builtIn === 'boolean') s.builtIn = msg.builtIn || this.packs.length === 0;
    this.bump();
  }

  // ---- packs (host page only) ---------------------------------------------------------------------------------

  /** Add a validated guild pack (shared/pack.js). Its prompts stay in this page. @param {Pack} pack */
  addPack(pack) {
    const known = this.packs.findIndex((p) => p.name === pack.name);
    if (known >= 0) this.packs.splice(known, 1);
    this.packs.push({ name: pack.name, prompts: [...pack.prompts] });
    this.state.packs = this.packs.map((p) => ({ name: p.name, count: p.prompts.length }));
    this.bump();
  }

  /** @param {string} name */
  removePack(name) {
    this.packs = this.packs.filter((p) => p.name !== name);
    this.state.packs = this.packs.map((p) => ({ name: p.name, count: p.prompts.length }));
    if (this.packs.length === 0) this.state.settings.builtIn = true;
    this.bump();
  }

  /** Every prompt the game may use now, minus the ones already played. */
  pool() {
    /** @type {string[]} */
    const all = [];
    if (this.state.settings.builtIn || this.packs.length === 0) all.push(...BUILT_IN_PROMPTS);
    for (const p of this.packs) all.push(...p.prompts);
    const fresh = all.filter((p) => !this.used.has(p.toLowerCase()));
    return fresh.length >= 3 ? fresh : all;
  }

  // ---- the game -----------------------------------------------------------------------------------------------

  /** Players who can take part now. */
  active() {
    return this.state.players.filter((p) => !p.left && !p.away);
  }

  startGame() {
    if (this.state.phase !== 'lobby' || this.active().length < 2) return;
    for (const p of this.state.players) p.score = 0;
    this.state.players = this.state.players.filter((p) => !p.left);
    this.used.clear();
    this.state.round = 0;
    this.nextRound();
  }

  toLobby() {
    this.state.phase = 'lobby';
    this.state.round = 0;
    this.state.drawer = null;
    this.state.order = [];
    this.state.players = this.state.players.filter((p) => !p.left);
    for (const p of this.state.players) p.score = 0;
    this.clearTurn();
    this.bump();
  }

  nextRound() {
    const s = this.state;
    if (s.round >= s.settings.rounds) return this.endGame();
    s.round++;
    // The order is public and comes from the seed, so a successor computes the same one.
    s.order = shuffle(this.active().map((p) => p.id), rng(s.seed + s.round * 7919));
    s.turn = -1;
    this.feed({ k: 'round', n: s.round });
    this.nextTurn();
  }

  nextTurn() {
    const s = this.state;
    for (;;) {
      s.turn++;
      if (s.turn >= s.order.length) return this.nextRound();
      const p = this.player(s.order[s.turn]);
      if (p && !p.left && !p.away) break;
    }
    if (this.active().length < 2) return this.endGame();
    s.turnNo++;
    s.drawer = s.order[s.turn];
    this.clearTurn();
    s.phase = 'pick';
    this.setTimer(s.settings.pickSecs);
    this.choices = this.deal(3);
    this.sendSecret();
    this.bump();
    this.publishInk(true);
  }

  /** `n` prompts from the pool, chosen with the host's secret randomness. @param {number} n */
  deal(n) {
    const pool = this.pool();
    /** @type {string[]} */
    const out = [];
    for (let tries = 0; out.length < n && tries < 50; tries++) {
      const p = pool[Math.floor(this.secretRandom() * pool.length)];
      if (p && !out.includes(p)) out.push(p);
    }
    return out;
  }

  clearTurn() {
    const s = this.state;
    s.guessed = [];
    s.turnScores = {};
    s.mask = '';
    s.shown = [];
    s.word = '';
    this.prompt = '';
    this.choices = [];
    this.ink = [];
    this.inkPoints = 0;
    this.inkPublished = 0;
    this.waitingReprompt = 0;
  }

  /** @param {number} i */
  pick(i) {
    const choice = this.choices[Number.isInteger(i) && i >= 0 ? i : 0] ?? this.choices[0];
    if (!choice) return this.endTurn('skip');
    this.prompt = choice;
    this.used.add(choice.toLowerCase());
    this.state.draws++;
    this.state.phase = 'draw';
    this.state.mask = maskPrompt(choice);
    this.state.shown = [];
    this.setTimer(this.state.settings.drawSecs);
    this.choices = [];
    this.sendSecret();
    this.bump();
  }

  /** The drawer's secret: the choices while picking, the prompt while drawing. */
  sendSecret() {
    const d = this.state.drawer;
    if (!d) return;
    if (this.state.phase === 'pick') this.opt.send(d, { t: 'choices', n: this.state.turnNo, list: this.choices, left: this.endsAt - this.now() });
    else if (this.state.phase === 'draw' && this.prompt) this.opt.send(d, { t: 'secret', n: this.state.turnNo, prompt: this.prompt });
  }

  /** @param {any} raw */
  addInk(raw) {
    const op = cleanInkOp(raw);
    if (!op || this.ink.length >= LIMITS.opsPerTurn) return;
    if (op.k === 'p') {
      if (this.inkPoints + op.pts.length / 2 > LIMITS.pointsPerTurn) return;
      this.inkPoints += op.pts.length / 2;
    }
    this.ink.push(op);
    this.publishInk(false);
  }

  /** @param {Player} me @param {unknown} text */
  guess(me, text) {
    const s = this.state;
    if (me.id === s.drawer || s.guessed.includes(me.id) || !this.prompt) return;
    const t = this.now();
    if (t - (this.lastGuess.get(me.id) ?? -Infinity) < LIMITS.guessGapMs) return;
    this.lastGuess.set(me.id, t);
    const g = cleanGuess(text);
    if (!g) return;
    const verdict = judge(g, this.prompt);
    if (verdict === 'hit') {
      const pts = guessPoints(this.endsAt - t, s.total, s.guessed.length);
      s.guessed.push(me.id);
      me.score += pts;
      s.turnScores[me.id] = pts;
      const drawer = s.drawer ? this.player(s.drawer) : undefined;
      if (drawer) {
        drawer.score += DRAWER_POINTS;
        s.turnScores[drawer.id] = (s.turnScores[drawer.id] ?? 0) + DRAWER_POINTS;
      }
      this.feed({ k: 'hit', who: me.name });
      this.opt.send(me.id, { t: 'hit', n: s.turnNo, prompt: this.prompt, pts });
      const guessers = this.active().filter((p) => p.id !== s.drawer);
      if (guessers.every((p) => s.guessed.includes(p.id))) return this.endTurn('all');
      this.bump();
      return;
    }
    // A wrong guess is everyone's to read (it is the game); a close one is also told to the guesser alone.
    this.feed({ k: 'guess', who: me.name, text: g });
    if (verdict === 'close') this.opt.send(me.id, { t: 'close', text: g });
    this.bump();
  }

  /** @param {'time' | 'all' | 'skip' | 'gone'} why */
  endTurn(why) {
    const s = this.state;
    if (s.phase !== 'pick' && s.phase !== 'draw') return;
    s.word = this.prompt;
    this.feed(this.prompt ? { k: 'word', text: this.prompt } : { k: 'skip' });
    s.phase = 'reveal';
    s.mask = this.prompt ? maskPrompt(this.prompt, letterPositions(this.prompt)) : '';
    this.waitingReprompt = 0;
    this.setTimer(s.settings.revealSecs);
    void why;
    this.bump();
  }

  endGame() {
    const s = this.state;
    s.phase = 'end';
    s.drawer = null;
    this.clearTurn();
    this.setTimer(0);
    this.bump();
    this.publishInk(true);
  }

  /** @param {Player} p */
  markLeft(p) {
    p.left = true;
    p.away = true;
    this.feed({ k: 'left', who: p.name });
    if (p.id === this.state.drawer) this.endTurn('gone');
  }

  /** @param {number} secs */
  setTimer(secs) {
    this.state.total = secs * 1000;
    this.endsAt = this.now() + secs * 1000;
    this.state.left = secs * 1000;
  }

  /** Timers, hints, players who went quiet. Called every TICK_MS (tests call it with their own clock). */
  tick() {
    const s = this.state;
    const t = this.now();
    for (const p of s.players) {
      if (p.left || p.away || p.id === this.opt.hostId) continue;
      const last = this.seen.get(p.id);
      if (last !== undefined && t - last > AWAY_MS) {
        p.away = true;
        this.bump();
        if (p.id === s.drawer) this.endTurn('gone');
      }
    }
    if (this.waitingReprompt && t >= this.waitingReprompt) {
      this.waitingReprompt = 0;
      this.endTurn('skip');
    }
    if (s.phase === 'draw' && this.prompt) {
      // Hints: one letter at half time and one at three quarters, never more than a third of the letters.
      const done = s.total > 0 ? 1 - (this.endsAt - t) / s.total : 0;
      const want = done >= 0.75 ? 2 : done >= 0.5 ? 1 : 0;
      const letters = letterPositions(this.prompt);
      if (s.shown.length < want && s.shown.length < Math.floor(letters.length / 3)) {
        const hidden = letters.filter((i) => !s.shown.includes(i));
        s.shown = [...s.shown, hidden[Math.floor(this.secretRandom() * hidden.length)]].sort((a, b) => a - b);
        s.mask = maskPrompt(this.prompt, s.shown);
        this.bump();
      }
    }
    if ((s.phase === 'pick' || s.phase === 'draw' || s.phase === 'reveal') && t >= this.endsAt) {
      if (s.phase === 'pick') this.pick(0);
      else if (s.phase === 'draw') this.endTurn('time');
      else this.nextTurn();
      return;
    }
    if (this.ink.length > this.inkPublished && t - this.inkFlushedAt >= INK_FLUSH_MS) this.publishInk(false);
    if (t - this.lastPublish >= PUBLISH_EVERY_MS && s.phase !== 'lobby' && s.phase !== 'end') this.publishState();
  }

  // ---- publishing ---------------------------------------------------------------------------------------------

  /** @param {FeedLine} line */
  feed(line) {
    this.state.feed.push(line);
    if (this.state.feed.length > LIMITS.feed) this.state.feed.splice(0, this.state.feed.length - LIMITS.feed);
  }

  bump() {
    this.state.ver++;
    this.publishState();
  }

  afterChange() {
    const s = this.state;
    if (s.phase === 'draw' && s.drawer) {
      const guessers = this.active().filter((p) => p.id !== s.drawer);
      if (guessers.length === 0) return this.endTurn('gone');
      if (guessers.every((p) => s.guessed.includes(p.id)) && s.guessed.length > 0) return this.endTurn('all');
    }
    this.bump();
  }

  publishState() {
    const t = this.now();
    this.lastPublish = t;
    this.state.left = Math.max(0, this.endsAt - t);
    this.opt.publish(ST_KEY, this.view());
  }

  /** The public state as published (a copy). @returns {PublicState} */
  view() {
    return JSON.parse(JSON.stringify(this.state));
  }

  /** @param {boolean} full */
  publishInk(full) {
    const at = full ? 0 : this.inkPublished;
    const ops = this.ink.slice(at);
    this.inkPublished = this.ink.length;
    this.inkFlushedAt = this.now();
    this.opt.publish(INK_KEY, { n: this.state.turnNo, at, ops });
  }

  // ---- handoff (D-388) ----------------------------------------------------------------------------------------

  /**
   * A new host carries on from the last public state it heard and the turn's ink. The prompt comes back from the
   * drawer (or is `prompt` when this page is the drawer); a drawing turn whose drawer does not say it in
   * REPROMPT_MS is skipped.
   * @param {PublicState} st @param {import('../shared/rules.js').InkOp[]} ink
   * @param {EngineOptions & { leaving?: string, prompt?: string }} opt
   */
  static resume(st, ink, opt) {
    const h = new SketchHost({ ...opt, seed: st.seed, instance: st.instance });
    const now = h.now();
    h.state = { ...JSON.parse(JSON.stringify(st)), host: opt.hostId, packs: [] };
    if (h.state.packs.length === 0 && !h.state.settings.builtIn) h.state.settings.builtIn = true;
    h.endsAt = now + Math.max(0, st.left);
    h.ink = [...ink];
    h.inkPublished = h.ink.length;
    h.inkPoints = ink.reduce((n, op) => n + (op.k === 'p' ? op.pts.length / 2 : 0), 0);
    for (const p of h.state.players) h.seen.set(p.id, now);
    if (st.word) h.used.add(st.word.toLowerCase());
    const old = opt.leaving ? h.player(opt.leaving) : undefined;
    if (old && !old.left) h.markLeft(old);
    const s = h.state;
    if (s.phase === 'pick') {
      h.choices = h.deal(3);
      h.sendSecret();
    } else if (s.phase === 'draw') {
      if (opt.prompt) h.prompt = opt.prompt;
      else h.waitingReprompt = now + REPROMPT_MS;
    }
    h.bump();
    return h;
  }
}
