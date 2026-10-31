// The dealer. Only the instance's host runs this: it owns the authoritative table (deck included), applies
// players' inputs, publishes the public view as shared state and answers each player with their own hole cards.
//
// The sender of an input comes from the SDK (`e.from`), never from the payload, so a player can act only
// for themselves. Not cheat-proof by design: every participant holds the room key, so a modified client
// could read what the host sends to someone else. It is a friends' table, not a casino.

import { cryptoRandom, type RandomInt } from './cards';
import {
  applyAction,
  canStart,
  createTable,
  handActive,
  HoldemError,
  legalActions,
  potTotal,
  computePots,
  rebuy,
  setBlinds,
  sit,
  stand,
  startHand,
  type Config,
  type Hand,
  type Table,
} from './engine';
import {
  PROTOCOL,
  TABLE_KEY,
  decodeClientMsg,
  encode,
  type ClientMsg,
  type PublicHand,
  type PublicTable,
} from './protocol';
import type { Transport } from './transport';

/** The part of the table every participant may see. */
export function publicView(
  t: Table,
  rev: number,
  turn: { seconds: number; deadline: number | null } = { seconds: DEFAULT_TURN_SECONDS, deadline: null },
): PublicTable {
  const h = t.hand;
  let hand: PublicHand | null = null;
  if (h) {
    const actor = h.toAct === null ? undefined : h.players.find((p) => p.seat === h.toAct);
    hand = {
      no: h.no,
      street: h.street,
      over: h.over,
      button: h.button,
      smallBlindSeat: h.smallBlindSeat,
      bigBlindSeat: h.bigBlindSeat,
      board: [...h.board],
      currentBet: h.currentBet,
      minRaise: h.minRaise,
      pot: potTotal(h),
      pots: computePots(h.players),
      players: h.players.map((p) => ({
        id: p.id,
        name: p.name,
        seat: p.seat,
        stack: p.stack,
        bet: p.bet,
        total: p.total,
        folded: p.folded,
        allIn: p.allIn,
      })),
      toAct: actor?.id ?? null,
      legal: actor && !h.over ? legalActions(t, actor.id) : null,
      result: h.result ? structuredClone(h.result) : null,
    };
  }
  return {
    v: PROTOCOL,
    rev,
    config: { ...t.config },
    seats: t.seats.map((s) =>
      s ? { id: s.id, name: s.name, chips: s.chips, leaving: !!s.leaving } : null,
    ),
    button: t.button,
    handNo: t.handNo,
    hand,
    log: t.log.slice(-30),
    canStart: canStart(t),
    turnSeconds: turn.seconds,
    turnDeadline: turn.deadline,
  };
}

export const DEFAULT_TURN_SECONDS = 60;
export const MIN_TURN_SECONDS = 15;
export const MAX_TURN_SECONDS = 120;

export interface HostOptions {
  turnSeconds?: number;
  /** Clock seam for tests. */
  now?: () => number;
  config?: Partial<Config>;
  rnd?: RandomInt;
}

export class HostController {
  private table: Table;
  private rev = 0;
  private readonly rnd: RandomInt;
  private off: (() => void) | null = null;
  private turnSeconds: number;
  private turnKey = '';
  private turnDeadline: number | null = null;
  private turnTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly now: () => number;

  constructor(
    private readonly tp: Transport,
    opts: HostOptions = {},
  ) {
    this.table = createTable(opts.config);
    this.rnd = opts.rnd ?? cryptoRandom;
    this.now = opts.now ?? (() => Date.now());
    this.turnSeconds = clampTurn(opts.turnSeconds ?? DEFAULT_TURN_SECONDS);
  }

  /** Start listening to inputs and publish the empty table. */
  start(): void {
    if (this.tp.self !== this.tp.host) throw new Error('Only the host runs the dealer.');
    this.off?.();
    this.off = this.tp.onInput((e) => {
      if (e.reply) return; // replies are for the player side
      const msg = decodeClientMsg(e.data);
      if (msg) this.handle(e.from, msg);
    });
    this.publish();
  }

  stop(): void {
    this.off?.();
    this.off = null;
    this.clearTurn();
  }

  get current(): Table {
    return this.table;
  }

  // ---- inputs from players ----

  handle(from: string, msg: ClientMsg): void {
    try {
      switch (msg.t) {
        case 'sit':
          this.table = sit(this.table, from, msg.name, msg.seat);
          break;
        case 'stand':
          this.table = stand(this.table, from);
          break;
        case 'rebuy':
          this.table = rebuy(this.table, from);
          break;
        case 'act': {
          const h = this.table.hand;
          if (!h || h.over || h.no !== msg.hand)
            throw new HoldemError('no-hand', 'That hand is over.');
          this.table = applyAction(this.table, from, msg.action);
          break;
        }
        case 'sync':
          this.sendHole(from);
          return;
      }
    } catch (e) {
      if (e instanceof HoldemError) {
        this.tell(from, { t: 'error', code: e.code, message: e.message });
        return;
      }
      throw e;
    }
    this.publish();
  }

  // ---- host controls (called by the host's own UI) ----

  /** Deal the next hand. */
  deal(): void {
    try {
      this.table = startHand(this.table, this.rnd);
    } catch (e) {
      if (e instanceof HoldemError) {
        this.tell(this.tp.self, { t: 'error', code: e.code, message: e.message });
        return;
      }
      throw e;
    }
    // Hole cards go out before the table so a player's screen never shows a hand they cannot see.
    const h = this.table.hand as Hand;
    for (const p of h.players) this.sendHole(p.id);
    this.publish();
  }

  setBlinds(small: number, big: number): void {
    try {
      this.table = setBlinds(this.table, small, big);
    } catch (e) {
      if (e instanceof HoldemError) {
        this.tell(this.tp.self, { t: 'error', code: e.code, message: e.message });
        return;
      }
      throw e;
    }
    this.publish();
  }

  /**
   * Remove a seated player: they fold at once if they are in a hand, their seat and stack go back to the bank
   * when the hand ends (at once between hands). The host cannot remove itself.
   */
  kick(id: string): void {
    if (id === this.tp.self) return;
    if (this.table.seats.some((s) => s?.id === id)) this.handle(id, { t: 'stand' });
  }

  /** The members still in the instance: any seated player who is not among them has left and is folded now. */
  syncParticipants(present: Iterable<string>): void {
    const here = new Set(present);
    for (const s of this.table.seats) if (s && s.id !== this.tp.self && !here.has(s.id)) this.kick(s.id);
  }

  /** Seconds a player has to act (15 to 120). */
  setTurnSeconds(n: number): void {
    if (!Number.isInteger(n) || n < MIN_TURN_SECONDS || n > MAX_TURN_SECONDS) {
      this.tell(this.tp.self, {
        t: 'error',
        code: 'bad-config',
        message: `The turn timer is ${MIN_TURN_SECONDS} to ${MAX_TURN_SECONDS} seconds.`,
      });
      return;
    }
    this.turnSeconds = n;
    this.turnKey = ''; // re-arm the running turn with the new length
    this.publish();
  }

  /** The player to act ran out of time: check if that is free, else fold. */
  private expire(key: string): void {
    this.turnTimer = null;
    const h = this.table.hand;
    if (key !== this.turnKey || !h || h.over || h.toAct === null) return;
    const p = h.players.find((x) => x.seat === h.toAct);
    if (!p) return;
    const legal = legalActions(this.table, p.id);
    this.table = applyAction(this.table, p.id, legal?.canCheck ? { type: 'check' } : { type: 'fold' });
    this.table.log.push(`${p.name} ran out of time.`);
    this.publish();
  }

  private clearTurn(): void {
    if (this.turnTimer) clearTimeout(this.turnTimer);
    this.turnTimer = null;
    this.turnKey = '';
    this.turnDeadline = null;
  }

  private armTurn(): void {
    const h = this.table.hand;
    if (!h || h.over || h.toAct === null) return this.clearTurn();
    const key = `${h.no}|${h.street}|${h.toAct}|${potTotal(h)}|${this.turnSeconds}`;
    if (key === this.turnKey) return;
    this.clearTurn();
    this.turnKey = key;
    this.turnDeadline = this.now() + this.turnSeconds * 1000;
    this.turnTimer = setTimeout(() => this.expire(key), this.turnSeconds * 1000);
  }

  // ---- output ----

  private publish(): void {
    this.rev += 1;
    this.armTurn();
    this.tp.setState({
      [TABLE_KEY]: publicView(this.table, this.rev, {
        seconds: this.turnSeconds,
        deadline: this.turnDeadline,
      }),
    });
  }

  private sendHole(id: string): void {
    const h = this.table.hand;
    if (!h) return;
    const cards = h.holes[id];
    if (!cards) return;
    this.tell(id, { t: 'hole', hand: h.no, cards });
  }

  private tell(member: string, msg: Parameters<typeof encode>[0]): void {
    this.tp.reply(member, encode(msg));
  }

  /** True while a hand is being played. */
  get handRunning(): boolean {
    return handActive(this.table);
  }
}

function clampTurn(n: number): number {
  return Math.max(MIN_TURN_SECONDS, Math.min(MAX_TURN_SECONDS, Math.round(n)));
}
