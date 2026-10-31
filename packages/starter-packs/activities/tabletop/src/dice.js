// Dice grammar of the tools page (UTL-DEV-001, crates/sr-tools-wasm/src/dice.rs), ported so an Activity (a
// sandboxed static bundle that cannot reach the app's wasm) rolls exactly what the tools page rolls: the same
// SplitMix64 stream, the same grammar, the same order of draws. A roll is therefore checkable: the same
// expression and seed give the same dice anywhere. Keep this file in step with dice.rs.

export const MAX_DICE = 1000;
export const MAX_SIDES = 1_000_000;
export const MAX_EXPLODE = 100;
export const MAX_ROLLS = 20_000;
export const MAX_EXPR_CHARS = 200;
const MAX_DEPTH = 16;
const MASK = (1n << 64n) - 1n;

/** A grammar error; `code` is the wasm's code (`syntax`, `too-many-dice`...) and `at` the index for syntax. */
export class DiceError extends Error {
  constructor(code, at) {
    super(at === undefined ? code : `${code}:${at}`);
    this.code = code;
    this.at = at;
  }
}

/** SplitMix64 over BigInt, fully determined by its seed. */
export class Rng {
  /** @param {bigint} seed */
  constructor(seed) {
    this.s = BigInt.asUintN(64, seed);
  }
  nextU64() {
    this.s = (this.s + 0x9e3779b97f4a7c15n) & MASK;
    let z = this.s;
    z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK;
    z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK;
    return z ^ (z >> 31n);
  }
  /** Uniform in 0..n without modulo bias; `n` is a bigint > 0. */
  below(n) {
    const zone = MASK - (MASK % n);
    for (;;) {
      const v = this.nextU64();
      if (v < zone) return v % n;
    }
  }
}

const CMP = {
  '>=': (v, t) => v >= t,
  '<=': (v, t) => v <= t,
  '>': (v, t) => v > t,
  '<': (v, t) => v < t,
  '=': (v, t) => v === t,
};

class Parser {
  constructor(s) {
    this.s = s;
    this.i = 0;
  }
  peek(o = 0) {
    return this.s[this.i + o];
  }
  eat(c) {
    if (this.peek() === c) {
      this.i += 1;
      return true;
    }
    return false;
  }
  number() {
    const start = this.i;
    let v = 0;
    while (this.peek() !== undefined && this.peek() >= '0' && this.peek() <= '9') {
      v = Math.min(v * 10 + (this.peek().charCodeAt(0) - 48), 0xffffffff);
      this.i += 1;
    }
    return this.i > start ? v : null;
  }
  expr(depth) {
    if (depth > MAX_DEPTH) throw new DiceError('too-deep');
    let left = this.term(depth);
    while (this.peek() === '+' || this.peek() === '-') {
      const op = this.peek();
      this.i += 1;
      left = { t: 'bin', op, l: left, r: this.term(depth) };
    }
    return left;
  }
  term(depth) {
    let left = this.factor(depth);
    while (this.eat('*')) left = { t: 'bin', op: '*', l: left, r: this.factor(depth) };
    return left;
  }
  factor(depth) {
    if (this.eat('(')) {
      const e = this.expr(depth + 1);
      if (!this.eat(')')) throw new DiceError('syntax', this.i);
      return e;
    }
    let count = this.number();
    if (this.peek() !== 'd') {
      if (count !== null) return { t: 'num', v: count };
      throw new DiceError('syntax', this.i);
    }
    this.i += 1;
    if (count === null) count = 1;
    if (count === 0) throw new DiceError('zero-dice');
    if (count > MAX_DICE) throw new DiceError('too-many-dice');
    let sides;
    if (this.eat('%')) sides = 100;
    else if (this.eat('f')) sides = 'F';
    else {
      const n = this.number();
      if (n === null) throw new DiceError('syntax', this.i);
      if (n < 1 || n > MAX_SIDES) throw new DiceError('bad-sides');
      sides = n;
    }
    const g = { t: 'dice', count, sides, keep: null, explode: null, success: null };
    for (;;) {
      const at = this.i;
      if (this.eat('!')) {
        if (g.explode !== null) throw new DiceError('syntax', at);
        g.explode = { cond: this.cmp() };
        continue;
      }
      const c = this.cmp();
      if (c) {
        if (g.success) throw new DiceError('syntax', at);
        g.success = c;
        continue;
      }
      const k = this.keep();
      if (!k) break;
      if (g.keep) throw new DiceError('syntax', at);
      g.keep = k;
    }
    validate(g);
    return g;
  }
  cmp() {
    let op = null;
    const a = this.peek();
    const b = this.peek(1);
    if ((a === '>' || a === '<') && b === '=') {
      op = a + '=';
      this.i += 2;
    } else if (a === '>' || a === '<' || a === '=') {
      op = a;
      this.i += 1;
    } else return null;
    const neg = this.eat('-');
    const n = this.number();
    if (n === null) throw new DiceError('syntax', this.i);
    return { op, t: neg ? -n : n };
  }
  keep() {
    const a = this.peek();
    const b = this.peek(1);
    let kind;
    if (a === 'k' && b === 'h') kind = 'kh';
    else if (a === 'k' && b === 'l') kind = 'kl';
    else if (a === 'd' && b === 'h') kind = 'dh';
    else if (a === 'd' && b === 'l') kind = 'dl';
    else return null;
    this.i += 2;
    return { kind, n: this.number() ?? 1 };
  }
}

const bounds = (sides) => (sides === 'F' ? [-1, 1] : [1, sides]);

function validate(g) {
  if (g.keep && g.keep.n > g.count) throw new DiceError('bad-modifier');
  if (g.explode) {
    const [lo, hi] = bounds(g.sides);
    let always = true;
    if (g.explode.cond === null) always = lo === hi;
    else
      for (let v = lo; v <= hi; v++)
        if (!CMP[g.explode.cond.op](v, g.explode.cond.t)) always = false;
    if (always) throw new DiceError('bad-modifier');
  }
}

function notation(g) {
  let s = `${g.count}d${g.sides}`;
  if (g.keep) s += `${g.keep.kind}${g.keep.n}`;
  if (g.explode) s += g.explode.cond ? `!${g.explode.cond.op}${g.explode.cond.t}` : '!';
  if (g.success) s += `${g.success.op}${g.success.t}`;
  return s;
}

function rollDie(ctx, sides) {
  ctx.rolls += 1;
  if (ctx.rolls > MAX_ROLLS) throw new DiceError('too-many-rolls');
  return sides === 'F'
    ? Number(ctx.rng.below(3n)) - 1
    : Number(ctx.rng.below(BigInt(sides))) + 1;
}

function rollGroup(ctx, g) {
  const hi = bounds(g.sides)[1];
  const dice = [];
  const push = (value, exploded) => dice.push({ value, dropped: false, exploded, success: false });
  for (let n = 0; n < g.count; n++) {
    let v = rollDie(ctx, g.sides);
    push(v, false);
    if (g.explode) {
      const c = g.explode.cond;
      let depth = 0;
      while ((c ? CMP[c.op](v, c.t) : v === hi) && depth < MAX_EXPLODE) {
        v = rollDie(ctx, g.sides);
        push(v, true);
        depth += 1;
      }
    }
  }
  if (g.keep) {
    const idx = dice.map((_, i) => i).sort((a, b) => dice[a].value - dice[b].value || a - b);
    const n = dice.length;
    const m = g.keep.n;
    let drop;
    if (g.keep.kind === 'kh') drop = idx.slice(0, Math.max(n - m, 0));
    else if (g.keep.kind === 'kl') drop = idx.slice(Math.min(m, n));
    else if (g.keep.kind === 'dh') drop = idx.slice(Math.max(n - m, 0));
    else drop = idx.slice(0, Math.min(m, n));
    for (const i of drop) dice[i].dropped = true;
  }
  let total = 0;
  for (const d of dice) {
    if (d.dropped) continue;
    if (g.success) {
      d.success = CMP[g.success.op](d.value, g.success.t);
      total += d.success ? 1 : 0;
    } else total += d.value;
  }
  ctx.groups.push({ notation: notation(g), dice, total });
  return total;
}

const I64 = 1n << 63n;
function checked(v) {
  if (v >= I64 || v < -I64) throw new DiceError('overflow');
  return v;
}

// Arithmetic is BigInt so the same overflow rule as i64 holds.
function evaluate(ctx, n, out) {
  if (n.t === 'num') {
    out.push(String(n.v));
    return BigInt(n.v);
  }
  if (n.t === 'dice') {
    const v = rollGroup(ctx, n);
    out.push(String(v));
    return BigInt(v);
  }
  const a = evaluate(ctx, n.l, out);
  out.push(` ${n.op} `);
  const b = evaluate(ctx, n.r, out);
  return checked(n.op === '+' ? a + b : n.op === '-' ? a - b : a * b);
}

/** Parse and roll `input` with `rng`. Throws a DiceError. */
export function rollExpr(input, rng) {
  const cleaned = [...input]
    .filter((c) => !/\s/.test(c))
    .join('')
    .toLowerCase();
  if (cleaned === '') throw new DiceError('empty');
  for (let i = 0; i < cleaned.length; i++)
    if (cleaned.charCodeAt(i) > 127) throw new DiceError('syntax', i);
  if (cleaned.length > MAX_EXPR_CHARS) throw new DiceError('too-long');
  const p = new Parser(cleaned);
  const node = p.expr(0);
  if (p.i !== cleaned.length) throw new DiceError('syntax', p.i);
  const ctx = { rng, rolls: 0, groups: [] };
  const out = [];
  const total = evaluate(ctx, node, out);
  return { expression: cleaned, total: Number(total), groups: ctx.groups, breakdown: out.join('') };
}

// --- the roll record -------------------------------------------------------------------------------------

/** A 64-bit seed as 16 lowercase hex digits (what the UI shows next to a roll). */
export const seedToHex = (seed) => BigInt.asUintN(64, seed).toString(16).padStart(16, '0');
export const seedFromHex = (hex) => {
  if (typeof hex !== 'string' || !/^[0-9a-f]{16}$/.test(hex)) throw new DiceError('bad-seed');
  return BigInt('0x' + hex);
};

/** 64 random bits from the CSPRNG. */
export function randomSeed(cryptoImpl = globalThis.crypto) {
  const a = new Uint32Array(2);
  cryptoImpl.getRandomValues(a);
  return (BigInt(a[0]) << 32n) | BigInt(a[1]);
}

/**
 * The shared record of one roll: who rolled what, the seed, and the outcome. Anyone can re-roll `expression`
 * with `seed` and must get the same dice and total back (see `verifyRecord`).
 */
export function makeRollRecord({ seq, by, expression, seed, label = '', at = 0 }) {
  const result = rollExpr(expression, new Rng(seed));
  return {
    seq,
    by,
    label,
    at,
    expression: result.expression,
    seed: seedToHex(seed),
    total: result.total,
    breakdown: result.breakdown,
    groups: result.groups,
  };
}

/** True when re-rolling the record's expression with its seed gives exactly its dice and total. */
export function verifyRecord(rec) {
  try {
    const again = rollExpr(rec.expression, new Rng(seedFromHex(rec.seed)));
    return (
      again.total === rec.total &&
      again.breakdown === rec.breakdown &&
      JSON.stringify(again.groups) === JSON.stringify(rec.groups)
    );
  } catch {
    return false;
  }
}

/** One line of text for a record: `2d20kh1+5 = 17 (2d20kh1: [12, ~~5~~])`. */
export function describeRecord(rec) {
  const face = (d) =>
    d.dropped ? `~~${d.value}~~` : d.exploded ? `${d.value}!` : String(d.value);
  const detail = rec.groups
    .map((g) => `${g.notation}: [${g.dice.map(face).join(', ')}]`)
    .join('  ');
  const head = `${rec.expression} = ${rec.total}`;
  return rec.groups.length === 0 ? head : `${head} (${detail})`;
}
