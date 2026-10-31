// @ts-check
// HTTP long polling: the transport that works through today's SIDE ROOM proxy (plain GET and POST under a URL
// mapping; no WebSockets, no streams). Inputs are POSTed the moment they happen; state comes from pipelined
// long polls that the server answers the moment their tick exists.

/** @typedef {import('./transport.js').Transport} Transport */
/** @typedef {import('./transport.js').PollReply} PollReply */

const MAX_IN_FLIGHT = 4;
/** A poll the server holds at most 2 s; give up on it well after that. */
const POLL_TIMEOUT_MS = 4000;

/** @param {{ base?: string, intervalMs?: number }} opt @returns {Transport} */
export function createHttpPoll(opt) {
  const base = opt.base ?? '/api';
  let token = '';
  let seq = 0;
  let bytesIn = 0;
  let polls = 0;
  let rtt = 0;
  let inFlight = 0;
  let running = false;
  /** @type {[number, number, number | null][]} [seq, dir or -1 for stop, tick to apply on] */
  const recent = [];
  /** @type {ReturnType<typeof setInterval> | null} */
  let timer = null;

  /** @param {string} path @param {unknown} body */
  async function post(path, body) {
    const r = await fetch(base + path, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
      cache: 'no-store',
    });
    const text = await r.text();
    bytesIn += text.length;
    return JSON.parse(text);
  }

  return {
    async join(who) {
      const sent = performance.now();
      const r = await post('/join', who);
      // Seed the round trip, so the long-poll pipeline is deep enough from the first tick.
      rtt = performance.now() - sent;
      if (r.ok) token = r.token;
      return r;
    },
    play: () => post('/play', { token }),
    settings: (body) => post('/settings', { token, ...body }),
    host: (body) => post('/host', { token, ...body }),
    input(input) {
      // Each POST repeats the last few actions with their sequence numbers: posts can overtake each other
      // on separate connections, and the server applies every action once, in order.
      seq++;
      recent.push([seq, input.card ?? (input.stop ? -1 : (input.turns?.[0] ?? -1)), input.at ?? null]);
      if (recent.length > 4) recent.shift();
      post('/input', { token, acts: recent }).catch(() => undefined);
      return seq;
    },
    start(onReply, cursor) {
      // Pipelined long polls: each asks "answer once tick N exists" for the next N, so every tick is sent the
      // moment it happens. Enough polls stay in flight to cover one round trip; each normally carries just
      // its own tick (since = N - 1), and falls back to the last complete tick when a reply went missing.
      running = true;
      let lastWant = 0;
      const pump = () => {
        if (!running || !token) return;
        const c = cursor();
        const depth = Math.min(MAX_IN_FLIGHT, Math.max(1, Math.ceil((rtt + 40) / c.tickMs)));
        if (lastWant > c.newest + depth + 1 || lastWant < c.since) lastWant = Math.max(c.since, c.newest); // a new arena, a resync
        while (inFlight < depth) {
          const want = Math.max(lastWant + 1, c.since + 1);
          if (want > c.newest + depth) break;
          lastWant = want;
          const since = want - 1 - c.since > depth ? c.since : want - 1;
          poll(want, since, c);
        }
      };
      /** @param {number} want @param {number} since @param {{rv: number, gv?: number, rdv?: number}} c */
      const poll = async (want, since, c) => {
        inFlight++;
        const sent = performance.now();
        const abort = new AbortController();
        const guard = setTimeout(() => abort.abort(), POLL_TIMEOUT_MS);
        try {
          const r = await fetch(`${base}/state?token=${encodeURIComponent(token)}&since=${since}&rv=${c.rv}&gv=${c.gv ?? 0}&rdv=${c.rdv ?? 0}&want=${want}`, {
            cache: 'no-store',
            signal: abort.signal,
          });
          const text = await r.text();
          const at = performance.now();
          bytesIn += text.length;
          polls++;
          const reply = JSON.parse(text);
          const net = Math.max(0, at - sent - (reply.held ?? 0));
          rtt = rtt ? rtt * 0.8 + net * 0.2 : net;
          onReply(reply, at);
        } catch {
          // A lost or stuck poll: the next one asks again from the last complete tick.
        } finally {
          clearTimeout(guard);
          inFlight--;
          pump();
        }
      };
      // A slow heartbeat keeps the pipeline full if replies stall.
      timer = setInterval(pump, 250);
      pump();
    },
    stop() {
      running = false;
      if (timer) clearInterval(timer);
      timer = null;
    },
    report(body) {
      if (token) post('/diag', { token, ...body }).catch(() => undefined);
    },
    async leave() {
      this.stop();
      if (token) await post('/leave', { token }).catch(() => undefined);
    },
    stats: () => ({ bytesIn, polls, rttMs: Math.round(rtt), inFlight }),
  };
}
