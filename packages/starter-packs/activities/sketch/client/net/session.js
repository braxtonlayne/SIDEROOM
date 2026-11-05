// @ts-check
// One page's side of a game (ACT-SKS-001): the host's page runs the game (host/engine.js) and talks to it in
// process; every other page (members and guests) sends its player's actions to the host over the data channel and
// keeps the newest public state and ink it hears. The page above (main.js) only sees `Session`.
//
// Host migration (D-388, `host_handoff`): when the platform names a new host, that page resumes the game from the
// last `host:st` it heard and the ink it holds; every page says hello again, and the drawer repeats its prompt to
// the new host. Without a handoff, the host leaving ends the game for everyone ("host-left").

import { INK_KEY, ST_KEY, SketchHost } from '../../host/engine.js';

/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('../../host/engine.js').PublicState} PublicState */
/** @typedef {import('../../shared/rules.js').InkOp} InkOp */

export const HEARTBEAT_MS = 2000;
/** A guest whose host left waits this long for the platform to name a new one before giving up. */
export const HANDOFF_GRACE_MS = 6000;

/**
 * @typedef {object} Session
 * @property {() => PublicState | null} state
 * @property {() => InkOp[]} ink                      the turn's ink as the host has it, plus my own ops in flight
 * @property {() => { prompt: string, choices: string[], n: number }} secret   mine when I draw
 * @property {(cb: (what: string, data?: any) => void) => () => void} on   'state' | 'ink' | 'close' | 'hit' | 'ended' | 'secret'
 * @property {(msg: object) => void} send
 * @property {(op: object) => void} draw              the drawer's ink op (applied locally at once)
 * @property {boolean} isHost
 * @property {SketchHost | null} engine
 * @property {string} self
 * @property {(name: string) => void} hello
 * @property {() => void} leave
 */

/**
 * @param {{ channel: Channel, name: string, guest?: boolean, now?: () => number,
 *   engine?: Partial<import('../../host/engine.js').EngineOptions> }} opt
 * @returns {Session}
 */
export function createSession(opt) {
  const ch = opt.channel;
  const clock = opt.now ?? (() => Date.now());
  const self = ch.self.id;
  /** @type {SketchHost | null} */
  let engine = null;
  /** @type {PublicState | null} */
  let st = null;
  let stAt = 0;
  /** The turn the ink belongs to, and its ops. */
  let inkTurn = -1;
  /** @type {InkOp[]} */
  let ops = [];
  /** My own ink ops the host has not echoed yet (the drawer's prediction). @type {InkOp[]} */
  let pending = [];
  let q = 0;
  const secret = { prompt: '', choices: /** @type {string[]} */ ([]), n: -1 };
  let myName = opt.name;
  /** @type {Set<(what: string, data?: any) => void>} */
  const cbs = new Set();
  /** @type {(() => void)[]} */
  const unsubs = [];
  /** @type {ReturnType<typeof setTimeout> | null} */
  let hostGone = null;
  let lastSync = -Infinity;
  const emit = (/** @type {string} */ what, /** @type {any} */ data) => cbs.forEach((cb) => cb(what, data));

  /** @param {object} msg */
  function toHost(msg) {
    if (engine) engine.handle(self, msg);
    else ch.toHost(msg);
  }

  /** @param {PublicState} value */
  function onSt(value) {
    if (!value || typeof value !== 'object' || value.v !== 1) return;
    st = value;
    stAt = clock();
    if (secret.n !== value.turnNo && value.drawer !== self) {
      secret.prompt = '';
      secret.choices = [];
    }
    emit('state', st);
  }

  /** @param {{ n: number, at: number, ops: InkOp[] }} v */
  function onInk(v) {
    if (!v || !Array.isArray(v.ops) || typeof v.at !== 'number') return;
    if (v.n !== inkTurn) {
      if (v.at !== 0) return askSync();
      inkTurn = v.n;
      ops = [];
      pending = [];
    }
    if (v.at === 0) ops = [...v.ops];
    else if (v.at === ops.length) ops.push(...v.ops);
    else if (v.at < ops.length) ops = ops.slice(0, v.at).concat(v.ops);
    else return askSync();
    // My ops the host has taken are no longer mine to predict.
    const taken = ops.reduce((m, o) => Math.max(m, o.q ?? 0), 0);
    pending = pending.filter((o) => (o.q ?? 0) > taken);
    emit('ink', ops);
  }

  function askSync() {
    const t = clock();
    if (t - lastSync < 1000) return;
    lastSync = t;
    toHost({ t: 'inksync' });
  }

  /** What the host addressed to me. @param {any} m */
  function onReply(m) {
    if (!m || typeof m !== 'object') return;
    if (m.t === 'ink') onInk(m);
    else if (m.t === 'choices') {
      secret.n = m.n;
      secret.choices = Array.isArray(m.list) ? m.list.map(String) : [];
      secret.prompt = '';
      emit('secret', secret);
    } else if (m.t === 'secret') {
      secret.n = m.n;
      secret.prompt = String(m.prompt ?? '');
      secret.choices = [];
      emit('secret', secret);
    } else if (m.t === 'close') emit('close', m.text);
    else if (m.t === 'hit') emit('hit', m);
    else if (m.t === 'err') emit('error', m.error);
  }

  function engineOptions() {
    return {
      instance: opt.engine?.instance ?? ch.hostId,
      hostId: self,
      send: (/** @type {string} */ to, /** @type {object} */ msg) => (to === self ? queueMicrotask(() => onReply(msg)) : ch.sendTo(to, msg)),
      publish: (/** @type {string} */ key, /** @type {any} */ value) => {
        ch.publish(key, value);
        if (key === ST_KEY) onSt(value);
        else if (key === INK_KEY) onInk(value);
      },
      ...(opt.now ? { now: opt.now } : {}),
      ...opt.engine,
    };
  }

  /** @param {SketchHost} eng */
  function wireHost(eng) {
    engine = eng;
    unsubs.push(ch.onInput((from, msg) => eng.handle(from, msg, { guest: !ch.members().some((m) => m.id === from) })));
    unsubs.push(ch.onMembers((list) => eng.setMembers(list)));
    eng.start();
  }

  function wireGuest() {
    unsubs.push(ch.onReply(onReply));
    unsubs.push(
      ch.onState((key, value) => {
        if (key === ST_KEY) onSt(value);
        else if (key === INK_KEY) onInk(value);
      }),
    );
    unsubs.push(
      ch.onMembers((list) => {
        if (engine || list.some((m) => m.id === ch.hostId) || hostGone || !ch.onHostChange) {
          if (!engine && !ch.onHostChange && !list.some((m) => m.id === ch.hostId)) emit('ended', 'host-left');
          return;
        }
        hostGone = setTimeout(() => {
          hostGone = null;
          if (!engine && !ch.members().some((m) => m.id === ch.hostId)) emit('ended', 'host-left');
        }, HANDOFF_GRACE_MS);
      }),
    );
  }

  if (ch.isHost) wireHost(new SketchHost(engineOptions()));
  else wireGuest();

  const beat = setInterval(() => {
    if (!engine) ch.toHost({ t: 'hb' });
  }, HEARTBEAT_MS);

  ch.onHostChange?.((host, previous) => {
    if (hostGone) clearTimeout(hostGone);
    hostGone = null;
    if (host !== self) {
      if (engine) return;
      toHost({ t: 'hello', name: myName, guest: opt.guest === true });
      if (st?.drawer === self && secret.prompt) toHost({ t: 'reprompt', prompt: secret.prompt });
      return;
    }
    if (engine || !st) return;
    for (const u of unsubs.splice(0)) u();
    const mine = st.drawer === self ? secret.prompt : undefined;
    wireHost(SketchHost.resume(st, ops, { ...engineOptions(), leaving: previous, prompt: mine }));
    engine?.handle(self, { t: 'hello', name: myName });
  });

  return {
    state: () => {
      if (!st) return null;
      // Time left counts down between frames.
      return { ...st, left: Math.max(0, st.left - (clock() - stAt)) };
    },
    ink: () => (pending.length ? [...ops, ...pending] : ops),
    secret: () => secret,
    on(cb) {
      cbs.add(cb);
      return () => cbs.delete(cb);
    },
    send: toHost,
    draw(op) {
      const mine = /** @type {InkOp} */ ({ ...op, q: ++q });
      pending.push(mine);
      emit('ink', ops);
      toHost({ t: 'ink', op: mine });
    },
    get isHost() {
      return engine !== null;
    },
    get engine() {
      return engine;
    },
    self,
    hello(name) {
      myName = name;
      toHost({ t: 'hello', name, guest: opt.guest === true });
    },
    leave() {
      clearInterval(beat);
      if (hostGone) clearTimeout(hostGone);
      if (!engine) ch.toHost({ t: 'bye' });
      for (const u of unsubs.splice(0)) u();
      engine?.close();
      engine = null;
    },
  };
}
