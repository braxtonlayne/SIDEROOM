// @ts-check
// The only way the game talks to the game. Two implementations keep this shape: HTTP polling through a URL
// mapping to a game server (`./http-poll.js`, the standalone game's; kept for reference and for a server
// build), and the one this activity uses, `./sdk-transport.js`: the member who launched the activity runs the
// game and everyone else reaches it through the SIDE ROOM data channel. `createTransport()` returns a transport
// that starts unattached; `attach(channel)` (after the platform is ready) binds it to the SDK transport.

import { createHttpPoll } from './http-poll.js';
import { createSdkTransport } from './sdk-transport.js';

/**
 * @typedef {object} PollReply           see server/arena.js `poll`
 * @property {boolean} ok
 * @property {string} [error]
 * @property {number} now                newest server tick
 * @property {number} ms                 ms since that tick on the server
 * @property {number} hz
 * @property {number} [held]             ms the server held a long poll before answering
 * @property {any[]} [recs]              tick records after `since`
 * @property {any} [full]                full snapshot instead of records
 * @property {number} [rv]
 * @property {{id: number, name: string, bot: boolean, look: number, team?: number}[]} [roster]
 * @property {number[]} [stats]          [id, land, kills, alive, score, ...] (every other tick)
 * @property {{n: number, phase: 'play'|'over', start: number, end: number, next: number, mode: string, map: string,
 *   md: Record<string, number | number[] | string[]>, result?: import('../../shared/types.js').RoundResult}} [round]
 * @property {GameInfo} [game]           the arena's settings and host (only when changed: see gv)
 * @property {number} [gv]               version of `game`
 * @property {number} [rdv]              version of `round` (sent only when changed)
 * @property {{pid: number, alive: boolean, death: any, ack: [number, number], pct: number, tick: number, spawn?: number, queued?: boolean, host?: boolean,
 *   hand: {items: [string, number][], active: [string, number, number] | null, passives: [string, number][],
 *   offers: {cards: string[], until: number, keep?: boolean}[], fx: Record<string, number>} | null}} you
 * @property {string} [token]            join only
 * @property {{code: string, instance: boolean, url?: string}} [room]   join only: the room joined (D-59)
 * @property {import('../../shared/types.js').Config} [cfg] join only
 */

/**
 * @typedef {object} GameSettings
 * @property {string} [mode]
 * @property {string} [map]
 * @property {number} [size]       tiles per side
 * @property {number} [bots]       bots fill the arena up to this many players
 * @property {'easy'|'normal'|'hard'} [skill]
 * @property {number} [roundMin]   minutes (0 = the mode's own)
 * @property {boolean} [cards]
 * @property {number} [shield]     spawn protection, seconds
 * @property {boolean} [hostOnly]  only the host may change settings (the host alone may set this)
 */

/**
 * @typedef {GameSettings & {host: string, hostPid: number, code: string, people: string[], seats?: [string, string, number][]}} GameInfo   code: the room's (D-59); people: who is seated; seats: [ref, name, is host] (D-61)
 */

/**
 * @typedef {object} Transport
 * @property {(who: {instance?: string, room?: string, create?: boolean, uid: string, name: string, ticket?: string | null}) => Promise<PollReply>} join
 *   D-59: `room` joins that room by code, `create` opens a new one, else `instance` joins its channel's room
 * @property {() => Promise<{ok: boolean, pid?: number, tick?: number, error?: string, next?: number}>} play   tick = server tick the player spawned after; error 'wait': in at the next round (tick `next`)
 * @property {(body: GameSettings) => Promise<{ok: boolean, error?: string}>} settings   change the arena's settings (mode, map or size start a new round)
 * @property {(body: {action: 'restart' | 'end' | 'host' | 'kick', to?: string}) => Promise<{ok: boolean, error?: string}>} host   the host's buttons (to: a seat's ref from GameInfo.seats)
 * @property {(input: {turns?: number[], stop?: boolean, card?: number, at?: number}) => number} input   fire and forget; returns its seq. `card` = a card action (shared/cards.js ACT), `at` = the server tick to apply on
 * @property {(onReply: (r: PollReply, arrivedAt: number) => void, cursor: () => {since: number, rv: number, gv?: number, rdv?: number, newest: number, tickMs: number}) => void} start
 * @property {() => void} stop
 * @property {(body: object) => void} report   fire and forget: client diagnostics for the server's metrics
 * @property {() => Promise<void>} leave
 * @property {() => {bytesIn: number, polls: number, rttMs: number, inFlight: number}} stats
 */

/**
 * D-388: how often the host publishes a handoff snapshot when the platform can move the host (a channel with
 * `onHostChange`, i.e. SIDE ROOM; Land.io declares `host_handoff`): every 5 s at 7 ticks a second. A 100x100 snapshot
 * is under 40 KB, about 4-8 KB/s per member; host/handoff.js has the numbers.
 */
export const HANDOFF_SNAPSHOT_TICKS = 35;

/**
 * The SDK transport for `channel`. When the platform can move the host, snapshots are on and a host change is handed
 * to the transport's `handoff.hostChanged` (the successor resumes from the newest snapshot, everyone else says hello
 * to it).
 * @param {import('./channel.js').Channel} channel
 * @param {Omit<Parameters<typeof createSdkTransport>[0], 'channel'>} [extra]   tests: the clock and engine options
 */
export function sdkTransportFor(channel, extra = {}) {
  const handoff = typeof channel.onHostChange === 'function';
  const t = createSdkTransport({ snapshotEveryTicks: handoff ? HANDOFF_SNAPSHOT_TICKS : 0, ...extra, channel });
  channel.onHostChange?.((host, previous) => {
    void t.handoff.hostChanged(host, previous).catch(() => undefined);
  });
  return t;
}

/**
 * @param {{ base?: string, intervalMs?: number, channel?: import('./channel.js').Channel }} [opt]
 * @returns {Transport & { attach(channel: import('./channel.js').Channel): void, impl(): Transport }}
 */
export function createTransport(opt = {}) {
  /** @type {Transport} */
  let current = opt.channel ? sdkTransportFor(opt.channel) : createHttpPoll(opt);
  return {
    attach(channel) {
      current = sdkTransportFor(channel);
    },
    impl: () => current,
    join: (who) => current.join(who),
    play: () => current.play(),
    settings: (body) => current.settings(body),
    host: (body) => current.host(body),
    input: (input) => current.input(input),
    start: (onReply, cursor) => current.start(onReply, cursor),
    stop: () => current.stop(),
    report: (body) => current.report(body),
    leave: () => current.leave(),
    stats: () => current.stats(),
  };
}
