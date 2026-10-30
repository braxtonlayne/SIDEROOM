import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadTools } from '../src/index.js';

/**
 * UTL-IMG-001: the JS bridge over the GIF builder and the palette extraction in sr-tools-wasm. The Rust tests
 * (including `wasm-pack test --node`, which decodes a 10-frame sequence with the `gif` crate) cover the
 * encoding; this checks the facade. Needs `pnpm build`; SR_WASM_REQUIRED=1 fails instead of skipping.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/tools/tools_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

const solid = (n: number, r: number, g: number, b: number) =>
  Uint8Array.from({ length: n * 4 }, (_, i) => [r, g, b, 255][i % 4] as number);

describe.skipIf(!built)('tools image facade', () => {
  it('builds a GIF with the right header, size and trailer', async () => {
    const tools = await loadTools(readFileSync(wasmPath));
    const b = tools.gif.create(8, 4, 10);
    expect(b.maxFrames).toBe(150);
    for (let i = 0; i < 3; i++) b.addFrame(solid(32, 40 * i, 20, 200));
    expect(b.frameCount).toBe(3);
    const bytes = b.finish();
    expect(new TextDecoder().decode(bytes.slice(0, 6))).toBe('GIF89a');
    expect(bytes[6] | (bytes[7] << 8)).toBe(8);
    expect(bytes[8] | (bytes[9] << 8)).toBe(4);
    expect(bytes[bytes.length - 1]).toBe(0x3b);
    b.free();
  });

  it('refuses the frame past 15 s, a wrong-sized frame and an empty clip, with codes', async () => {
    const tools = await loadTools(readFileSync(wasmPath));
    expect(tools.gif.limits).toMatchObject({ maxDurationMs: 15000, maxBytes: 20 * 1024 * 1024 });
    const b = tools.gif.create(2, 2, 100); // 1 s per frame: 15 fit
    for (let i = 0; i < 15; i++) b.addFrame(solid(4, 1, 2, 3));
    expect(() => b.addFrame(solid(4, 1, 2, 3))).toThrow(/GIF_TOO_LONG/);
    const c = tools.gif.create(2, 2, 10);
    expect(() => c.addFrame(solid(3, 1, 2, 3))).toThrow(/GIF_BAD_FRAME/);
    expect(() => c.finish()).toThrow(/GIF_EMPTY/);
    expect(() => tools.gif.create(0, 5, 10)).toThrow(/GIF_BAD_SIZE/);
  });

  it('extracts a palette as hex colours with shares', async () => {
    const tools = await loadTools(readFileSync(wasmPath));
    const px = new Uint8Array([...solid(30, 255, 0, 0), ...solid(10, 0, 0, 255)]);
    const p = tools.palette.extract(px, 2);
    expect(p.map((c) => c.hex)).toEqual(['#ff0000', '#0000ff']);
    expect(p[0]?.share).toBeCloseTo(0.75);
  });
});
