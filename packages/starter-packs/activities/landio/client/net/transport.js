// @ts-check
// The only way the game talks to its server. Phase 1 ships one implementation, HTTP polling through the
// Activity's URL mapping (`./http-poll.js`); a WebSocket or streaming transport can replace it without the
// game noticing, as long as it keeps this shape.

import { createHttpPoll } from './http-poll.js';

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
 *   offers: {cards: string[], until: number}[], fx: Record<string, number>} | null}} you
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

/** @param {{ base?: string, intervalMs?: number }} [opt] @returns {Transport} */
export function createTransport(opt = {}) {
  return createHttpPoll(opt);
}
