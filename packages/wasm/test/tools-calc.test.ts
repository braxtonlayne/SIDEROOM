import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadTools } from '../src/index.js';

/**
 * UTL-CALC-001: the JSON bridge over sr-tools-wasm's calc, units and percent modules. The Rust unit tests carry the
 * 200+ expression cases; this checks the exports exist under their JS names and return what the web tools parse.
 * Needs `pnpm build`. Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail instead of skipping when the build is missing.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/tools/tools_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

interface Raw {
  calcEval(expr: string, degrees: boolean, ans: number): string;
  calcFormat(x: number): string;
  programmerEval(expr: string, bits: number, signed: boolean, base: number): string;
  programmerConvertBase(text: string, from: number, to: number, bits: number, signed: boolean): string;
  unitsList(): string;
  unitsConvert(value: number, from: string, to: string): string;
  unitsConvertAll(value: number, from: string): string;
  unitsQuery(text: string): string;
  percentPhrase(text: string): string;
  percentCompute(kind: string, a: number, b: number): string;
}

describe.skipIf(!built)('sr-tools-wasm calculator exports', () => {
  const load = async () => (await loadTools(readFileSync(wasmPath))) as unknown as Raw;

  it('evaluates expressions and reports errors with a code and a position', async () => {
    const m = await load();
    expect(JSON.parse(m.calcEval('2*(3+4)', false, Number.NaN))).toEqual({ ok: true, value: 14, text: '14' });
    expect(JSON.parse(m.calcEval('sin(30)', true, Number.NaN)).text).toBe('0.5');
    expect(JSON.parse(m.calcEval('ans*2', false, 21)).text).toBe('42');
    expect(JSON.parse(m.calcEval('1/0', false, Number.NaN))).toEqual({
      ok: false,
      error: { code: 'divide_by_zero', pos: 1, detail: '' },
    });
    expect(m.calcFormat(0.1 + 0.2)).toBe('0.3');
  });

  it('does programmer arithmetic at a width', async () => {
    const m = await load();
    const r = JSON.parse(m.programmerEval('127 + 1', 8, true, 10));
    expect(r.text).toBe('-128');
    expect(r.wrapped).toBe(true);
    expect(r.views).toMatchObject({ hex: '80', unsigned: '128', bin: '1000 0000', bits: 8 });
    expect(JSON.parse(m.programmerConvertBase('FF', 16, 2, 32, true))).toEqual({ ok: true, text: '11111111' });
    expect(JSON.parse(m.programmerEval('0xFFFFFFFFFFFFFFFF', 64, false, 10)).text).toBe('18446744073709551615');
  });

  it('converts units and parses a text query', async () => {
    const m = await load();
    const list = JSON.parse(m.unitsList());
    expect(list.categories.map((c: { category: string }) => c.category)).toEqual([
      'length', 'mass', 'volume', 'temperature', 'data', 'speed', 'time', 'area',
    ]);
    expect(JSON.parse(m.unitsConvert(100, 'c', 'f')).text).toBe('212');
    expect(JSON.parse(m.unitsQuery('5 km to mi')).text).toBe('3.10685596119');
    expect(JSON.parse(m.unitsConvertAll(1, 'm')).rows).toHaveLength(10);
    expect(JSON.parse(m.unitsConvert(1, 'm', 'kg')).error.code).toBe('category_mismatch');
  });

  it('reads percent phrases and computes the form', async () => {
    const m = await load();
    expect(JSON.parse(m.percentPhrase('15% of 80'))).toMatchObject({ ok: true, kind: 'of', text: '12' });
    expect(JSON.parse(m.percentPhrase('80 + 15%')).text).toBe('92');
    expect(JSON.parse(m.percentCompute('change', 50, 65)).text).toBe('30');
    expect(JSON.parse(m.percentPhrase('banana')).error.code).toBe('no_phrase');
  });
});
