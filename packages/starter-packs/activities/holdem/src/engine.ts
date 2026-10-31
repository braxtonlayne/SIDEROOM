// The table and the betting state machine. Everything here is pure: each function takes a `Table` and returns a
// new one (or throws a `HoldemError`), so the host can keep one authoritative table and tests can replay hands.
//
// Rules: no-limit Texas Hold'em with play-money chips, blinds, a rotating dealer button, all-ins with side pots,
// minimum raises that only reopen the betting for a full raise, and heads-up blinds (the dealer is the small
// blind and acts first before the flop).

import { cryptoRandom, makeDeck, shuffle, type Card, type RandomInt } from './cards';
import { describe as describeHand, evaluate, type HandRank } from './ranker';

export interface Config {
  smallBlind: number;
  bigBlind: number;
  startingStack: number;
  maxSeats: number;
}

export const DEFAULT_CONFIG: Config = {
  smallBlind: 10,
  bigBlind: 20,
  startingStack: 1000,
  maxSeats: 9,
};

export interface Seat {
  id: string;
  name: string;
  /** Chips behind the player between hands (during a hand the stack lives in `HandPlayer`). */
  chips: number;
  /** Stands up when the current hand ends. */
  leaving?: boolean;
}

export type Street = 'preflop' | 'flop' | 'turn' | 'river';

export interface HandPlayer {
  id: string;
  name: string;
  seat: number;
  /** Chips not yet bet. */
  stack: number;
  /** Chips bet in the current betting round. */
  bet: number;
  /** Chips bet in the whole hand. */
  total: number;
  folded: boolean;
  allIn: boolean;
  /** The table's current bet when this player last acted this round, or -1 if they have not acted yet. */
  actedAt: number;
}

export interface Pot {
  amount: number;
  /** Players who can win it (not folded, put in at least this pot's level). */
  eligible: string[];
}

export interface PotAward extends Pot {
  winners: string[];
}

export interface Reveal {
  cards: [Card, Card];
  label: string;
  best: Card[];
}

export interface HandResult {
  awards: PotAward[];
  /** Chips won per player (including uncalled bets handed back). */
  payouts: Record<string, number>;
  /** Hole cards shown at a showdown with a contested pot; empty when everyone else folded. */
  reveals: Record<string, Reveal>;
  uncontested: boolean;
}

export interface Hand {
  no: number;
  street: Street;
  /** Seat index of the dealer button. */
  button: number;
  smallBlindSeat: number;
  bigBlindSeat: number;
  /** In seat order. */
  players: HandPlayer[];
  board: Card[];
  currentBet: number;
  /** The size of the last full raise (a bet counts as a raise from zero). */
  minRaise: number;
  /** Seat index of the player to act, or null when nobody can (between streets, or the hand is over). */
  toAct: number | null;
  over: boolean;
  result: HandResult | null;
  /** Host-private: the undealt cards and every player's hole cards. Never published. */
  deck: Card[];
  holes: Record<string, [Card, Card]>;
}

export interface Table {
  config: Config;
  /** One entry per seat; null for an empty seat. */
  seats: (Seat | null)[];
  /** Seat index of the last dealer button, -1 before the first hand. */
  button: number;
  handNo: number;
  hand: Hand | null;
  log: string[];
}

export type Action =
  | { type: 'fold' }
  | { type: 'check' }
  | { type: 'call' }
  /** Bet or raise so that the player's bet this round is `to` (all-in when `to` is their whole stack). */
  | { type: 'raise'; to: number }
  | { type: 'allin' };

export type ErrorCode =
  | 'no-hand'
  | 'hand-running'
  | 'not-seated'
  | 'already-seated'
  | 'table-full'
  | 'not-your-turn'
  | 'cannot-check'
  | 'raise-closed'
  | 'raise-too-small'
  | 'raise-too-big'
  | 'bad-amount'
  | 'need-players'
  | 'has-chips'
  | 'bad-config';

export class HoldemError extends Error {
  readonly code: ErrorCode;
  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = 'HoldemError';
    this.code = code;
  }
}

const LOG_LIMIT = 40;

export function createTable(config: Partial<Config> = {}): Table {
  const c = { ...DEFAULT_CONFIG, ...config };
  validateConfig(c);
  return {
    config: c,
    seats: Array.from({ length: c.maxSeats }, () => null),
    button: -1,
    handNo: 0,
    hand: null,
    log: [],
  };
}

export function validateConfig(c: Config): void {
  const ok = (n: number) => Number.isInteger(n) && n > 0;
  if (!ok(c.smallBlind) || !ok(c.bigBlind) || !ok(c.startingStack) || !ok(c.maxSeats))
    throw new HoldemError('bad-config', 'Blinds, stack and seats must be positive whole numbers.');
  if (c.bigBlind < c.smallBlind)
    throw new HoldemError('bad-config', 'The big blind cannot be below the small blind.');
  if (c.maxSeats < 2 || c.maxSeats > 10)
    throw new HoldemError('bad-config', 'A table has two to ten seats.');
  if (c.startingStack < c.bigBlind * 2)
    throw new HoldemError('bad-config', 'The starting stack must cover at least two big blinds.');
}

function clone(t: Table): Table {
  return structuredClone(t);
}

function say(t: Table, line: string): void {
  t.log.push(line);
  if (t.log.length > LOG_LIMIT) t.log.splice(0, t.log.length - LOG_LIMIT);
}

export function handActive(t: Table): boolean {
  return !!t.hand && !t.hand.over;
}

export function seatOf(t: Table, id: string): number {
  return t.seats.findIndex((s) => s?.id === id);
}

function playerOf(h: Hand, id: string): HandPlayer | undefined {
  return h.players.find((p) => p.id === id);
}

// ---------- seating ----------

/** Sit down with a fresh stack. Joins the next hand. */
export function sit(t0: Table, id: string, name: string, seatIndex?: number): Table {
  const t = clone(t0);
  if (seatOf(t, id) >= 0) throw new HoldemError('already-seated', 'You already have a seat.');
  const wanted = seatIndex !== undefined && t.seats[seatIndex] === null ? seatIndex : -1;
  const idx = wanted >= 0 ? wanted : t.seats.findIndex((s) => s === null);
  if (idx < 0) throw new HoldemError('table-full', 'The table is full.');
  t.seats[idx] = { id, name: name.slice(0, 32) || 'Player', chips: t.config.startingStack };
  say(t, `${t.seats[idx]?.name} sits down with ${t.config.startingStack}.`);
  return t;
}

/** Stand up. In a hand the player folds first and leaves when the hand ends. */
export function stand(t0: Table, id: string): Table {
  const t = clone(t0);
  const idx = seatOf(t, id);
  const seat = t.seats[idx];
  if (!seat) throw new HoldemError('not-seated', 'You are not seated.');
  const h = t.hand;
  const p = h && !h.over ? playerOf(h, id) : undefined;
  if (h && p) {
    seat.leaving = true;
    say(t, `${seat.name} stands up.`);
    if (!p.folded && !p.allIn) forceFold(t, h, p);
  } else {
    t.seats[idx] = null;
    say(t, `${seat.name} leaves the table.`);
  }
  return t;
}

/** A busted player (no chips) takes a fresh stack between hands. */
export function rebuy(t0: Table, id: string): Table {
  const t = clone(t0);
  const seat = t.seats[seatOf(t, id)];
  if (!seat) throw new HoldemError('not-seated', 'You are not seated.');
  if (handActive(t) && playerOf(t.hand as Hand, id))
    throw new HoldemError('hand-running', 'Wait for the hand to end.');
  if (seat.chips > 0) throw new HoldemError('has-chips', 'You still have chips.');
  seat.chips = t.config.startingStack;
  say(t, `${seat.name} rebuys for ${seat.chips}.`);
  return t;
}

export function setBlinds(t0: Table, smallBlind: number, bigBlind: number): Table {
  const t = clone(t0);
  if (handActive(t)) throw new HoldemError('hand-running', 'Change the blinds between hands.');
  const c = { ...t.config, smallBlind, bigBlind };
  validateConfig(c);
  t.config = c;
  say(t, `Blinds are now ${smallBlind}/${bigBlind}.`);
  return t;
}

export function canStart(t: Table): boolean {
  return !handActive(t) && t.seats.filter((s) => s && s.chips > 0).length >= 2;
}

// ---------- dealing ----------

function nextFunded(t: Table, after: number, skip = 0): number {
  const n = t.seats.length;
  let seen = 0;
  for (let k = 1; k <= n; k++) {
    const i = (((after + k) % n) + n) % n;
    const s = t.seats[i];
    if (s && s.chips > 0 && !s.leaving) {
      if (seen === skip) return i;
      seen++;
    }
  }
  return -1;
}

export function startHand(t0: Table, rnd: RandomInt = cryptoRandom): Table {
  const t = clone(t0);
  if (handActive(t)) throw new HoldemError('hand-running', 'A hand is already running.');
  // Seats whose players stood up after the last hand are free now.
  t.seats = t.seats.map((s) => (s?.leaving ? null : s));
  const funded = t.seats.map((s, i) => (s && s.chips > 0 ? i : -1)).filter((i) => i >= 0);
  if (funded.length < 2) throw new HoldemError('need-players', 'Two players with chips are needed.');

  const button = nextFunded(t, t.button);
  const headsUp = funded.length === 2;
  const sbSeat = headsUp ? button : nextFunded(t, button);
  const bbSeat = nextFunded(t, sbSeat);

  const players: HandPlayer[] = funded.map((i) => {
    const s = t.seats[i] as Seat;
    return {
      id: s.id,
      name: s.name,
      seat: i,
      stack: s.chips,
      bet: 0,
      total: 0,
      folded: false,
      allIn: false,
      actedAt: -1,
    };
  });

  const deck = shuffle(makeDeck(), rnd);
  const holes: Record<string, [Card, Card]> = {};
  // Deal one card at a time starting left of the button, as at a real table.
  const order = [...players].sort(
    (a, b) => ((a.seat - button + 100) % 100) - ((b.seat - button + 100) % 100),
  );
  const first: Record<string, Card> = {};
  for (const p of order) first[p.id] = deck.pop() as Card;
  for (const p of order) holes[p.id] = [first[p.id] as Card, deck.pop() as Card];

  t.handNo += 1;
  t.button = button;
  const hand: Hand = {
    no: t.handNo,
    street: 'preflop',
    button,
    smallBlindSeat: sbSeat,
    bigBlindSeat: bbSeat,
    players,
    board: [],
    currentBet: t.config.bigBlind,
    minRaise: t.config.bigBlind,
    toAct: null,
    over: false,
    result: null,
    deck,
    holes,
  };
  t.hand = hand;
  say(t, `Hand ${hand.no}: ${t.seats[button]?.name} has the button.`);

  const sb = players.find((p) => p.seat === sbSeat) as HandPlayer;
  const bb = players.find((p) => p.seat === bbSeat) as HandPlayer;
  const sbPaid = commit(sb, t.config.smallBlind);
  const bbPaid = commit(bb, t.config.bigBlind);
  say(t, `${sb.name} posts the small blind ${sbPaid}.`);
  say(t, `${bb.name} posts the big blind ${bbPaid}.`);

  advance(t, hand, bbSeat);
  return t;
}

/** Move up to `amount` chips from the stack into the current bet. Returns what was moved. */
function commit(p: HandPlayer, amount: number): number {
  const put = Math.min(amount, p.stack);
  p.stack -= put;
  p.bet += put;
  p.total += put;
  if (p.stack === 0) p.allIn = true;
  return put;
}

// ---------- betting ----------

function livePlayers(h: Hand): HandPlayer[] {
  return h.players.filter((p) => !p.folded);
}

function needsToAct(h: Hand, p: HandPlayer): boolean {
  if (p.folded || p.allIn) return false;
  const canAct = h.players.filter((x) => !x.folded && !x.allIn);
  // The only player who can still bet has nobody to bet against once their bet is matched.
  if (canAct.length === 1 && p.bet >= h.currentBet) return false;
  return p.actedAt < 0 || p.bet < h.currentBet;
}

function nextActor(h: Hand, afterSeat: number): HandPlayer | undefined {
  const n = h.players.length;
  const start = h.players.findIndex((p) => p.seat > afterSeat);
  const from = start < 0 ? 0 : start;
  for (let k = 0; k < n; k++) {
    const p = h.players[(from + k) % n] as HandPlayer;
    if (needsToAct(h, p)) return p;
  }
  return undefined;
}

export interface Legal {
  /** The player to act can check. */
  canCheck: boolean;
  /** Chips needed to call (capped at the stack). */
  toCall: number;
  canRaise: boolean;
  /** Smallest legal bet-to (the all-in amount when the stack is short of a full raise). */
  minRaiseTo: number;
  /** The player's whole stack as a bet-to. */
  maxRaiseTo: number;
}

/** What `id` may do now; null when it is not their turn. */
export function legalActions(t: Table, id: string): Legal | null {
  const h = t.hand;
  if (!h || h.over) return null;
  const p = playerOf(h, id);
  if (!p || h.toAct !== p.seat) return null;
  return legalFor(h, p);
}

function legalFor(h: Hand, p: HandPlayer): Legal {
  const owed = h.currentBet - p.bet;
  const closed = p.actedAt >= 0 && h.currentBet - p.actedAt < h.minRaise;
  const maxRaiseTo = p.bet + p.stack;
  return {
    canCheck: owed <= 0,
    toCall: Math.max(0, Math.min(owed, p.stack)),
    canRaise: !closed && p.stack > owed,
    minRaiseTo: Math.min(h.currentBet + h.minRaise, maxRaiseTo),
    maxRaiseTo,
  };
}

export function applyAction(t0: Table, id: string, action: Action): Table {
  const t = clone(t0);
  const h = t.hand;
  if (!h || h.over) throw new HoldemError('no-hand', 'There is no hand running.');
  const p = playerOf(h, id);
  if (!p) throw new HoldemError('not-seated', 'You are not in this hand.');
  if (h.toAct !== p.seat) throw new HoldemError('not-your-turn', 'It is not your turn.');
  const legal = legalFor(h, p);

  switch (action.type) {
    case 'fold':
      p.folded = true;
      say(t, `${p.name} folds.`);
      break;
    case 'check':
      if (!legal.canCheck) throw new HoldemError('cannot-check', 'There is a bet to call.');
      p.actedAt = h.currentBet;
      say(t, `${p.name} checks.`);
      break;
    case 'call': {
      if (legal.canCheck) {
        p.actedAt = h.currentBet;
        say(t, `${p.name} checks.`);
        break;
      }
      const paid = commit(p, legal.toCall);
      p.actedAt = h.currentBet;
      say(t, `${p.name} calls ${paid}${p.allIn ? ' and is all in' : ''}.`);
      break;
    }
    case 'allin':
      if (legal.maxRaiseTo <= h.currentBet) {
        const paid = commit(p, legal.toCall);
        p.actedAt = h.currentBet;
        say(t, `${p.name} calls ${paid} and is all in.`);
      } else {
        if (!legal.canRaise)
          throw new HoldemError('raise-closed', 'The betting is not open to you; call or fold.');
        raiseTo(t, h, p, legal.maxRaiseTo);
      }
      break;
    case 'raise': {
      const to = action.to;
      if (!Number.isInteger(to) || to <= 0)
        throw new HoldemError('bad-amount', 'Enter a whole number of chips.');
      if (!legal.canRaise) throw new HoldemError('raise-closed', 'You cannot raise now.');
      if (to > legal.maxRaiseTo) throw new HoldemError('raise-too-big', 'You do not have that many chips.');
      if (to < legal.minRaiseTo) throw new HoldemError('raise-too-small', 'That is below the minimum raise.');
      if (to <= h.currentBet) throw new HoldemError('raise-too-small', 'A raise must exceed the current bet.');
      raiseTo(t, h, p, to);
      break;
    }
  }
  advance(t, h, p.seat);
  return t;
}

function raiseTo(t: Table, h: Hand, p: HandPlayer, to: number): void {
  const wasBet = h.currentBet === 0;
  commit(p, to - p.bet);
  const size = to - h.currentBet;
  if (size >= h.minRaise) h.minRaise = size;
  h.currentBet = to;
  p.actedAt = to;
  const verb = wasBet ? 'bets' : 'raises to';
  say(t, `${p.name} ${verb} ${to}${p.allIn ? ' and is all in' : ''}.`);
}

function forceFold(t: Table, h: Hand, p: HandPlayer): void {
  p.folded = true;
  say(t, `${p.name} folds.`);
  if (h.toAct === p.seat) advance(t, h, p.seat);
  else if (livePlayers(h).length === 1) finishUncontested(t, h);
}

/** Pick the next player to act, or close the round, deal the next street, or end the hand. */
function advance(t: Table, h: Hand, afterSeat: number): void {
  let after = afterSeat;
  for (;;) {
    if (livePlayers(h).length === 1) return finishUncontested(t, h);
    const next = nextActor(h, after);
    if (next) {
      h.toAct = next.seat;
      return;
    }
    // The betting round is complete.
    if (h.street === 'river') return showdown(t, h);
    dealStreet(t, h);
    after = h.button;
  }
}

function dealStreet(t: Table, h: Hand): void {
  const take = (n: number) => {
    for (let i = 0; i < n; i++) h.board.push(h.deck.pop() as Card);
  };
  if (h.street === 'preflop') {
    take(3);
    h.street = 'flop';
  } else if (h.street === 'flop') {
    take(1);
    h.street = 'turn';
  } else {
    take(1);
    h.street = 'river';
  }
  for (const p of h.players) {
    p.bet = 0;
    p.actedAt = -1;
  }
  h.currentBet = 0;
  h.minRaise = t.config.bigBlind;
  h.toAct = null;
  say(t, `The ${h.street}: ${h.board.join(' ')}.`);
}

// ---------- pots and showdown ----------

/** Main pot and side pots from what each player put in. Folded chips stay in the pots they reached. */
export function computePots(players: { id: string; total: number; folded: boolean }[]): Pot[] {
  const levels = [...new Set(players.filter((p) => p.total > 0).map((p) => p.total))].sort(
    (a, b) => a - b,
  );
  const pots: Pot[] = [];
  let prev = 0;
  for (const level of levels) {
    let amount = 0;
    for (const p of players) amount += Math.max(0, Math.min(p.total, level) - prev);
    const eligible = players.filter((p) => !p.folded && p.total >= level).map((p) => p.id);
    prev = level;
    if (amount === 0) continue;
    const last = pots[pots.length - 1];
    if (last && (eligible.length === 0 || sameSet(last.eligible, eligible))) last.amount += amount;
    else pots.push({ amount, eligible });
  }
  return pots;
}

function sameSet(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x) => b.includes(x));
}

export function potTotal(h: Hand): number {
  return h.players.reduce((s, p) => s + p.total, 0);
}

function finishHand(t: Table, h: Hand, result: HandResult): void {
  for (const p of h.players) {
    p.stack += result.payouts[p.id] ?? 0;
    const seat = t.seats[p.seat];
    if (seat && seat.id === p.id) seat.chips = p.stack;
  }
  h.toAct = null;
  h.over = true;
  h.result = result;
  for (const [id, won] of Object.entries(result.payouts)) {
    if (won > 0) {
      const name = playerOf(h, id)?.name ?? id;
      say(t, `${name} wins ${won}.`);
    }
  }
  // Players who asked to stand up leave now.
  t.seats = t.seats.map((s) => (s?.leaving ? null : s));
}

function finishUncontested(t: Table, h: Hand): void {
  const winner = livePlayers(h)[0] as HandPlayer;
  const total = potTotal(h);
  finishHand(t, h, {
    awards: [{ amount: total, eligible: [winner.id], winners: [winner.id] }],
    payouts: { [winner.id]: total },
    reveals: {},
    uncontested: true,
  });
}

function showdown(t: Table, h: Hand): void {
  const pots = computePots(h.players);
  const ranks = new Map<string, HandRank>();
  for (const p of livePlayers(h)) {
    ranks.set(p.id, evaluate([...(h.holes[p.id] as [Card, Card]), ...h.board]));
  }
  const payouts: Record<string, number> = {};
  const awards: PotAward[] = [];
  let contested = false;

  for (const pot of pots) {
    if (pot.eligible.length === 1) {
      // Nobody could match this part of a bet: it goes back.
      const only = pot.eligible[0] as string;
      payouts[only] = (payouts[only] ?? 0) + pot.amount;
      awards.push({ ...pot, winners: [only] });
      continue;
    }
    contested = true;
    let best = -1;
    let win: string[] = [];
    for (const id of pot.eligible) {
      const s = (ranks.get(id) as HandRank).score;
      if (s > best) {
        best = s;
        win = [id];
      } else if (s === best) win.push(id);
    }
    // Split evenly; odd chips go one each to the winners nearest the left of the button.
    const order = [...win].sort((a, b) => distanceFromButton(h, a) - distanceFromButton(h, b));
    const share = Math.floor(pot.amount / order.length);
    let extra = pot.amount - share * order.length;
    for (const id of order) {
      const got = share + (extra > 0 ? 1 : 0);
      if (extra > 0) extra--;
      payouts[id] = (payouts[id] ?? 0) + got;
    }
    awards.push({ ...pot, winners: order });
  }

  const reveals: Record<string, Reveal> = {};
  if (contested) {
    for (const p of livePlayers(h)) {
      const r = ranks.get(p.id) as HandRank;
      reveals[p.id] = { cards: h.holes[p.id] as [Card, Card], label: describeHand(r), best: r.best };
      say(t, `${p.name} shows ${(h.holes[p.id] as [Card, Card]).join(' ')}: ${describeHand(r)}.`);
    }
  }
  finishHand(t, h, { awards, payouts, reveals, uncontested: false });
}

function distanceFromButton(h: Hand, id: string): number {
  const seat = (playerOf(h, id) as HandPlayer).seat;
  // The seat left of the button is first in line; the button itself is last.
  return (seat - h.button + 100) % 100 || 100;
}
