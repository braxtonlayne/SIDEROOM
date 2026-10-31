// Cards are two-character strings: a rank from "23456789TJQKA" and a suit from "cdhs" ("As", "Td", "2c").

export type Suit = 'c' | 'd' | 'h' | 's';
export type Card = string;

export const RANKS = '23456789TJQKA';
export const SUITS: readonly Suit[] = ['c', 'd', 'h', 's'];

export interface ParsedCard {
  /** 2 to 14 (ace high). */
  rank: number;
  suit: Suit;
}

export function parseCard(card: Card): ParsedCard {
  const r = RANKS.indexOf(card.charAt(0));
  const s = card.charAt(1) as Suit;
  if (card.length !== 2 || r < 0 || !SUITS.includes(s)) throw new Error(`Not a card: ${card}`);
  return { rank: r + 2, suit: s };
}

export function isCard(x: unknown): x is Card {
  if (typeof x !== 'string') return false;
  try {
    parseCard(x);
    return true;
  } catch {
    return false;
  }
}

export function makeDeck(): Card[] {
  const deck: Card[] = [];
  for (const s of SUITS) for (const r of RANKS) deck.push(`${r}${s}`);
  return deck;
}

/** A source of integers in [0, n). The table uses `cryptoRandom`; tests pass a seeded one. */
export type RandomInt = (n: number) => number;

export const cryptoRandom: RandomInt = (n) => {
  // Rejection sampling over 32 bits so the shuffle has no modulo bias.
  const limit = Math.floor(0x100000000 / n) * n;
  const buf = new Uint32Array(1);
  for (;;) {
    globalThis.crypto.getRandomValues(buf);
    const v = buf[0] as number;
    if (v < limit) return v % n;
  }
};

/** A small deterministic generator for tests (mulberry32). */
export function seededRandom(seed: number): RandomInt {
  let a = seed >>> 0;
  return (n) => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return Math.floor((((t ^ (t >>> 14)) >>> 0) / 4294967296) * n);
  };
}

/** Fisher-Yates; returns a new array. */
export function shuffle<T>(items: readonly T[], rnd: RandomInt = cryptoRandom): T[] {
  const a = items.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = rnd(i + 1);
    [a[i], a[j]] = [a[j] as T, a[i] as T];
  }
  return a;
}

export const SUIT_SYMBOL: Record<Suit, string> = {
  c: '♣',
  d: '♦',
  h: '♥',
  s: '♠',
};

export function cardLabel(card: Card): string {
  const { suit } = parseCard(card);
  const r = card.charAt(0) === 'T' ? '10' : card.charAt(0);
  return `${r}${SUIT_SYMBOL[suit]}`;
}
