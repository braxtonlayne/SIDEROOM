// What travels between the table's host and the players.
//
// Shared state (`sdk.state`): the host publishes one document under the `host:table` key. It is the public
// view of the table: seats, stacks, bets, the board, whose turn it is and what each action may be. It never
// holds an undealt card or anybody's hole cards (those appear only in a showdown's `reveals`).
//
// Inputs (`sdk.input`): players send `ClientMsg` to the host; the host answers one member at a time with a
// `HostMsg` (the hole cards go to their owner this way and nobody else is addressed).

import { isCard, type Card } from './cards';
import type { Action, Config, HandResult, Legal, Pot, Street } from './engine';

export const TABLE_KEY = 'host:table';
export const PROTOCOL = 1;

export interface PublicSeat {
  id: string;
  name: string;
  chips: number;
  leaving: boolean;
}

export interface PublicPlayer {
  id: string;
  name: string;
  seat: number;
  stack: number;
  bet: number;
  total: number;
  folded: boolean;
  allIn: boolean;
}

export interface PublicHand {
  no: number;
  street: Street;
  over: boolean;
  button: number;
  smallBlindSeat: number;
  bigBlindSeat: number;
  board: Card[];
  currentBet: number;
  minRaise: number;
  pot: number;
  pots: Pot[];
  players: PublicPlayer[];
  /** Member id of the player to act. */
  toAct: string | null;
  /** What the player to act may do (everyone can see it; it holds no secret). */
  legal: Legal | null;
  result: HandResult | null;
}

export interface PublicTable {
  v: typeof PROTOCOL;
  /** Bumped on every publish so a client can tell a fresh document. */
  rev: number;
  config: Config;
  seats: (PublicSeat | null)[];
  button: number;
  handNo: number;
  hand: PublicHand | null;
  log: string[];
  /** Whether the host can start a hand now (two players with chips, none running). */
  canStart: boolean;
}

export type ClientMsg =
  | { t: 'sit'; name: string; seat?: number }
  | { t: 'stand' }
  | { t: 'rebuy' }
  | { t: 'act'; hand: number; action: Action }
  | { t: 'sync' };

export type HostMsg =
  | { t: 'hole'; hand: number; cards: [Card, Card] }
  | { t: 'error'; code: string; message: string };

const enc = new TextEncoder();
const dec = new TextDecoder();

export function encode(msg: ClientMsg | HostMsg): Uint8Array {
  return enc.encode(JSON.stringify(msg));
}

function parseJson(bytes: Uint8Array): Record<string, unknown> | null {
  try {
    const v: unknown = JSON.parse(dec.decode(bytes));
    return typeof v === 'object' && v !== null && !Array.isArray(v)
      ? (v as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function parseAction(raw: unknown): Action | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const a = raw as Record<string, unknown>;
  switch (a.type) {
    case 'fold':
    case 'check':
    case 'call':
    case 'allin':
      return { type: a.type };
    case 'raise':
      return typeof a.to === 'number' && Number.isFinite(a.to) ? { type: 'raise', to: a.to } : null;
    default:
      return null;
  }
}

/** Validate a message a player sent to the host. Anything malformed is dropped (null). */
export function decodeClientMsg(bytes: Uint8Array): ClientMsg | null {
  const m = parseJson(bytes);
  if (!m) return null;
  switch (m.t) {
    case 'sit':
      if (typeof m.name !== 'string') return null;
      return {
        t: 'sit',
        name: m.name,
        ...(typeof m.seat === 'number' && Number.isInteger(m.seat) ? { seat: m.seat } : {}),
      };
    case 'stand':
    case 'rebuy':
    case 'sync':
      return { t: m.t };
    case 'act': {
      const action = parseAction(m.action);
      return action && typeof m.hand === 'number' ? { t: 'act', hand: m.hand, action } : null;
    }
    default:
      return null;
  }
}

/** Validate a message the host sent to this member. */
export function decodeHostMsg(bytes: Uint8Array): HostMsg | null {
  const m = parseJson(bytes);
  if (!m) return null;
  if (m.t === 'hole') {
    const c = m.cards;
    if (
      typeof m.hand === 'number' &&
      Array.isArray(c) &&
      c.length === 2 &&
      isCard(c[0]) &&
      isCard(c[1])
    )
      return { t: 'hole', hand: m.hand, cards: [c[0], c[1]] };
    return null;
  }
  if (m.t === 'error' && typeof m.code === 'string' && typeof m.message === 'string')
    return { t: 'error', code: m.code, message: m.message };
  return null;
}
