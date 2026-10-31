// @ts-check
// The transport for Land.io as a SIDE ROOM activity: the same `Transport` shape as http-poll.js, but there is no
// server. The member who launched the activity runs the game (host/engine.js) in their page; everyone else
// reaches it through the Embedded App SDK's data channel (net/channel.js).
//
//   host's page   engine <-> this transport, in process (no network); every tick's frame is also published
//   guest's page  join -> { t: 'hello' } to the host, answered by a full snapshot; then the host's tick frames
//                 arrive through the shared state; inputs go to the host as they happen
//
// The page above (main.js) cannot tell the difference: replies have the shape of http-poll's `PollReply`.

import { LandioHost } from '../../host/engine.js';
import { TICK_KEY } from './channel.js';

/** @typedef {import('./transport.js').Transport} Transport */
/** @typedef {import('./transport.js').PollReply} PollReply */
/** @typedef {import('./channel.js').Channel} Channel */

/** A request (play, settings, host) that gets no answer in this long fails like a lost network. */
const REQUEST_TIMEOUT_MS = 6000;
/** How long a hello waits for the host's welcome before it asks again, and how many times. */
const HELLO_RETRY_MS = 2000;
const HELLO_TRIES = 4;
const HEARTBEAT_MS = 2000;
/** A page that is missing ticks asks for a fresh snapshot at most this often. */
const RESYNC_MIN_MS = 1000;
/** Ticks the page may lag before it asks (a gap the next frames did not fill). */
const GAP_TICKS = 6;

/**
 * Turn one broadcast frame into the reply the page expects, for member `uid`.
 * @param {any} f the frame (host/engine.js `sendFrame`)
 * @param {string} uid
 * @param {any} you what the host last told this member about themselves
 * @returns {PollReply}
 */
export function frameToReply(f, uid, you) {
  /** @type {any} */
  const r = { ok: true, now: f.now, ms: 0, hz: f.hz, recs: f.recs };
  if (f.roster) {
    r.roster = f.roster;
    r.rv = f.rv;
  }
  if (f.stats) r.stats = f.stats;
  if (f.round) {
    r.round = f.round;
    r.rdv = f.rdv;
  }
  if (f.game) {
    r.game = f.game;
    r.gv = f.gv;
  }
  const mine = (f.y ?? []).find((/** @type {any[]} */ e) => e[0] === uid);
  if (you) r.you = { ...you, ack: mine ? [mine[1], mine[2]] : you.ack, tick: f.now };
  return r;
}

/**
 * @param {{ channel: Channel, now?: () => number, engine?: Partial<import('../../host/engine.js').EngineOptions> }} opt
 * @returns {Transport & { engine: LandioHost | null }}
 */
export function createSdkTransport(opt) {
  const ch = opt.channel;
  const clock = opt.now ?? (() => performance.now());
  const self = ch.self;
  /** @type {LandioHost | null} */
  let engine = null;
  /** @type {((r: PollReply, at: number) => void) | null} */
  let onReply = null;
  /** @type {(() => {since: number, rv: number, gv?: number, rdv?: number, newest: number, tickMs: number}) | null} */
  let cursor = null;
  /** @type {any} */
  let you = null;
  let running = false;
  let seq = 0;
  let bytesIn = 0;
  let frames = 0;
  let rtt = 0;
  let reqId = 0;
  let lastResync = -Infinity;
  /** @type {[number, number, number | null][]} */
  const recent = [];
  /** @type {Map<number, (res: any) => void>} */
  const waiting = new Map();
  /** @type {(() => void)[]} */
  const unsubs = [];
  /** @type {ReturnType<typeof setInterval> | null} */
  let beat = null;
  /** The welcome (or error) a pending join waits for. @type {((m: any) => void) | null} */
  let helloWaiter = null;
  let wired = false;

  /** @param {object} msg a message from this page to the host */
  function toHost(msg) {
    if (engine) engine.handle(self.id, self.name, msg);
    else ch.toHost(msg);
  }

  /** The host's answers to me (addressed), from the wire or from the engine in this page. @param {any} m */
  function fromHost(m) {
    if (!m || typeof m !== 'object') return;
    if (m.t === 'welcome' || (m.t === 'err' && helloWaiter)) {
      helloWaiter?.(m);
      if (m.t === 'welcome') {
        you = m.you;
        // A resync while running: hand the snapshot to the page like any reply.
        if (running && onReply) onReply(welcomeToReply(m), clock());
      }
    } else if (m.t === 'res') {
      waiting.get(m.id)?.(m.res);
      waiting.delete(m.id);
    } else if (m.t === 'you') {
      you = m.you;
    } else if (m.t === 'pong') {
      const net = Math.max(0, clock() - Number(m.at));
      if (Number.isFinite(net)) rtt = rtt ? rtt * 0.8 + net * 0.2 : net;
    } else if (m.t === 'err' && running && onReply) {
      onReply({ ok: false, error: String(m.error) }, clock());
    }
  }

  /** @param {any} m a welcome @returns {PollReply} */
  function welcomeToReply(m) {
    const { t: _t, host: _h, max: _m, ...rest } = m;
    return { ...rest, ok: true };
  }

  /** @param {any} f a frame @param {number} size its JSON length */
  function onFrame(f, size) {
    if (!running || !onReply || !f || !Array.isArray(f.recs)) return;
    frames++;
    bytesIn += size;
    const at = clock();
    const r = frameToReply(f, self.id, you);
    onReply(r, at);
    // The host's own copy is the original: nothing to catch up on.
    if (engine) return;
    const c = cursor?.();
    if (!c) return;
    // A frame that did not carry what its versions say (the one that did was lost), or a gap that stays open:
    // ask the host for a fresh snapshot.
    const missing =
      (f.rv !== undefined && !f.roster && f.rv !== c.rv) ||
      (f.rdv !== undefined && !f.round && f.rdv !== (c.rdv ?? 0)) ||
      (f.gv !== undefined && !f.game && f.gv !== (c.gv ?? 0)) ||
      c.newest - c.since > GAP_TICKS;
    if (missing && at - lastResync > RESYNC_MIN_MS) {
      lastResync = at;
      toHost({ t: 'resync' });
    }
  }

  /** @param {string} type @param {object} [body] @returns {Promise<any>} */
  function request(type, body) {
    const id = ++reqId;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        waiting.delete(id);
        resolve({ ok: false, error: 'network' });
      }, REQUEST_TIMEOUT_MS);
      waiting.set(id, (res) => {
        clearTimeout(timer);
        resolve(res);
      });
      toHost({ t: type, id, body });
    });
  }

  return {
    get engine() {
      return engine;
    },

    async join(who) {
      const sent = clock();
      if (wired) {
        // A rejoin (the host dropped our seat): the same wiring, a new hello.
      } else if (ch.isHost) {
        wired = true;
        // The game runs here. Members reach it through the channel; this page talks to it directly.
        engine = new LandioHost({
          instance: opt.engine?.instance ?? ch.hostId,
          hostId: self.id,
          now: clock,
          send: (to, msg) => (to === self.id ? queueMicrotask(() => fromHost(msg)) : ch.sendTo(to, msg)),
          publish: (frame) => {
            ch.publish(TICK_KEY, frame);
            onFrame(frame, 0);
          },
          ...opt.engine,
        });
        const eng = engine;
        unsubs.push(ch.onInput((from, msg) => eng.handle(from, ch.members().find((m) => m.id === from)?.name ?? '', msg)));
        unsubs.push(ch.onMembers((list) => eng.setMembers(list)));
        eng.setMembers(ch.members());
        eng.start();
      } else {
        wired = true;
        unsubs.push(ch.onReply(fromHost));
        unsubs.push(
          ch.onState((key, value) => {
            if (key === TICK_KEY) onFrame(value, JSON.stringify(value).length);
          }),
        );
        // The host going away ends the game: there is nobody left to run it.
        unsubs.push(
          ch.onMembers((list) => {
            if (!list.some((m) => m.id === ch.hostId) && running && onReply) onReply({ ok: false, error: 'host-left' }, clock());
          }),
        );
      }
      const first = await new Promise((resolve) => {
        let tries = 0;
        const ask = () => {
          if (++tries > HELLO_TRIES) {
            helloWaiter = null;
            resolve({ t: 'err', error: 'no-host' });
            return;
          }
          toHost({ t: 'hello', name: who.name });
          retry = setTimeout(ask, HELLO_RETRY_MS);
        };
        /** @type {ReturnType<typeof setTimeout>} */
        let retry;
        helloWaiter = (m) => {
          clearTimeout(retry);
          helloWaiter = null;
          resolve(m);
        };
        ask();
      });
      if (first.t === 'err') return { ok: false, error: first.error };
      rtt = ch.isHost ? 0 : clock() - sent;
      return welcomeToReply(first);
    },

    play: () => request('play'),
    settings: (body) => request('settings', body),
    host: (body) => request('host', body),

    input(input) {
      seq++;
      recent.push([seq, input.card ?? (input.stop ? -1 : (input.turns?.[0] ?? -1)), input.at ?? null]);
      if (recent.length > 4) recent.shift();
      toHost({ t: 'in', acts: recent });
      return seq;
    },

    start(cb, cur) {
      running = true;
      onReply = cb;
      cursor = cur;
      beat = setInterval(() => toHost({ t: 'hb', at: clock() }), HEARTBEAT_MS);
    },

    stop() {
      running = false;
      if (beat) clearInterval(beat);
      beat = null;
    },

    report(body) {
      toHost({ t: 'diag', ...body });
    },

    async leave() {
      this.stop();
      if (!engine) toHost({ t: 'bye' });
      for (const u of unsubs.splice(0)) u();
      wired = false;
      if (engine) {
        // The host leaving closes the game for everyone.
        engine.close();
        engine = null;
      }
    },

    stats: () => ({ bytesIn, polls: frames, rttMs: Math.round(rtt), inFlight: 0 }),
  };
}
