import {
  DiceError,
  Rng,
  describeRecord,
  makeRollRecord,
  rollExpr,
  seedFromHex,
  seedToHex,
  verifyRecord,
} from '../src/dice.js';

const roll = (s, seed = 1n) => rollExpr(s, new Rng(seed));
const code = (s) => {
  try {
    roll(s);
  } catch (e) {
    return e instanceof DiceError ? e.code : 'other';
  }
  return 'ok';
};

describe('SplitMix64', () => {
  it('matches the reference stream for seed 0', () => {
    const r = new Rng(0n);
    expect(r.nextU64()).toBe(0xe220a8397b1dcdafn);
    expect(r.nextU64()).toBe(0x6e789e6aa1b965f4n);
    expect(r.nextU64()).toBe(0x06c45d188009454fn);
  });
  it('is uniform enough', () => {
    const r = new Rng(42n);
    const c = [0, 0, 0, 0, 0, 0];
    for (let i = 0; i < 60000; i++) c[Number(r.below(6n))]++;
    for (const n of c) {
      expect(n).toBeGreaterThan(9000);
      expect(n).toBeLessThan(11000);
    }
  });
});

describe('grammar (ported from sr-tools-wasm)', () => {
  it('does arithmetic', () => {
    expect(roll('7').total).toBe(7);
    expect(roll('1+2*3').total).toBe(7);
    expect(roll('(1+2)*3').total).toBe(9);
    expect(roll('10-3-2').total).toBe(5);
    expect(roll(' 2 * ( 3 + 4 ) ').total).toBe(14);
  });
  it('keeps every total in range over many seeds', () => {
    for (let s = 0n; s < 300n; s++) {
      const t = roll('2d20kh1+5', s).total;
      expect(t >= 6 && t <= 25).toBe(true);
      const f = roll('4dF', s).total;
      expect(f >= -4 && f <= 4).toBe(true);
      expect(roll('d1', s).total).toBe(1);
      const p = roll('d%', s).total;
      expect(p >= 1 && p <= 100).toBe(true);
    }
  });
  it('marks kept and dropped dice', () => {
    const r = roll('4d6dl1', 3n);
    const g = r.groups[0];
    expect(g.dice.filter((d) => d.dropped)).toHaveLength(1);
    expect(g.dice.find((d) => d.dropped).value).toBe(Math.min(...g.dice.map((d) => d.value)));
    expect(g.dice.filter((d) => !d.dropped).reduce((a, d) => a + d.value, 0)).toBe(r.total);
    const vals = roll('5d10kh2', 9n)
      .groups[0].dice.map((d) => d.value)
      .sort((a, b) => a - b);
    expect(roll('5d10kh2', 9n).total).toBe(vals[3] + vals[4]);
    expect(roll('5d10kl2', 9n).total).toBe(vals[0] + vals[1]);
    expect(roll('5d10dh2', 9n).total).toBe(vals[0] + vals[1] + vals[2]);
  });
  it('explodes on the maximum', () => {
    let saw = false;
    for (let s = 0n; s < 200n; s++) {
      const g = roll('3d4!', s).groups[0];
      g.dice.forEach((d, i) => {
        if (d.exploded) {
          saw = true;
          expect(g.dice[i - 1].value).toBe(4);
        }
      });
    }
    expect(saw).toBe(true);
    expect(roll('2d6!>=5', 11n).groups[0].notation).toBe('2d6!>=5');
  });
  it('counts successes', () => {
    for (let s = 0n; s < 50n; s++) {
      const r = roll('6d10>=8', s);
      expect(r.total).toBe(r.groups[0].dice.filter((d) => d.value >= 8).length);
    }
  });
  it('normalises notation', () => {
    expect(roll('d20').groups[0].notation).toBe('1d20');
    expect(roll('D%').groups[0].notation).toBe('1d100');
    expect(roll('4d6dl').groups[0].notation).toBe('4d6dl1');
    expect(roll('4df').groups[0].notation).toBe('4dF');
    expect(roll('2D6 + 1d4 + 3').expression).toBe('2d6+1d4+3');
  });
  it('refuses bad input with the same codes', () => {
    expect(code('')).toBe('empty');
    expect(code('0d6')).toBe('zero-dice');
    expect(code('1001d6')).toBe('too-many-dice');
    expect(code('d0')).toBe('bad-sides');
    expect(code('d1000001')).toBe('bad-sides');
    expect(code('4d6kh5')).toBe('bad-modifier');
    expect(code('d1!')).toBe('bad-modifier');
    expect(code('d6!>=1')).toBe('bad-modifier');
    for (const s of ['d', '2d6+', '2d6)', '(2d6', 'hello', '2d6!!', '2d6>3>4', '2dé']) {
      expect(code(s)).toBe('syntax');
    }
    expect(code('1+'.repeat(200))).toBe('too-long');
    expect(code('('.repeat(50))).toBe('too-deep');
    expect(code(Array(21).fill('1000d2!').join('+'))).toBe('too-many-rolls');
    expect(code('4294967295*4294967295*4294967295')).toBe('overflow');
  });
});

describe('the roll record', () => {
  const seed = 0xdeadbeefcafen;
  const rec = makeRollRecord({ seq: 1, by: 'u1', expression: '2d20kh1+5', seed, at: 5 });

  it('stores the seed as 16 hex digits and the outcome', () => {
    expect(rec.seed).toBe('0000deadbeefcafe');
    expect(rec.expression).toBe('2d20kh1+5');
    expect(rec.total).toBeGreaterThanOrEqual(6);
    expect(rec.groups[0].dice).toHaveLength(2);
    expect(seedFromHex(rec.seed)).toBe(seed);
    expect(seedToHex(1n)).toBe('0000000000000001');
  });
  it('is reproducible: the same seed gives the same record', () => {
    const again = makeRollRecord({ seq: 1, by: 'u1', expression: '2D20KH1 + 5', seed, at: 5 });
    expect(again).toEqual(rec);
  });
  it('verifies, and fails when the total, a die or the seed is changed', () => {
    expect(verifyRecord(rec)).toBe(true);
    expect(verifyRecord({ ...rec, total: rec.total + 1 })).toBe(false);
    const bent = structuredClone(rec);
    bent.groups[0].dice[0].value = bent.groups[0].dice[0].value === 20 ? 19 : 20;
    expect(verifyRecord(bent)).toBe(false);
    expect(verifyRecord({ ...rec, seed: '0000deadbeefcaff' })).toBe(false);
    expect(verifyRecord({ ...rec, seed: 'zz' })).toBe(false);
    expect(verifyRecord({ ...rec, expression: 'garbage' })).toBe(false);
  });
  it('describes itself like the /roll message', () => {
    expect(describeRecord(rec)).toMatch(/^2d20kh1\+5 = \d+ \(2d20kh1: \[/);
    const plain = makeRollRecord({ seq: 2, by: 'u', expression: '7', seed: 1n });
    expect(describeRecord(plain)).toBe('7 = 7');
  });
});
