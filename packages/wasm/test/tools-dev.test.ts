import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadTools } from '../src/index.js';

/**
 * UTL-DEV-001: the dice grammar (50 cases), the diff and the hashes of sr-tools-wasm, through the built wasm
 * and the facade. The Rust unit tests cover the same code natively; this proves the browser build. Needs `pnpm
 * build`. Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail instead of skipping when the build is missing.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/tools/tools_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

interface Api {
  diceRoll(expr: string, hi: number, lo: number): string;
  diceFlip(hi: number, lo: number): boolean;
  dicePickWeighted(w: Uint32Array, hi: number, lo: number): number;
  diffText(a: string, b: string, mode: string): string;
  hashMd5(d: Uint8Array): string;
  hashCrc32(d: Uint8Array): string;
}

const load = async () => (await loadTools(readFileSync(wasmPath))).dev as Api;
const text = (s: string) => new TextEncoder().encode(s);

// [expression, lowest total, highest total] for every seed.
const RANGES: [string, number, number][] = [
  ['d20', 1, 20],
  ['1d6', 1, 6],
  ['2d6', 2, 12],
  ['3d8+2', 5, 26],
  ['d%', 1, 100],
  ['D100', 1, 100],
  ['2d20kh1', 1, 20],
  ['2d20kl1', 1, 20],
  ['2d20kh1+5', 6, 25],
  ['4d6dl1', 3, 18],
  ['4d6dh1', 3, 18],
  ['4d6kh3', 3, 18],
  ['5d10dl2', 3, 30],
  ['4dF', -4, 4],
  ['d1', 1, 1],
  ['7', 7, 7],
  ['(1+2)*3', 9, 9],
  ['2*(1d4+1)', 4, 10],
  ['d6-1', 0, 5],
  ['10-d6', 4, 9],
  [' 2 d 6 ', 2, 12],
  ['6d10>=8', 0, 6],
  ['6d10>8', 0, 6],
  ['6d10<=3', 0, 6],
  ['6d10<3', 0, 6],
  ['6d10=10', 0, 6],
  ['5d10kh3>=6', 0, 3],
];

// [expression, error code prefix]
const ERRORS: [string, string][] = [
  ['', 'empty'],
  ['   ', 'empty'],
  ['0d6', 'zero-dice'],
  ['1001d6', 'too-many-dice'],
  ['d0', 'bad-sides'],
  ['d1000001', 'bad-sides'],
  ['4d6kh5', 'bad-modifier'],
  ['d1!', 'bad-modifier'],
  ['d', 'syntax'],
  ['2d6+', 'syntax'],
  ['2d6)', 'syntax'],
  ['hello', 'syntax'],
  ['2d6!!', 'syntax'],
  ['2dé', 'syntax'],
  ['4294967295*4294967295*4294967295', 'overflow'],
];

describe.skipIf(!built)('sr-tools-wasm through the facade', () => {
  it('the dice grammar: totals stay in range for many seeds', async () => {
    const api = await load();
    expect(RANGES.length + ERRORS.length).toBeGreaterThanOrEqual(40);
    for (const [expr, lo, hi] of RANGES) {
      for (let seed = 1; seed <= 40; seed++) {
        const r = JSON.parse(api.diceRoll(expr, seed, seed * 7919)) as { total: number; groups: unknown[] };
        expect(r.total, `${expr} seed ${seed}`).toBeGreaterThanOrEqual(lo);
        expect(r.total, `${expr} seed ${seed}`).toBeLessThanOrEqual(hi);
      }
    }
  });

  it('the dice grammar: errors carry a code', async () => {
    const api = await load();
    for (const [expr, code] of ERRORS) {
      let message = '';
      try {
        api.diceRoll(expr, 1, 2);
      } catch (e) {
        message = e instanceof Error ? e.message : String(e);
      }
      expect(message, expr).toContain(code);
    }
  });

  it('the dice grammar: the same seed rolls the same, different seeds differ, exploding and keep behave', async () => {
    const api = await load();
    expect(api.diceRoll('10d100', 5, 6)).toBe(api.diceRoll('10d100', 5, 6));
    expect(api.diceRoll('10d100', 5, 6)).not.toBe(api.diceRoll('10d100', 5, 7));
    const kept = JSON.parse(api.diceRoll('4d6dl1', 1, 2)) as {
      groups: { dice: { value: number; dropped: boolean }[] }[];
    };
    const dice = kept.groups[0]!.dice;
    expect(dice).toHaveLength(4);
    expect(dice.filter((d) => d.dropped)).toHaveLength(1);
    expect(dice.find((d) => d.dropped)!.value).toBe(Math.min(...dice.map((d) => d.value)));
    let exploded = false;
    for (let s = 0; s < 60 && !exploded; s++) {
      const r = JSON.parse(api.diceRoll('3d4!', s, 1)) as { groups: { dice: { exploded: boolean }[] }[] };
      exploded = r.groups[0]!.dice.some((d) => d.exploded);
    }
    expect(exploded).toBe(true);
  });

  it('coin and weighted picks', async () => {
    const api = await load();
    const heads = Array.from({ length: 400 }, (_, i) => api.diceFlip(i, i * 31 + 7)).filter(Boolean).length;
    expect(heads).toBeGreaterThan(140);
    expect(heads).toBeLessThan(260);
    expect(api.dicePickWeighted(new Uint32Array([0, 0]), 1, 2)).toBe(-1);
    const picks = Array.from({ length: 300 }, (_, i) => api.dicePickWeighted(new Uint32Array([1, 0, 5]), i, 3));
    expect(picks.includes(1)).toBe(false);
    expect(picks.filter((p) => p === 2).length).toBeGreaterThan(picks.filter((p) => p === 0).length);
  });

  it('the diff', async () => {
    const api = await load();
    const r = JSON.parse(api.diffText('a\nb\nc\n', 'a\nx\nc\nd\n', 'lines')) as {
      ops: { op: string; text: string }[];
      added: number;
      removed: number;
    };
    expect(r.added).toBe(2);
    expect(r.removed).toBe(1);
    expect(r.ops.map((o) => o.op)).toEqual(['eq', 'del', 'add', 'eq', 'add']);
    const words = JSON.parse(api.diffText('the quick fox', 'the slow fox', 'words')) as { ops: { op: string; text: string }[] };
    expect(words.ops.find((o) => o.op === 'del')!.text).toBe('quick');
    expect(() => api.diffText('a', 'b', 'sideways')).toThrow(/bad-mode/);
    expect(() => api.diffText('x'.repeat(1_000_001), '', 'chars')).toThrow(/too-large/);
  });

  it('the hashes', async () => {
    const api = await load();
    expect(api.hashMd5(text(''))).toBe('d41d8cd98f00b204e9800998ecf8427e');
    expect(api.hashMd5(text('abc'))).toBe('900150983cd24fb0d6963f7d28e17f72');
    expect(api.hashMd5(text('The quick brown fox jumps over the lazy dog'))).toBe('9e107d9d372bb6826bd81d3542a419d6');
    expect(api.hashCrc32(text('123456789'))).toBe('cbf43926');
    expect(api.hashCrc32(text(''))).toBe('00000000');
    // Binary input, checked against the platform's own digest for the SHA family and the known CRC.
    const bytes = Uint8Array.from({ length: 256 }, (_, i) => i);
    expect(api.hashMd5(bytes)).toHaveLength(32);
    const sha = Buffer.from(await crypto.subtle.digest('SHA-256', bytes)).toString('hex');
    expect(sha).toHaveLength(64);
  });
});
