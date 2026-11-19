/**
 * D-1050: the detector's worker. One per call. It owns the runtime's session (one thread, no SharedArrayBuffer, so
 * no cross-origin isolation is needed) and answers each 16 kHz frame with a speech probability.
 * D-1052: the only network use is the fetch of our own model file and the runtime's fetch of our own wasm file
 * (`wasmPaths` is set before anything else; never the runtime's default path). Nothing is logged, and no frame,
 * probability or timing leaves the page. The runtime is imported here and nowhere else, so it is this worker's chunk.
 */
import { SileroRunner, type SessionLike, type TensorCtor } from './runner';
import type { FromWorker, ToWorker } from './protocol';

const scope = self as unknown as {
  postMessage(m: FromWorker): void;
  onmessage: ((ev: MessageEvent<ToWorker>) => void) | null;
};
const post = (m: FromWorker) => scope.postMessage(m);

let runner: SileroRunner | null = null;
let initialising = false;

async function init(modelUrl: string, wasmUrl: string): Promise<void> {
  if (initialising || runner) return;
  initialising = true;
  let bytes: Uint8Array;
  try {
    const res = await fetch(modelUrl, { credentials: 'same-origin' });
    if (!res.ok) throw new Error('model');
    bytes = new Uint8Array(await res.arrayBuffer());
  } catch {
    post({ type: 'error', reason: 'model-fetch' });
    return;
  }
  let ort: typeof import('onnxruntime-web/wasm');
  try {
    ort = await import('onnxruntime-web/wasm');
    ort.env.wasm.numThreads = 1;
    ort.env.wasm.proxy = false;
    ort.env.wasm.wasmPaths = { wasm: new URL(wasmUrl, self.location.href).href };
  } catch {
    post({ type: 'error', reason: 'runtime-load' });
    return;
  }
  try {
    const session = await ort.InferenceSession.create(bytes, {
      executionProviders: ['wasm'],
      graphOptimizationLevel: 'all',
    });
    runner = new SileroRunner(
      session as unknown as SessionLike,
      ort.Tensor as unknown as TensorCtor,
    );
    post({ type: 'ready' });
  } catch {
    post({ type: 'error', reason: 'session' });
  }
}

scope.onmessage = (ev) => {
  const m = ev.data;
  if (m.type === 'init') {
    void init(m.modelUrl, m.wasmUrl);
  } else if (m.type === 'reset') {
    runner?.reset();
  } else if (m.type === 'frame' && runner) {
    const r = runner;
    r.process(m.frame).then(
      ({ prob, ms }) => post({ type: 'prob', prob, ms, ...r.stats }),
      () => post({ type: 'frameError' }),
    );
  }
};
