import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadGraph } from '../src/index.js';

/**
 * ADM-GRAPH-002: the Invite Graph layout runs in the browser as WASM. The algorithm is tested
 * natively in crates/sr-graph-wasm; this checks the typed-array bridge. Needs `pnpm build` first.
 * Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail instead of skipping when the build is missing.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/graph/graph_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

function dist(p: Float32Array, a: number, b: number): number {
  return Math.hypot(p[2 * a]! - p[2 * b]!, p[2 * a + 1]! - p[2 * b + 1]!);
}

describe.runIf(built)('sr-graph-wasm layout', () => {
  it('returns x/y pairs as a Float32Array, deterministic per seed', async () => {
    const g = await loadGraph(readFileSync(wasmPath));
    const edges = [0, 1, 1, 2, 2, 3, 3, 0, 3, 4];
    const a = g.layout(5, edges);
    const b = g.layout(5, new Uint32Array(edges));
    expect(a).toBeInstanceOf(Float32Array);
    expect(a.length).toBe(10);
    expect(Array.from(a)).toEqual(Array.from(b));
    expect(Array.from(g.layout(5, edges, { seed: 99 }))).not.toEqual(Array.from(a));
  });

  it('pulls linked nodes together and keeps strangers apart', async () => {
    const g = await loadGraph(readFileSync(wasmPath));
    const edges: number[] = [];
    for (const base of [0, 5]) {
      for (let i = 0; i < 5; i++) for (let j = i + 1; j < 5; j++) edges.push(base + i, base + j);
    }
    edges.push(4, 5);
    const p = g.layout(10, edges);
    const within = (dist(p, 0, 1) + dist(p, 1, 2) + dist(p, 2, 3)) / 3;
    const across = (dist(p, 0, 5) + dist(p, 1, 6) + dist(p, 2, 7)) / 3;
    expect(within * 1.5).toBeLessThan(across);
    expect(Array.from(p).every(Number.isFinite)).toBe(true);
  });

  it('handles empty and single-node graphs and rejects dangling edges', async () => {
    const g = await loadGraph(readFileSync(wasmPath));
    expect(g.layout(0, []).length).toBe(0);
    expect(Array.from(g.layout(1, []))).toEqual([0, 0]);
    expect(() => g.layout(3, [0, 7])).toThrow(/node 7/);
    expect(() => g.layout(3, [0, 1, 2])).toThrow(/pairs/);
    expect(g.defaults.iterations).toBe(300);
  });

  it('lays out a few thousand nodes quickly', async () => {
    const g = await loadGraph(readFileSync(wasmPath));
    const n = 3000;
    const edges: number[] = [];
    for (let i = 1; i < n; i++) edges.push(Math.floor((i - 1) / 3), i);
    const t0 = performance.now();
    const p = g.layout(n, edges, { iterations: 60 });
    expect(p.length).toBe(2 * n);
    expect(performance.now() - t0).toBeLessThan(20_000);
  });
});
