import { describe as suite, expect, it } from 'vitest';
import { makeDeck, seededRandom, shuffle } from './cards';
import { CATEGORY, compareHands, describe, evaluate, winners } from './ranker';

const hand = (s: string) => evaluate(s.split(' '));

suite('evaluate: categories from seven cards', () => {
  it('royal flush and straight flush', () => {
    const r = hand('As Ks Qs Js Ts 2d 3c');
    expect(r.category).toBe(CATEGORY.straightFlush);
    expect(describe(r)).toBe('a royal flush');
    const sf = hand('9h 8h 7h 6h 5h Ad Ac');
    expect(sf.category).toBe(CATEGORY.straightFlush);
    expect(sf.kickers[0]).toBe(9);
    expect(describe(sf)).toBe('a straight flush, nine high');
  });

  it('the steel wheel is a five-high straight flush', () => {
    const r = hand('Ah 2h 3h 4h 5h Kd Qc');
    expect(r.category).toBe(CATEGORY.straightFlush);
    expect(r.kickers[0]).toBe(5);
    expect(r.best).toHaveLength(5);
  });

  it('a straight and a flush in different cards is only a flush', () => {
    // Straight 5-9 with mixed suits, five hearts that are not a straight.
    const r = hand('9h 8h 7c 6h 5d 2h Kh');
    expect(r.category).toBe(CATEGORY.flush);
    expect(r.kickers).toEqual([13, 9, 8, 6, 2]);
  });

  it('four of a kind takes the best kicker of the other three cards', () => {
    const r = hand('7s 7h 7d 7c Ah Kd 2c');
    expect(r.category).toBe(CATEGORY.quads);
    expect(r.kickers).toEqual([7, 14]);
    expect(describe(r)).toBe('four sevens');
  });

  it('full house: trips over the higher pair of two pairs', () => {
    const r = hand('Kh Kd Kc 9s 9h 4d 4c');
    expect(r.category).toBe(CATEGORY.fullHouse);
    expect(r.kickers).toEqual([13, 9]);
    expect(describe(r)).toBe('a full house, kings full of nines');
  });

  it('two sets make a full house with the lower set as the pair', () => {
    const r = hand('Qh Qd Qc 8s 8h 8d 2c');
    expect(r.category).toBe(CATEGORY.fullHouse);
    expect(r.kickers).toEqual([12, 8]);
  });

  it('flush uses the five highest cards of the suit', () => {
    const r = hand('Ac Jc 9c 6c 4c 3c 2d');
    expect(r.category).toBe(CATEGORY.flush);
    expect(r.kickers).toEqual([14, 11, 9, 6, 4]);
  });

  it('straight: broadway, wheel, and the highest of a seven-card run', () => {
    expect(hand('Ad Kc Qh Js Td 2c 3c').kickers[0]).toBe(14);
    const wheel = hand('Ad 2c 3h 4s 5d Kc 9c');
    expect(wheel.category).toBe(CATEGORY.straight);
    expect(wheel.kickers[0]).toBe(5);
    expect(describe(wheel)).toBe('a straight, five high');
    const run = hand('4d 5c 6h 7s 8d 9c Kc');
    expect(run.kickers[0]).toBe(9);
  });

  it('a wheel does not wrap: Q-K-A-2-3 is not a straight', () => {
    expect(hand('Qd Kc Ah 2s 3d 7c 9c').category).toBe(CATEGORY.highCard);
  });

  it('trips, two pair (best two of three pairs), pair, high card', () => {
    const trips = hand('9s 9h 9d Ac Kd 3c 2h');
    expect(trips.category).toBe(CATEGORY.trips);
    expect(trips.kickers).toEqual([9, 14, 13]);

    const tp = hand('Jh Jd 8c 8s 3d 3c Ah');
    expect(tp.category).toBe(CATEGORY.twoPair);
    expect(tp.kickers).toEqual([11, 8, 14]); // the third pair's 3 loses to the ace kicker
    expect(describe(tp)).toBe('two pair, jacks and eights');

    const pair = hand('Ts Th Ad Kc 9d 4c 2h');
    expect(pair.category).toBe(CATEGORY.pair);
    expect(pair.kickers).toEqual([10, 14, 13, 9]);

    const high = hand('Ad Jc 9h 7s 5d 3c 2h');
    expect(high.category).toBe(CATEGORY.highCard);
    expect(high.kickers).toEqual([14, 11, 9, 7, 5]);
    expect(describe(high)).toBe('ace high');
  });

  it('takes five cards and rejects bad input', () => {
    expect(hand('As Ks Qs Js Ts').category).toBe(CATEGORY.straightFlush);
    expect(() => evaluate(['As', 'Ks', 'Qs', 'Js'])).toThrow();
    expect(() => evaluate(['As', 'As', 'Qs', 'Js', 'Ts'])).toThrow();
    expect(() => evaluate(['As', 'Ks', 'Qs', 'Js', 'Xs'])).toThrow();
  });
});

suite('comparison and ties', () => {
  it('orders the categories', () => {
    const ladder = [
      'Ad Jc 9h 7s 5d 3c 2h',
      'Ts Th Ad Kc 9d 4c 2h',
      'Jh Jd 8c 8s 3d 3c Ah',
      '9s 9h 9d Ac Kd 3c 2h',
      '4d 5c 6h 7s 8d Kc 2c',
      'Ac Jc 9c 6c 4c Kd 2d',
      'Kh Kd Kc 9s 9h 4d 2c',
      '7s 7h 7d 7c Ah Kd 2c',
      '9h 8h 7h 6h 5h Ad Ac',
    ].map(hand);
    for (let i = 1; i < ladder.length; i++) {
      expect(compareHands(ladder[i]!, ladder[i - 1]!)).toBeGreaterThan(0);
    }
  });

  it('kickers decide between equal pairs', () => {
    const a = hand('Ks Kh Ad 9c 5d 3c 2h');
    const b = hand('Kd Kc Qd 9s 5h 3d 2s');
    expect(compareHands(a, b)).toBeGreaterThan(0);
  });

  it('the board plays: two players tie and split', () => {
    const board = 'As Kd Qc Jh Td';
    const a = evaluate([...board.split(' '), '2c', '3d']);
    const b = evaluate([...board.split(' '), '4c', '5d']);
    expect(a.score).toBe(b.score);
    expect(winners([a, b])).toEqual([0, 1]);
  });

  it('a higher straight beats a lower one; the wheel is the lowest', () => {
    const wheel = hand('Ad 2c 3h 4s 5d 9c Kc');
    const six = hand('2d 3c 4h 5s 6d 9c Kc');
    expect(compareHands(six, wheel)).toBeGreaterThan(0);
  });

  it('winners picks the single best hand', () => {
    const hs = [hand('Ad Jc 9h 7s 5d 3c 2h'), hand('Ts Th Ad Kc 9d 4c 2h'), hand('2s 2h 3d 4c 9d Jc Ah')];
    expect(winners(hs)).toEqual([1]);
  });

  it('is symmetric and total over random seven-card hands', () => {
    const rnd = seededRandom(7);
    for (let i = 0; i < 300; i++) {
      const cards = shuffle(makeDeck(), rnd);
      const a = evaluate(cards.slice(0, 7));
      const b = evaluate(cards.slice(7, 14));
      expect(Math.sign(compareHands(a, b))).toBe(-Math.sign(compareHands(b, a)));
      expect(a.best).toHaveLength(5);
      // The five chosen cards re-evaluate to the same score.
      expect(evaluate(a.best).score).toBe(a.score);
    }
  });

  it('category frequencies over a million-ish deals look like poker (spot check)', () => {
    const rnd = seededRandom(99);
    const counts = new Array<number>(9).fill(0);
    const n = 6000;
    for (let i = 0; i < n; i++) counts[evaluate(shuffle(makeDeck(), rnd).slice(0, 7)).category]!++;
    // Seven-card odds: pair about 43.8%, high card about 17.4%, two pair about 23.5%.
    expect(counts[CATEGORY.pair]! / n).toBeGreaterThan(0.4);
    expect(counts[CATEGORY.pair]! / n).toBeLessThan(0.48);
    expect(counts[CATEGORY.highCard]! / n).toBeGreaterThan(0.14);
    expect(counts[CATEGORY.highCard]! / n).toBeLessThan(0.21);
    expect(counts[CATEGORY.twoPair]! / n).toBeGreaterThan(0.2);
    expect(counts[CATEGORY.twoPair]! / n).toBeLessThan(0.27);
  });
});
