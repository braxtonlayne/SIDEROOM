import { apiFetch } from '../api/client';
import { buildCrashCard, type CrashKind, type CrashReport } from './allowList';
import { detectCapabilities, detectEngine } from './capabilities';
import { parseStack } from './frames';
import { recordTick, ticks } from './ringBuffer';

/** Most reports one page load sends: a crash loop must not become a request loop. */
export const MAX_REPORTS_PER_PAGE = 5;

/** The build's commit (vite `define`), or seven zeros for a build outside git. */
export function buildCommit(): string {
  const c = typeof __SR_COMMIT__ === 'string' ? __SR_COMMIT__ : '';
  return /^[0-9a-f]{7,40}$/.test(c) ? c : '0000000';
}

type RouteName = () => string;
let routeName: RouteName = () => 'unknown';
let sent = 0;
let sending = false;
const seen = new Set<string>();

const GPU_ERRORS = /^(GPU\w*Error|GPUPipelineError|GPUDeviceLostInfo)$/;

/**
 * What kind of crash an error is. A WebAssembly trap (`RuntimeError`, which is what a Rust panic becomes without a
 * panic hook, or an `unreachable`) or any frame inside a `wasm://` module is a WASM panic; a WebGPU error object is a
 * WebGPU panic.
 */
export function classify(error: unknown, fallback: CrashKind): CrashKind {
  const e = error as { name?: unknown; stack?: unknown } | null;
  const name = typeof e?.name === 'string' ? e.name : '';
  if (GPU_ERRORS.test(name)) return 'webgpu_panic';
  const stack = typeof e?.stack === 'string' ? e.stack : '';
  if (
    (typeof WebAssembly === 'object' &&
      typeof WebAssembly.RuntimeError === 'function' &&
      error instanceof WebAssembly.RuntimeError) ||
    name === 'RuntimeError' ||
    stack.includes('wasm://') ||
    stack.includes('wasm-function[')
  ) {
    return 'wasm_panic';
  }
  return fallback;
}

/**
 * The report for one failure. Only the stack's frames are read from the error: its message is never copied, since a
 * message can hold anything (a message body, a URL with a token, a key). `buildCrashCard` strips the rest.
 */
export function reportFor(
  error: unknown,
  kind: CrashKind,
  now: number = Date.now(),
): CrashReport | null {
  const stack = (error as { stack?: unknown } | null)?.stack;
  return buildCrashCard(
    {
      commit: buildCommit(),
      kind: classify(error, kind),
      route: routeName(),
      frames: parseStack(stack),
      caps: { ...detectCapabilities(), engine: detectEngine() },
      at: now,
      ticks: ticks.snapshot(),
    },
    now,
  );
}

/** A local fingerprint: one report per page load for the same kind and top frames. The server deduplicates properly. */
function localKey(r: CrashReport): string {
  const top = r.frames
    .slice(0, 3)
    .map((f) => `${f.file}:${f.line}:${f.fn ?? ''}`)
    .join('|');
  return `${r.kind}|${r.frames.length ? top : r.route}`;
}

export type Sender = (report: CrashReport) => Promise<unknown>;

const defaultSender: Sender = (report) =>
  apiFetch('/crash-reports', { method: 'POST', json: report, retries: 0 });

let sender: Sender = defaultSender;

/** Swap the transport (tests). */
export function setCrashSender(s: Sender | null): void {
  sender = s ?? defaultSender;
}

/**
 * Report a failure: build the allow-listed report and send it, once per distinct crash, at most
 * `MAX_REPORTS_PER_PAGE` times per page load. Never throws and never reports its own failure.
 */
export async function reportCrash(error: unknown, kind: CrashKind): Promise<boolean> {
  if (sending || sent >= MAX_REPORTS_PER_PAGE) return false;
  try {
    const report = reportFor(error, kind);
    if (!report) return false;
    const key = localKey(report);
    if (seen.has(key)) return false;
    seen.add(key);
    sending = true;
    sent += 1;
    await sender(report);
    return true;
  } catch {
    return false;
  } finally {
    sending = false;
  }
}

/** A React error boundary caught `error`. */
export const reportRenderError = (error: unknown): Promise<boolean> =>
  reportCrash(error, 'react_boundary');

/** A WASM module panicked (call this where the app catches it; unhandled ones are caught by the page hooks). */
export const reportWasmPanic = (error: unknown): Promise<boolean> =>
  reportCrash(error, 'wasm_panic');

/** A WebGPU error was caught. */
export const reportWebGpuPanic = (error: unknown): Promise<boolean> =>
  reportCrash(error, 'webgpu_panic');

/** The event an in-page game runtime dispatches to feed the ring without importing app code. */
export const TICK_EVENT = 'sr-crash-tick';

/**
 * Hook the page: unhandled errors, unhandled promise rejections and ticks from in-page runtimes. Call once, after
 * sign-in (the gated app bundle is the only caller, D-385). Returns the function that removes the hooks.
 */
export function installCrashTelemetry(options: { routeName: RouteName }): () => void {
  routeName = options.routeName;
  const onError = (ev: ErrorEvent) => {
    void reportCrash(ev.error ?? new Error('unknown'), 'error');
  };
  const onRejection = (ev: PromiseRejectionEvent) => {
    void reportCrash(ev.reason, 'unhandled_rejection');
  };
  const onTick = (ev: Event) => {
    const d = (ev as CustomEvent<{ src?: unknown; input?: unknown }>).detail;
    if (d && (d.src === 'game' || d.src === 'emulator' || d.src === 'activity'))
      recordTick(d.src, d.input);
  };
  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);
  window.addEventListener(TICK_EVENT, onTick);
  return () => {
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
    window.removeEventListener(TICK_EVENT, onTick);
  };
}

/** Test seam: forget what was sent. */
export function resetCrashState(): void {
  sent = 0;
  sending = false;
  seen.clear();
  sender = defaultSender;
  routeName = () => 'unknown';
}
