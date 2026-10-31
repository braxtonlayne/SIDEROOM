import { describe, expect, it } from 'vitest';
import { makeDeck, seededRandom, type Card } from './cards';
import {
  applyAction,
  computePots,
  createTable,
  handActive,
  HoldemError,
  legalActions,
  potTotal,
  rebuy,
  setBlinds,
  sit,
  stand,
  startHand,
  type Action,
  type Hand,
  type Table,
} from './engine';

const rnd = () => seededRandom(1234);

function seat(ids: string[], chips?: number[], cfg = {}): Table {
  let t = createTable({ smallBlind: 10, bigBlind: 20, startingStack: 1000, ...cfg });
  for (const id of ids) t = sit(t, id, id.toUpperCase());
  if (chips) {
    t = structuredClone(t);
    ids.forEach((_, i) => {
      (t.seats[i] as { chips: number }).chips = chips[i] as number;
    });
  }
  return t;
}

/** Fix the hole cards and the cards that will be dealt to the board. */
function rig(t: Table, holes: Record<string, [Card, Card]>, board: Card[]): Table {
  const c = structuredClone(t);
  const h = c.hand as Hand;
  for (const [id, cards] of Object.entries(holes)) h.holes[id] = cards;
  const used = new Set<string>([...Object.values(holes).flat(), ...board]);
  const rest = makeDeck().filter((x) => !used.has(x));
  h.deck = [...rest, ...[...board].reverse()];
  return c;
}

function act(t: Table, id: string, a: Action): Table {
  return applyAction(t, id, a);
}
function toActId(t: Table): string | null {
  const h = t.hand;
  if (!h || h.toAct === null) return null;
  return h.players.find((p) => p.seat === h.toAct)?.id ?? null;
}
function chipsTotal(t: Table): number {
  const inHand = t.hand && !t.hand.over ? t.hand.players.reduce((s, p) => s + p.stack + p.total, 0) : 0;
  const seated = t.hand && !t.hand.over ? 0 : t.seats.reduce((s, x) => s + (x?.chips ?? 0), 0);
  return inHand + seated;
}
function expectCode(fn: () => unknown, code: string) {
  try {
    fn();
  } catch (e) {
    expect(e).toBeInstanceOf(HoldemError);
    expect((e as HoldemError).code).toBe(code);
    return;
  }
  throw new Error(`expected HoldemError ${code}`);
}
function stacks(t: Table): Record<string, number> {
  return Object.fromEntries(t.seats.filter(Boolean).map((s) => [s!.id, s!.chips]));
}

describe('seating and starting a hand', () => {
  it('seats players, refuses duplicates and a full table', () => {
    let t = createTable({ maxSeats: 2 });
    t = sit(t, 'a', 'Ann');
    expectCode(() => sit(t, 'a', 'Ann'), 'already-seated');
    t = sit(t, 'b', 'Bob');
    expectCode(() => sit(t, 'c', 'Cy'), 'table-full');
  });

  it('needs two funded players', () => {
    expectCode(() => startHand(seat(['a'])), 'need-players');
  });

  it('three-handed: button, blinds and first to act', () => {
    const t = startHand(seat(['a', 'b', 'c']), rnd());
    const h = t.hand as Hand;
    expect(h.button).toBe(0);
    expect(h.smallBlindSeat).toBe(1);
    expect(h.bigBlindSeat).toBe(2);
    expect(h.players.map((p) => [p.id, p.bet])).toEqual([
      ['a', 0],
      ['b', 10],
      ['c', 20],
    ]);
    expect(toActId(t)).toBe('a');
    expect(h.holes.a).toHaveLength(2);
    expect(new Set(Object.values(h.holes).flat()).size).toBe(6);
    expect(potTotal(h)).toBe(30);
  });

  it('heads-up: the dealer is the small blind and acts first before the flop, last after', () => {
    let t = startHand(seat(['a', 'b']), rnd());
    let h = t.hand as Hand;
    expect(h.button).toBe(0);
    expect(h.smallBlindSeat).toBe(0);
    expect(h.bigBlindSeat).toBe(1);
    expect(toActId(t)).toBe('a');
    t = act(t, 'a', { type: 'call' });
    expect(toActId(t)).toBe('b'); // the big blind has the option
    t = act(t, 'b', { type: 'check' });
    h = t.hand as Hand;
    expect(h.street).toBe('flop');
    expect(toActId(t)).toBe('b'); // after the flop the non-dealer acts first
  });

  it('the dealer button rotates and skips busted seats', () => {
    let t = seat(['a', 'b', 'c']);
    t = startHand(t, rnd());
    expect((t.hand as Hand).button).toBe(0);
    t = act(t, 'a', { type: 'fold' });
    t = act(t, 'b', { type: 'fold' });
    expect(t.hand?.over).toBe(true);
    t = startHand(t, rnd());
    expect((t.hand as Hand).button).toBe(1);
    // Seat 2 goes broke: the button skips it.
    t = structuredClone(t);
    t.hand = { ...(t.hand as Hand), over: true };
    (t.seats[2] as { chips: number }).chips = 0;
    t = startHand(t, rnd());
    expect((t.hand as Hand).button).toBe(0);
    expect((t.hand as Hand).players.map((p) => p.id)).toEqual(['a', 'b']);
  });

  it('refuses a second hand while one runs', () => {
    const t = startHand(seat(['a', 'b']), rnd());
    expectCode(() => startHand(t, rnd()), 'hand-running');
  });
});

describe('the betting round', () => {
  it('enforces the turn, checking and call amounts', () => {
    let t = startHand(seat(['a', 'b', 'c']), rnd());
    expectCode(() => act(t, 'b', { type: 'fold' }), 'not-your-turn');
    expectCode(() => act(t, 'a', { type: 'check' }), 'cannot-check');
    expect(legalActions(t, 'a')).toMatchObject({ canCheck: false, toCall: 20, canRaise: true });
    expect(legalActions(t, 'b')).toBeNull();
    t = act(t, 'a', { type: 'call' });
    t = act(t, 'b', { type: 'call' });
    expect(toActId(t)).toBe('c');
    expect(legalActions(t, 'c')).toMatchObject({ canCheck: true, toCall: 0 });
    t = act(t, 'c', { type: 'check' });
    const h = t.hand as Hand;
    expect(h.street).toBe('flop');
    expect(h.board).toHaveLength(3);
    expect(h.players.every((p) => p.bet === 0)).toBe(true);
    expect(potTotal(h)).toBe(60);
    expect(toActId(t)).toBe('b'); // first live seat left of the button
  });

  it('minimum raise is the size of the last raise, and re-raises build on it', () => {
    let t = startHand(seat(['a', 'b', 'c']), rnd());
    expect(legalActions(t, 'a')).toMatchObject({ minRaiseTo: 40, maxRaiseTo: 1000 });
    expectCode(() => act(t, 'a', { type: 'raise', to: 30 }), 'raise-too-small');
    expectCode(() => act(t, 'a', { type: 'raise', to: 20 }), 'raise-too-small');
    expectCode(() => act(t, 'a', { type: 'raise', to: 2000 }), 'raise-too-big');
    expectCode(() => act(t, 'a', { type: 'raise', to: 50.5 }), 'bad-amount');
    t = act(t, 'a', { type: 'raise', to: 100 }); // raise of 80
    expect(legalActions(t, 'b')).toMatchObject({ toCall: 90, minRaiseTo: 180 });
    expectCode(() => act(t, 'b', { type: 'raise', to: 150 }), 'raise-too-small');
    t = act(t, 'b', { type: 'raise', to: 180 });
    expect(legalActions(t, 'c')).toMatchObject({ toCall: 160, minRaiseTo: 260 });
    t = act(t, 'c', { type: 'fold' });
    expect(legalActions(t, 'a')).toMatchObject({ toCall: 80, minRaiseTo: 260 });
  });

  it('big blind keeps the option when everyone limps', () => {
    let t = startHand(seat(['a', 'b', 'c']), rnd());
    t = act(t, 'a', { type: 'call' });
    t = act(t, 'b', { type: 'call' });
    expect(toActId(t)).toBe('c');
    t = act(t, 'c', { type: 'raise', to: 60 });
    expect(toActId(t)).toBe('a');
    t = act(t, 'a', { type: 'call' });
    t = act(t, 'b', { type: 'call' });
    expect((t.hand as Hand).street).toBe('flop');
  });

  it('a bet on the flop must be at least the big blind', () => {
    let t = startHand(seat(['a', 'b']), rnd());
    t = act(t, 'a', { type: 'call' });
    t = act(t, 'b', { type: 'check' });
    expect(legalActions(t, 'b')).toMatchObject({ minRaiseTo: 20 });
    expectCode(() => act(t, 'b', { type: 'raise', to: 10 }), 'raise-too-small');
    t = act(t, 'b', { type: 'raise', to: 20 });
    expect((t.hand as Hand).currentBet).toBe(20);
  });

  it('everyone folding to one player ends the hand without a showdown', () => {
    let t = startHand(seat(['a', 'b', 'c']), rnd());
    t = act(t, 'a', { type: 'raise', to: 60 });
    t = act(t, 'b', { type: 'fold' });
    t = act(t, 'c', { type: 'fold' });
    const h = t.hand as Hand;
    expect(h.over).toBe(true);
    expect(h.result?.uncontested).toBe(true);
    expect(h.result?.reveals).toEqual({});
    expect(stacks(t)).toEqual({ a: 1030, b: 990, c: 980 });
    expectCode(() => act(t, 'a', { type: 'check' }), 'no-hand');
  });

  it('plays a hand to a showdown and pays the best hand', () => {
    let t = startHand(seat(['a', 'b']), rnd());
    t = rig(t, { a: ['As', 'Ad'], b: ['Kh', 'Kc'] }, ['2c', '7d', '9h', 'Js', '3s']);
    t = act(t, 'a', { type: 'raise', to: 60 });
    t = act(t, 'b', { type: 'call' });
    expect((t.hand as Hand).board).toEqual(['2c', '7d', '9h']);
    t = act(t, 'b', { type: 'check' });
    t = act(t, 'a', { type: 'check' });
    expect((t.hand as Hand).board).toHaveLength(4);
    t = act(t, 'b', { type: 'raise', to: 100 });
    t = act(t, 'a', { type: 'call' });
    t = act(t, 'b', { type: 'check' });
    t = act(t, 'a', { type: 'check' });
    const h = t.hand as Hand;
    expect(h.over).toBe(true);
    expect(h.board).toEqual(['2c', '7d', '9h', 'Js', '3s']);
    expect(stacks(t)).toEqual({ a: 1160, b: 840 });
    expect(Object.keys(h.result?.reveals ?? {}).sort()).toEqual(['a', 'b']);
    expect(h.result?.reveals.a?.label).toBe('a pair of aces');
    expect(chipsTotal(t)).toBe(2000);
  });
});

describe('all-ins, side pots and showdown', () => {
  it('computePots builds a main pot and side pots by level', () => {
    const pots = computePots([
      { id: 'a', total: 50, folded: false },
      { id: 'b', total: 200, folded: false },
      { id: 'c', total: 500, folded: false },
      { id: 'd', total: 500, folded: false },
    ]);
    expect(pots).toEqual([
      { amount: 200, eligible: ['a', 'b', 'c', 'd'] },
      { amount: 450, eligible: ['b', 'c', 'd'] },
      { amount: 600, eligible: ['c', 'd'] },
    ]);
  });

  it('a folded player funds pots but cannot win them', () => {
    const pots = computePots([
      { id: 'a', total: 100, folded: true },
      { id: 'b', total: 100, folded: false },
      { id: 'c', total: 300, folded: false },
    ]);
    expect(pots.reduce((s, p) => s + p.amount, 0)).toBe(500);
    expect(pots.every((p) => !p.eligible.includes('a'))).toBe(true);
    expect(pots[0]).toEqual({ amount: 300, eligible: ['b', 'c'] });
    expect(pots[1]).toEqual({ amount: 200, eligible: ['c'] });
  });

  it('three all-ins with different stacks: each side pot goes to its best eligible hand', () => {
    // a is short (300), b middle (600), c deep (1000).
    let t = startHand(seat(['a', 'b', 'c'], [300, 600, 1000]), rnd());
    t = rig(
      t,
      { a: ['As', 'Ad'], b: ['Ks', 'Kd'], c: ['Qs', 'Qd'] },
      ['2c', '7d', '9h', 'Js', '3s'],
    );
    t = act(t, 'a', { type: 'allin' });
    t = act(t, 'b', { type: 'allin' });
    t = act(t, 'c', { type: 'call' });
    const h = t.hand as Hand;
    expect(h.over).toBe(true);
    // Main pot 900 to a (aces), side pot 600 to b (kings); c never needed the other 400.
    expect(h.result?.awards.map((x) => [x.amount, x.winners])).toEqual([
      [900, ['a']],
      [600, ['b']],
    ]);
    expect(stacks(t)).toEqual({ a: 900, b: 600, c: 400 });
    expect(chipsTotal(t)).toBe(1900);
  });

  it('the short stack loses its pot but not the side pot it is not part of', () => {
    let t = startHand(seat(['a', 'b', 'c'], [300, 600, 1000]), rnd());
    t = rig(
      t,
      { a: ['2s', '2d'], b: ['Ks', 'Kd'], c: ['Qs', 'Qd'] },
      ['5c', '7d', '9h', 'Js', '3s'],
    );
    t = act(t, 'a', { type: 'allin' });
    t = act(t, 'b', { type: 'allin' });
    t = act(t, 'c', { type: 'allin' });
    expect(stacks(t)).toEqual({ a: 0, b: 1500, c: 400 });
  });

  it('runs the board out when everyone is all in before the flop', () => {
    let t = startHand(seat(['a', 'b']), rnd());
    t = rig(t, { a: ['As', 'Ad'], b: ['Kh', 'Kc'] }, ['2c', '7d', '9h', 'Js', '3s']);
    t = act(t, 'a', { type: 'allin' });
    t = act(t, 'b', { type: 'call' });
    const h = t.hand as Hand;
    expect(h.over).toBe(true);
    expect(h.board).toHaveLength(5);
    expect(stacks(t)).toEqual({ a: 2000, b: 0 });
  });

  it('an uncalled over-bet goes back to the bettor', () => {
    let t = startHand(seat(['a', 'b'], [1000, 200]), rnd());
    t = rig(t, { a: ['2s', '2d'], b: ['Ah', 'Ac'] }, ['5c', '7d', '9h', 'Js', '3s']);
    t = act(t, 'a', { type: 'raise', to: 600 });
    t = act(t, 'b', { type: 'call' }); // all in for 200
    expect(t.hand?.over).toBe(true);
    expect(stacks(t)).toEqual({ a: 800, b: 400 });
  });

  it('ties split the pot and the odd chip goes left of the button', () => {
    let t = startHand(seat(['a', 'b', 'c'], [1000, 1000, 1000], { smallBlind: 1, bigBlind: 2 }), rnd());
    // Board plays for a and b; c folds after posting... c is the BB, so make c fold to a bet.
    t = rig(
      t,
      { a: ['2s', '3d'], b: ['4s', '5d'], c: ['6s', '7d'] },
      ['Ac', 'Kd', 'Qh', 'Js', 'Td'],
    );
    t = act(t, 'a', { type: 'raise', to: 5 }); // a (button, seat 0) raises to 5
    t = act(t, 'b', { type: 'call' }); // b is the small blind
    t = act(t, 'c', { type: 'fold' }); // c folds the big blind: pot 5 + 5 + 2 = 12
    // Check down: the board is a broadway straight that all live players share.
    for (let i = 0; i < 3; i++) {
      t = act(t, 'b', { type: 'check' });
      t = act(t, 'a', { type: 'check' });
    }
    expect(t.hand?.over).toBe(true);
    // 12 / 2 = 6 each; a stands at 1000 - 5 + 6, b the same.
    expect(stacks(t)).toEqual({ a: 1001, b: 1001, c: 998 });
  });

  it('an odd chip is awarded to the winner closest left of the button', () => {
    let t = startHand(seat(['a', 'b', 'c'], [1000, 1000, 1000], { smallBlind: 1, bigBlind: 3 }), rnd());
    t = rig(t, { a: ['2s', '3d'], b: ['4s', '5d'], c: ['6s', '7d'] }, ['Ac', 'Kd', 'Qh', 'Js', 'Td']);
    // a calls 3, b (sb) calls, c checks: pot 9; make c fold post-flop to leave a pot of 9 split two ways... instead:
    t = act(t, 'a', { type: 'call' });
    t = act(t, 'b', { type: 'call' });
    t = act(t, 'c', { type: 'check' });
    t = act(t, 'b', { type: 'raise', to: 3 });
    t = act(t, 'c', { type: 'fold' });
    t = act(t, 'a', { type: 'call' });
    for (let i = 0; i < 2; i++) {
      t = act(t, 'b', { type: 'check' });
      t = act(t, 'a', { type: 'check' });
    }
    // Pot = 9 + 6 = 15, split between a and b: 7 each, the odd chip goes to b (first left of button 0).
    expect(stacks(t)).toEqual({ a: 1000 - 6 + 7, b: 1000 - 6 + 8, c: 997 });
  });

  it('a short all-in raise does not reopen the betting for a player who already acted', () => {
    // a bets 100, b calls, c (short) shoves 130: a and b may call or fold but not raise.
    let t = startHand(seat(['a', 'b', 'c'], [1000, 1000, 130]), rnd());
    t = act(t, 'a', { type: 'raise', to: 100 });
    t = act(t, 'b', { type: 'call' });
    t = act(t, 'c', { type: 'allin' });
    expect((t.hand as Hand).currentBet).toBe(130);
    expect(legalActions(t, 'a')).toMatchObject({ toCall: 30, canRaise: false });
    expectCode(() => act(t, 'a', { type: 'raise', to: 300 }), 'raise-closed');
    expectCode(() => act(t, 'a', { type: 'allin' }), 'raise-closed');
    t = act(t, 'a', { type: 'call' });
    expect(legalActions(t, 'b')).toMatchObject({ toCall: 30, canRaise: false });
  });

  it('a full raise reopens the betting', () => {
    let t = startHand(seat(['a', 'b', 'c']), rnd());
    t = act(t, 'a', { type: 'raise', to: 100 });
    t = act(t, 'b', { type: 'call' });
    t = act(t, 'c', { type: 'raise', to: 200 });
    expect(legalActions(t, 'a')).toMatchObject({ toCall: 100, canRaise: true, minRaiseTo: 300 });
  });

  it('short all-ins that add up to a full raise reopen the betting for the raiser', () => {
    // d raises to 100 (a raise of 80). e shoves 170 (+70) and c shoves 240 (+70): each is short, together 140.
    let t = startHand(seat(['a', 'b', 'c', 'd', 'e'], [1000, 1000, 240, 1000, 170]), rnd());
    expect(toActId(t)).toBe('d');
    t = act(t, 'd', { type: 'raise', to: 100 });
    t = act(t, 'e', { type: 'allin' });
    expect(legalActions(t, 'a')).toMatchObject({ canRaise: true }); // a has not acted yet
    t = act(t, 'a', { type: 'fold' });
    t = act(t, 'b', { type: 'fold' });
    t = act(t, 'c', { type: 'allin' });
    expect(legalActions(t, 'd')).toMatchObject({ toCall: 140, canRaise: true, minRaiseTo: 320 });
  });

  it('standing up mid-hand folds and frees the seat after the hand', () => {
    let t = startHand(seat(['a', 'b', 'c']), rnd());
    t = stand(t, 'c');
    expect(t.hand?.players.find((p) => p.id === 'c')?.folded).toBe(true);
    expect(t.seats[2]?.leaving).toBe(true);
    t = act(t, 'a', { type: 'fold' });
    expect(t.hand?.over).toBe(true);
    expect(t.seats[2]).toBeNull();
    expect(stacks(t)).toEqual({ a: 1000, b: 1020 });
  });

  it('standing up on your own turn passes the action on', () => {
    let t = startHand(seat(['a', 'b', 'c']), rnd());
    t = stand(t, 'a');
    expect(toActId(t)).toBe('b');
    t = stand(t, 'b');
    expect(t.hand?.over).toBe(true);
    expect(t.seats.filter(Boolean).map((s) => s?.id)).toEqual(['c']);
  });

  it('rebuy and blind changes only between hands', () => {
    let t = startHand(seat(['a', 'b'], [1000, 30]), rnd());
    expectCode(() => setBlinds(t, 20, 40), 'hand-running');
    expectCode(() => rebuy(t, 'b'), 'hand-running');
    t = rig(t, { a: ['As', 'Ad'], b: ['2h', '3c'] }, ['5c', '7d', '9h', 'Js', '4s']);
    t = act(t, 'a', { type: 'raise', to: 40 });
    t = act(t, 'b', { type: 'call' });
    expect(stacks(t)).toEqual({ a: 1030, b: 0 });
    expectCode(() => rebuy(t, 'a'), 'has-chips');
    t = rebuy(t, 'b');
    expect(stacks(t).b).toBe(1000);
    t = setBlinds(t, 20, 40);
    expect(t.config.bigBlind).toBe(40);
    expectCode(() => setBlinds(t, 50, 40), 'bad-config');
  });

  it('a short blind posts what it has and is all in', () => {
    let t = startHand(seat(['a', 'b'], [1000, 15]), rnd());
    const h = t.hand as Hand;
    expect(h.players.find((p) => p.id === 'b')).toMatchObject({ bet: 15, stack: 0, allIn: true });
    // a still owes the full big blind.
    expect(legalActions(t, 'a')).toMatchObject({ toCall: 10 });
    t = rig(t, { a: ['As', 'Ad'], b: ['2h', '3c'] }, ['5c', '7d', '9h', 'Js', '3s']);
    t = act(t, 'a', { type: 'call' });
    expect(t.hand?.over).toBe(true);
    expect(stacks(t)).toEqual({ a: 1015, b: 0 });
  });
});

describe('chips are conserved over many random hands', () => {
  it('plays random legal actions without errors or lost chips', () => {
    const r = seededRandom(42);
    for (let game = 0; game < 40; game++) {
      const n = 2 + r(6);
      const ids = Array.from({ length: n }, (_, i) => `p${i}`);
      let t = seat(ids, ids.map(() => 100 + r(2000)), { smallBlind: 5, bigBlind: 10 });
      const total = t.seats.reduce((s, x) => s + (x?.chips ?? 0), 0);
      for (let hand = 0; hand < 12; hand++) {
        if (t.seats.filter((s) => s && s.chips > 0).length < 2) break;
        t = startHand(t, r);
        let guard = 0;
        while (handActive(t)) {
          if (guard++ > 200) throw new Error('hand does not end');
          const id = toActId(t) as string;
          const lg = legalActions(t, id);
          expect(lg).not.toBeNull();
          const roll = r(10);
          let a: Action;
          if (roll < 1) a = { type: 'fold' };
          else if (roll < 5) a = lg!.canCheck ? { type: 'check' } : { type: 'call' };
          else if (roll < 8 && lg!.canRaise) {
            const to = lg!.minRaiseTo + r(Math.max(1, lg!.maxRaiseTo - lg!.minRaiseTo + 1));
            a = { type: 'raise', to: Math.min(to, lg!.maxRaiseTo) };
          } else if (roll < 9 && lg!.canRaise) a = { type: 'allin' };
          else a = lg!.canCheck ? { type: 'check' } : { type: 'call' };
          t = act(t, id, a);
          expect(chipsTotal(t)).toBe(total);
        }
        expect(t.seats.reduce((s, x) => s + (x?.chips ?? 0), 0)).toBe(total);
        const res = t.hand?.result;
        expect(Object.values(res?.payouts ?? {}).reduce((s, x) => s + x, 0)).toBe(
          potTotal(t.hand as Hand),
        );
      }
    }
  });
});
