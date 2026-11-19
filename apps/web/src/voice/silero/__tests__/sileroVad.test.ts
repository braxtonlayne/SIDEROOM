import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../frames.worklet.ts?worker&url', () => ({ default: '/app/frames.worklet.js' }));
vi.mock('../assets', () => ({
  sileroAssets: { modelUrl: '/app/silero_vad-x.onnx', wasmUrl: '/app/ort-wasm-x.wasm' },
}));

import { FRAME_MS, MAX_INFERENCE_ERRORS, SLOW_FALLBACK_MS } from '../constants';
import type { FromWorker, ToWorker } from '../protocol';
import { SileroVad, type FramesNode, type SileroDeps, type WorkerLike } from '../sileroVad';

class FakeWorker implements WorkerLike {
  sent: { m: ToWorker; transfer?: Transferable[] }[] = [];
  terminated = false;
  onmessage: ((ev: { data: FromWorker }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onmessageerror: ((ev: unknown) => void) | null = null;
  postMessage(m: ToWorker, transfer?: Transferable[]) {
    this.sent.push({ m, ...(transfer ? { transfer } : {}) });
  }
  terminate() {
    this.terminated = true;
  }
  say(m: FromWorker) {
    this.onmessage?.({ data: m });
  }
}

class FakeNode implements FramesNode {
  port: FramesNode['port'] = { onmessage: null };
  onprocessorerror: ((ev: unknown) => void) | null = null;
  connected: unknown[] = [];
  disconnects = 0;
  connect(d: unknown) {
    this.connected.push(d);
    return d;
  }
  disconnect() {
    this.disconnects++;
  }
}

function rig(opts: { worklet?: 'ok' | 'reject' | 'throw'; createWorker?: 'ok' | 'throw' } = {}) {
  const worker = new FakeWorker();
  const node = new FakeNode();
  const clock = { t: 0 };
  const deps: SileroDeps = {
    createWorker: () => {
      if (opts.createWorker === 'throw') throw new Error('no worker');
      return worker;
    },
    addWorklet: () => {
      if (opts.worklet === 'throw') throw new Error('no worklet');
      return opts.worklet === 'reject' ? Promise.reject(new Error('addModule')) : Promise.resolve();
    },
    createNode: () => node,
    modelUrl: '/app/silero_vad-x.onnx',
    wasmUrl: '/app/ort-wasm-x.wasm',
    now: () => clock.t,
  };
  const gain = { gain: { value: 1 }, connect: vi.fn(), disconnect: vi.fn() };
  const ctx = { createGain: () => gain, destination: { id: 'dest' } } as unknown as AudioContext;
  const source = { connect: vi.fn(), disconnect: vi.fn() } as unknown as AudioNode;
  const vad = new SileroVad(deps);
  const states: [string, string | null][] = [];
  vad.onState((s, r) => states.push([s, r]));
  return { vad, worker, node, ctx, source, clock, states, gain };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

async function ready(r: ReturnType<typeof rig>) {
  r.vad.start(r.ctx, r.source);
  r.worker.say({ type: 'ready' });
  await flush();
}
const prob = (r: ReturnType<typeof rig>, p: number, ms = 3) =>
  r.worker.say({ type: 'prob', prob: p, ms, meanMs: ms, maxMs: ms });

describe('SileroVad client (D-1050, D-1051)', () => {
  beforeEach(() => vi.useRealTimers());

  it('loads, then is ready once the worker and the worklet are; taps the source before anything else', async () => {
    const r = rig();
    r.vad.start(r.ctx, r.source);
    expect(r.vad.state).toBe('loading');
    expect(r.worker.sent[0]!.m).toEqual({
      type: 'init',
      modelUrl: '/app/silero_vad-x.onnx',
      wasmUrl: '/app/ort-wasm-x.wasm',
    });
    await flush();
    expect(r.vad.state).toBe('loading'); // the worklet is in, the model is not
    r.worker.say({ type: 'ready' });
    expect(r.vad.state).toBe('ready');
    expect(r.source.connect).toHaveBeenCalledWith(r.node);
    expect(r.node.connected).toEqual([r.gain]); // a muted sink so the node is pulled, never heard
    expect(r.gain.gain.value).toBe(0);
    expect(r.states.map((s) => s[0])).toEqual(['loading', 'ready']);
  });

  it('sends each frame to the worker as a transfer, and reports probabilities and the figures', async () => {
    const r = rig();
    await ready(r);
    const frame = new Float32Array(512);
    r.node.port.onmessage?.({ data: frame });
    const sent = r.worker.sent.at(-1)!;
    expect(sent.m.type).toBe('frame');
    expect(sent.transfer).toEqual([frame.buffer]);
    const got: number[] = [];
    r.vad.onProb((p) => got.push(p));
    prob(r, 0.9, 4);
    prob(r, 0.1, 6);
    expect(got).toEqual([0.9, 0.1]);
    expect(r.vad.frames).toBe(2);
    expect(r.vad.speechFrames).toBe(1);
    expect(r.vad.maxMs).toBe(6);
  });

  it('falls back with the worker reason: model-fetch, runtime-load, session', () => {
    for (const reason of ['model-fetch', 'runtime-load', 'session'] as const) {
      const r = rig();
      r.vad.start(r.ctx, r.source);
      r.worker.say({ type: 'error', reason });
      expect(r.vad.state).toBe('fallback');
      expect(r.vad.reason).toBe(reason);
      expect(r.worker.terminated).toBe(true);
    }
  });

  it('falls back when the worker dies (onerror), when it cannot be made, and when messages cannot be read', async () => {
    const a = rig();
    await ready(a);
    a.worker.onerror?.({});
    expect([a.vad.state, a.vad.reason]).toEqual(['fallback', 'worker']);
    const b = rig({ createWorker: 'throw' });
    b.vad.start(b.ctx, b.source);
    expect([b.vad.state, b.vad.reason]).toEqual(['fallback', 'worker']);
    const c = rig();
    await ready(c);
    c.worker.onmessageerror?.({});
    expect(c.vad.reason).toBe('worker');
  });

  it('falls back when the worklet cannot be added (rejected or thrown) or the node fails', async () => {
    const a = rig({ worklet: 'reject' });
    a.vad.start(a.ctx, a.source);
    await flush();
    expect([a.vad.state, a.vad.reason]).toEqual(['fallback', 'worklet']);
    const b = rig({ worklet: 'throw' });
    b.vad.start(b.ctx, b.source);
    expect([b.vad.state, b.vad.reason]).toEqual(['fallback', 'worklet']);
    const c = rig();
    await ready(c);
    c.node.onprocessorerror?.({});
    expect(c.vad.reason).toBe('worklet');
  });

  it('falls back when frames take longer than their duration for three seconds, not before', async () => {
    const r = rig();
    await ready(r);
    const slow = FRAME_MS + 5;
    r.clock.t = 1000;
    prob(r, 0.2, slow);
    r.clock.t += SLOW_FALLBACK_MS - 1;
    prob(r, 0.2, slow);
    expect(r.vad.state).toBe('ready');
    r.clock.t += 1;
    prob(r, 0.2, slow);
    expect([r.vad.state, r.vad.reason]).toEqual(['fallback', 'slow']);
  });

  it('forgives a slow stretch that ends: five in-time frames reset the three seconds', async () => {
    const r = rig();
    await ready(r);
    r.clock.t = 1000;
    prob(r, 0.2, FRAME_MS + 5);
    r.clock.t += 2000;
    for (let i = 0; i < 5; i++) prob(r, 0.2, 5);
    r.clock.t += 2000;
    prob(r, 0.2, FRAME_MS + 5);
    expect(r.vad.state).toBe('ready');
  });

  it('falls back when too many frames wait for a worker that does not answer', async () => {
    const r = rig();
    await ready(r);
    for (let i = 0; i < 200 && r.vad.state === 'ready'; i++)
      r.node.port.onmessage?.({ data: new Float32Array(512) });
    expect([r.vad.state, r.vad.reason]).toEqual(['fallback', 'slow']);
  });

  it('falls back after ten inference errors in a row, but not when a good frame comes between', async () => {
    const r = rig();
    await ready(r);
    for (let i = 0; i < MAX_INFERENCE_ERRORS - 1; i++) r.worker.say({ type: 'frameError' });
    prob(r, 0.3);
    for (let i = 0; i < MAX_INFERENCE_ERRORS - 1; i++) r.worker.say({ type: 'frameError' });
    expect(r.vad.state).toBe('ready');
    r.worker.say({ type: 'frameError' });
    expect([r.vad.state, r.vad.reason]).toEqual(['fallback', 'inference']);
  });

  it('stop frees the worker and the nodes, goes idle and ignores late messages', async () => {
    const r = rig();
    await ready(r);
    r.vad.stop();
    expect(r.worker.terminated).toBe(true);
    expect(r.node.disconnects).toBe(1);
    expect(r.vad.state).toBe('idle');
    const before = r.vad.frames;
    r.worker.onmessage?.({ data: { type: 'prob', prob: 0.9, ms: 1, meanMs: 1, maxMs: 1 } });
    expect(r.vad.frames).toBe(before);
  });

  it('a stop before the worklet is added never wires the graph', async () => {
    const r = rig();
    r.vad.start(r.ctx, r.source);
    r.vad.stop();
    await flush();
    r.worker.onmessage?.({ data: { type: 'ready' } });
    expect(r.source.connect).not.toHaveBeenCalled();
    expect(r.vad.state).toBe('idle');
  });
});
