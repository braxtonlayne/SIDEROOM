/**
 * D-1050, D-1051: the detector's client. `start(ctx, source)` adds the frames worklet to the microphone's context,
 * starts the worker and wires one to the other; probabilities come out of `onProb`. It never throws into the
 * caller: a failure is a state, `fallback`, with a reason, and the caller goes back to the energy gate.
 * D-1052: frames go worklet -> this page -> worker, all inside the page. Nothing here touches the network; the
 * worker fetches our own two files (model and wasm).
 * Loaded only through a dynamic `import()` from `../vad` (the host), so a member who never turns it on never loads it.
 */
import workletUrl from './frames.worklet.ts?worker&url';
import { sileroAssets } from './assets';
import { FRAME_MS, MAX_INFERENCE_ERRORS, SLOW_FALLBACK_MS } from './constants';
import { ProbGate, OPEN_PROB } from './probGate';
import type { FromWorker, ToWorker } from './protocol';
import type { VadDetector, VadDetectorState } from '../vad';

export type FallbackReason =
  'model-fetch' | 'runtime-load' | 'session' | 'worker' | 'worklet' | 'slow' | 'inference';

export interface WorkerLike {
  postMessage(m: ToWorker, transfer?: Transferable[]): void;
  terminate(): void;
  onmessage: ((ev: { data: FromWorker }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  onmessageerror?: ((ev: unknown) => void) | null;
}

export interface FramesNode {
  port: {
    onmessage: ((ev: { data: Float32Array }) => void) | null;
    onmessageerror?: ((ev: unknown) => void) | null;
  };
  connect(dest: unknown): unknown;
  disconnect(dest?: unknown): void;
  onprocessorerror?: ((ev: unknown) => void) | null;
}

export interface SileroDeps {
  createWorker(): WorkerLike;
  addWorklet(ctx: AudioContext): Promise<void>;
  createNode(ctx: AudioContext): FramesNode;
  modelUrl: string;
  wasmUrl: string;
  now(): number;
}

export const defaultDeps = (): SileroDeps => ({
  createWorker: () =>
    new Worker(new URL('./vad.worker.ts', import.meta.url), {
      type: 'module',
      name: 'sr-silero-vad',
    }) as unknown as WorkerLike,
  addWorklet: (ctx) => ctx.audioWorklet.addModule(workletUrl),
  createNode: (ctx) =>
    new AudioWorkletNode(ctx, 'sr-silero-frames', {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      channelCount: 1,
      channelCountMode: 'explicit',
      outputChannelCount: [1],
    }) as unknown as FramesNode,
  modelUrl: sileroAssets.modelUrl,
  wasmUrl: sileroAssets.wasmUrl,
  now: () => performance.now(),
});

/** Frames waiting for the worker; more than three seconds' worth means it cannot keep up. */
const MAX_IN_FLIGHT = Math.ceil(SLOW_FALLBACK_MS / FRAME_MS);
/** This many in-time frames in a row end a slow stretch. */
const RECOVER_FRAMES = 5;

export class SileroVad implements VadDetector {
  readonly gate = new ProbGate();
  state: VadDetectorState = 'idle';
  reason: FallbackReason | null = null;
  frames = 0;
  speechFrames = 0;
  meanMs = 0;
  maxMs = 0;

  private worker: WorkerLike | null = null;
  private node: FramesNode | null = null;
  private sink: GainNode | null = null;
  private source: AudioNode | null = null;
  private run = 0;
  private inFlight = 0;
  private slowSince = -1;
  private okStreak = 0;
  private errorStreak = 0;
  private probCbs = new Set<(prob: number) => void>();
  private stateCbs = new Set<(s: VadDetectorState, reason: string | null) => void>();

  constructor(private readonly deps: SileroDeps = defaultDeps()) {}

  onProb(cb: (prob: number) => void): () => void {
    this.probCbs.add(cb);
    return () => this.probCbs.delete(cb);
  }

  onState(cb: (s: VadDetectorState, reason: string | null) => void): () => void {
    this.stateCbs.add(cb);
    return () => this.stateCbs.delete(cb);
  }

  /** Start on the microphone's context, tapping `source` (before any gain). Reports; never throws. */
  start(ctx: AudioContext, source: AudioNode): void {
    this.teardown();
    const run = ++this.run;
    this.frames = this.speechFrames = 0;
    this.meanMs = this.maxMs = 0;
    this.reason = null;
    this.gate.reset();
    this.set('loading');
    let ready = false;
    let worklet = false;
    const proceed = () => {
      if (run !== this.run || !ready || !worklet) return;
      try {
        const node = this.deps.createNode(ctx);
        this.node = node;
        node.onprocessorerror = () => this.fail('worklet', run);
        node.port.onmessage = (ev) => this.onFrame(ev.data, run);
        node.port.onmessageerror = () => this.fail('worklet', run);
        // The node only listens: it feeds a muted sink so the graph pulls it, and nothing is heard.
        const sink = ctx.createGain();
        sink.gain.value = 0;
        this.sink = sink;
        source.connect(node as unknown as AudioNode);
        node.connect(sink);
        sink.connect(ctx.destination);
        this.source = source;
        this.set('ready');
      } catch {
        this.fail('worklet', run);
      }
    };

    let worker: WorkerLike;
    try {
      worker = this.deps.createWorker();
    } catch {
      this.fail('worker', run);
      return;
    }
    this.worker = worker;
    worker.onerror = () => this.fail('worker', run);
    worker.onmessageerror = () => this.fail('worker', run);
    worker.onmessage = (ev) => {
      if (run !== this.run) return;
      const m = ev.data;
      if (m.type === 'ready') {
        ready = true;
        proceed();
      } else if (m.type === 'error') {
        this.fail(m.reason, run);
      } else if (m.type === 'prob') {
        this.onProbReply(m, run);
      } else if (m.type === 'frameError') {
        this.inFlight = Math.max(0, this.inFlight - 1);
        if (++this.errorStreak >= MAX_INFERENCE_ERRORS) this.fail('inference', run);
      }
    };
    try {
      worker.postMessage({
        type: 'init',
        modelUrl: this.deps.modelUrl,
        wasmUrl: this.deps.wasmUrl,
      });
    } catch {
      this.fail('worker', run);
      return;
    }

    let added: Promise<void>;
    try {
      added = this.deps.addWorklet(ctx);
    } catch {
      this.fail('worklet', run);
      return;
    }
    added.then(
      () => {
        worklet = true;
        proceed();
      },
      () => this.fail('worklet', run),
    );
  }

  /** Forget the carried model state and the gate (a new stretch of audio). */
  reset(): void {
    this.gate.reset();
    this.worker?.postMessage({ type: 'reset' });
  }

  stop(): void {
    this.run++;
    this.teardown();
    this.gate.reset();
    this.state = 'idle';
    this.reason = null;
  }

  private onFrame(frame: Float32Array, run: number): void {
    if (run !== this.run || this.state !== 'ready' || !this.worker) return;
    if (this.inFlight >= MAX_IN_FLIGHT) {
      this.fail('slow', run);
      return;
    }
    this.inFlight++;
    this.worker.postMessage({ type: 'frame', frame }, [frame.buffer]);
  }

  private onProbReply(m: Extract<FromWorker, { type: 'prob' }>, run: number): void {
    this.inFlight = Math.max(0, this.inFlight - 1);
    this.errorStreak = 0;
    this.frames++;
    if (m.prob >= OPEN_PROB) this.speechFrames++;
    this.meanMs = m.meanMs;
    this.maxMs = m.maxMs;
    const now = this.deps.now();
    if (m.ms > FRAME_MS) {
      this.okStreak = 0;
      if (this.slowSince < 0) this.slowSince = now;
      if (now - this.slowSince >= SLOW_FALLBACK_MS) {
        this.fail('slow', run);
        return;
      }
    } else if (++this.okStreak >= RECOVER_FRAMES) {
      this.slowSince = -1;
    }
    for (const cb of [...this.probCbs]) cb(m.prob);
  }

  private fail(reason: FallbackReason, run: number): void {
    if (run !== this.run || this.state === 'fallback') return;
    this.teardown();
    this.reason = reason;
    this.gate.reset();
    this.set('fallback');
  }

  private set(state: VadDetectorState): void {
    this.state = state;
    for (const cb of [...this.stateCbs]) cb(state, this.reason);
  }

  private teardown(): void {
    const { worker, node, sink, source } = this;
    this.worker = this.node = this.sink = this.source = null;
    this.inFlight = this.errorStreak = this.okStreak = 0;
    this.slowSince = -1;
    if (worker) {
      worker.onmessage = worker.onerror = null;
      worker.terminate(); // frees the session with the worker
    }
    try {
      if (node) {
        node.port.onmessage = null;
        if (source) source.disconnect(node as unknown as AudioNode);
        node.disconnect();
      }
      sink?.disconnect();
    } catch {
      /* already disconnected */
    }
  }
}

export function createSileroVad(deps?: SileroDeps): SileroVad {
  return new SileroVad(deps);
}
