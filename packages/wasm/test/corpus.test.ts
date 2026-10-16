import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadMarkdown } from '../src/index.js';

/**
 * LNG-MD-005 (Q86): the WASM build must give the same AST, plain text and jumbo flag as the
 * Rust tests for every case in crates/sr-markdown/tests/corpus. Needs `pnpm build` first.
 * Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail instead of skipping when the build is missing.
 */
interface Case {
  name: string;
  input: string;
  ast: unknown[];
  plain: string;
  jumbo: boolean;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpusDir = join(here, '../../../crates/sr-markdown/tests/corpus');
const wasmPath = join(here, '../dist/markdown/markdown_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

const cases: Case[] = readdirSync(corpusDir)
  .filter((f) => f.endsWith('.json'))
  .sort()
  .flatMap(
    (f) => (JSON.parse(readFileSync(join(corpusDir, f), 'utf8')) as { cases: Case[] }).cases,
  );

describe.runIf(built)('sr-markdown WASM matches the shared corpus', () => {
  it('covers at least 500 cases', () => {
    expect(cases.length).toBeGreaterThanOrEqual(500);
  });

  it('produces identical AST, plain text and jumbo flags', async () => {
    const md = await loadMarkdown(readFileSync(wasmPath));
    for (const c of cases) {
      expect(md.parse(c.input), `${c.name} ast`).toEqual(c.ast);
      expect(md.toPlainText(c.input), `${c.name} plain`).toBe(c.plain);
      expect(md.isJumbo(c.input), `${c.name} jumbo`).toBe(c.jumbo);
    }
  });
});
