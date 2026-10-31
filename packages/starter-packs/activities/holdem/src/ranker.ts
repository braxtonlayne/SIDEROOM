// Texas Hold'em hand ranking: the best five of five to seven cards.

import { parseCard, type Card } from './cards';

export const CATEGORY = {
  highCard: 0,
  pair: 1,
  twoPair: 2,
  trips: 3,
  straight: 4,
  flush: 5,
  fullHouse: 6,
  quads: 7,
  straightFlush: 8,
} as const;
export type Category = (typeof CATEGORY)[keyof typeof CATEGORY];

export interface HandRank {
  category: Category;
  /** Tie-breaking ranks, most significant first. */
  kickers: number[];
  /** One number: bigger beats smaller, equal ties. */
  score: number;
  /** The five cards that make the hand. */
  best: Card[];
}

const RANK_NAME: Record<number, [string, string]> = {
  2: ['two', 'twos'],
  3: ['three', 'threes'],
  4: ['four', 'fours'],
  5: ['five', 'fives'],
  6: ['six', 'sixes'],
  7: ['seven', 'sevens'],
  8: ['eight', 'eights'],
  9: ['nine', 'nines'],
  10: ['ten', 'tens'],
  11: ['jack', 'jacks'],
  12: ['queen', 'queens'],
  13: ['king', 'kings'],
  14: ['ace', 'aces'],
};

function score(category: number, kickers: number[]): number {
  let s = category;
  for (let i = 0; i < 5; i++) s = s * 15 + (kickers[i] ?? 0);
  return s;
}

/** The high card of the best straight in a set of ranks, or 0. The wheel (A-2-3-4-5) is five-high. */
function straightHigh(ranks: Set<number>): number {
  for (let high = 14; high >= 5; high--) {
    let ok = true;
    for (let r = high; r > high - 5; r--) {
      const want = r === 1 ? 14 : r;
      if (!ranks.has(want)) {
        ok = false;
        break;
      }
    }
    if (ok) return high;
  }
  return 0;
}

interface P {
  card: Card;
  rank: number;
  suit: string;
}

function pickStraight(cards: P[], high: number): Card[] {
  const out: Card[] = [];
  for (let r = high; r > high - 5; r--) {
    const want = r === 1 ? 14 : r;
    const c = cards.find((x) => x.rank === want);
    if (c) out.push(c.card);
  }
  return out;
}

/** Rank the best five-card hand among 5 to 7 distinct cards. */
export function evaluate(cards: readonly Card[]): HandRank {
  if (cards.length < 5 || cards.length > 7) throw new Error('A hand needs five to seven cards');
  if (new Set(cards).size !== cards.length) throw new Error('Duplicate card');
  const parsed: P[] = cards
    .map((card) => ({ card, ...parseCard(card) }))
    .sort((a, b) => b.rank - a.rank);

  const make = (category: Category, kickers: number[], best: Card[]): HandRank => ({
    category,
    kickers,
    score: score(category, kickers),
    best,
  });

  const bySuit = new Map<string, P[]>();
  for (const c of parsed) bySuit.set(c.suit, [...(bySuit.get(c.suit) ?? []), c]);
  const flushCards = [...bySuit.values()].find((g) => g.length >= 5);

  if (flushCards) {
    const sfHigh = straightHigh(new Set(flushCards.map((c) => c.rank)));
    if (sfHigh) return make(CATEGORY.straightFlush, [sfHigh], pickStraight(flushCards, sfHigh));
  }

  const byRank = new Map<number, P[]>();
  for (const c of parsed) byRank.set(c.rank, [...(byRank.get(c.rank) ?? []), c]);
  const groups = [...byRank.entries()]
    .map(([rank, cs]) => ({ rank, cs }))
    .sort((a, b) => b.cs.length - a.cs.length || b.rank - a.rank);

  const top = groups[0];
  const second = groups[1];
  if (top && top.cs.length === 4) {
    const kicker = parsed.find((c) => c.rank !== top.rank);
    return make(
      CATEGORY.quads,
      [top.rank, kicker?.rank ?? 0],
      [...top.cs, ...(kicker ? [kicker] : [])].map((c) => c.card),
    );
  }
  if (top && top.cs.length === 3 && second && second.cs.length >= 2) {
    return make(
      CATEGORY.fullHouse,
      [top.rank, second.rank],
      [...top.cs, ...second.cs.slice(0, 2)].map((c) => c.card),
    );
  }
  if (flushCards) {
    const five = flushCards.slice(0, 5);
    return make(
      CATEGORY.flush,
      five.map((c) => c.rank),
      five.map((c) => c.card),
    );
  }
  const stHigh = straightHigh(new Set(parsed.map((c) => c.rank)));
  if (stHigh) return make(CATEGORY.straight, [stHigh], pickStraight(parsed, stHigh));

  if (top && top.cs.length === 3) {
    const kick = parsed.filter((c) => c.rank !== top.rank).slice(0, 2);
    return make(
      CATEGORY.trips,
      [top.rank, ...kick.map((c) => c.rank)],
      [...top.cs, ...kick].map((c) => c.card),
    );
  }
  if (top && second && top.cs.length === 2 && second.cs.length === 2) {
    const kick = parsed.find((c) => c.rank !== top.rank && c.rank !== second.rank);
    return make(
      CATEGORY.twoPair,
      [top.rank, second.rank, kick?.rank ?? 0],
      [...top.cs, ...second.cs, ...(kick ? [kick] : [])].map((c) => c.card),
    );
  }
  if (top && top.cs.length === 2) {
    const kick = parsed.filter((c) => c.rank !== top.rank).slice(0, 3);
    return make(
      CATEGORY.pair,
      [top.rank, ...kick.map((c) => c.rank)],
      [...top.cs, ...kick].map((c) => c.card),
    );
  }
  const five = parsed.slice(0, 5);
  return make(
    CATEGORY.highCard,
    five.map((c) => c.rank),
    five.map((c) => c.card),
  );
}

/** Negative when `a` loses, positive when `a` wins, 0 for a tie. */
export function compareHands(a: HandRank, b: HandRank): number {
  return a.score - b.score;
}

/** The indexes (into `hands`) of the best hand(s). */
export function winners(hands: readonly HandRank[]): number[] {
  let best = -1;
  let out: number[] = [];
  hands.forEach((h, i) => {
    if (h.score > best) {
      best = h.score;
      out = [i];
    } else if (h.score === best) out.push(i);
  });
  return out;
}

/** "a pair of kings", "a flush, ace high" and so on, for the showdown line. */
export function describe(rank: HandRank): string {
  const k = rank.kickers;
  const one = (r: number | undefined) => RANK_NAME[r ?? 0]?.[0] ?? '?';
  const many = (r: number | undefined) => RANK_NAME[r ?? 0]?.[1] ?? '?';
  switch (rank.category) {
    case CATEGORY.straightFlush:
      return k[0] === 14 ? 'a royal flush' : `a straight flush, ${one(k[0])} high`;
    case CATEGORY.quads:
      return `four ${many(k[0])}`;
    case CATEGORY.fullHouse:
      return `a full house, ${many(k[0])} full of ${many(k[1])}`;
    case CATEGORY.flush:
      return `a flush, ${one(k[0])} high`;
    case CATEGORY.straight:
      return `a straight, ${one(k[0])} high`;
    case CATEGORY.trips:
      return `three ${many(k[0])}`;
    case CATEGORY.twoPair:
      return `two pair, ${many(k[0])} and ${many(k[1])}`;
    case CATEGORY.pair:
      return `a pair of ${many(k[0])}`;
    default:
      return `${one(k[0])} high`;
  }
}
