// The force-directed layout of the invite graph, computed by `sr-graph-wasm` (ADM-GRAPH-002) and loaded on
// first use so the admin bundle does not carry it. Positions come back as `[x0, y0, x1, y1, ...]`.
import type { GraphApi } from '@sideroom/wasm';

let loaded: Promise<GraphApi> | undefined;

/** Lay out `nodeCount` nodes joined by `edges` (`[a0, b0, a1, b1, ...]`). Rejects when WASM cannot load. */
export async function layoutGraph(nodeCount: number, edges: Uint32Array): Promise<Float32Array> {
  loaded ??= import('@sideroom/wasm').then((m) => m.loadGraph());
  try {
    return (await loaded).layout(nodeCount, edges);
  } catch (e) {
    loaded = undefined; // let a retry load it again
    throw e;
  }
}
