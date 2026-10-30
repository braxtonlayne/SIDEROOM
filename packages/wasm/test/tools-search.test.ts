import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadTools } from '../src/index.js';

/**
 * UTL-SRC-001: the facade over the search in sr-tools-wasm. The Rust tests cover the three stages and the
 * corpus; this checks the JS bridge (JSON in and out, the channel and people list). Needs `pnpm build`.
 * Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail instead of skipping when the build is missing.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/tools/tools_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

describe.skipIf(!built)('tools search facade', () => {
  it('finds a tool by name, by typo and by task', async () => {
    const tools = await loadTools(readFileSync(wasmPath));
    const s = tools.search.create();
    expect(s.search('calc').hits[0]).toMatchObject({ id: 'calculator', stage: 1 });
    const typo = s.search('calcualtor');
    expect(typo.hits[0]).toMatchObject({ id: 'calculator', stage: 2 });
    expect(typo.didYouMean).toBe('Calculator');
    expect(s.search('make my image smaller').hits[0]).toMatchObject({ id: 'image-studio', stage: 3 });
    expect(s.search('').hits).toEqual([]);
    s.free();
  });

  it('indexes channels and people from the client cache', async () => {
    const tools = await loadTools(readFileSync(wasmPath));
    const s = tools.search.create();
    expect(
      s.setEntities([
        { kind: 'channel', id: 'c1', name: 'general' },
        { kind: 'person', id: 'u1', name: 'Alex', aliases: ['alexander'] },
      ]),
    ).toBe(2);
    expect(s.search('#gen').hits.map((h) => h.id)).toEqual(['c1']);
    expect(s.search('alx').hits[0]).toMatchObject({ kind: 'person', id: 'u1' });
    expect(s.search('unit converter').hits.some((h) => h.id === 'units')).toBe(true);
    expect(() => s.setEntities({} as never)).toThrow();
    s.free();
  });

  it('survives hostile input', async () => {
    const tools = await loadTools(readFileSync(wasmPath));
    const s = tools.search.create();
    for (const q of ['\u0000', 'x'.repeat(5000), '\u{1f600}'.repeat(300), '>>>###@@@', 'é́'.repeat(200)]) {
      expect(() => s.search(q, 5)).not.toThrow();
    }
    s.free();
  });
});
