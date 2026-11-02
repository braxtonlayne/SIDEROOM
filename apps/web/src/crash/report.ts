import { apiFetch } from '../api/client';
import { buildCrashCard, type CrashKind, type CrashReport } from './allowList';
import { detectCapabilities, detectEngine } from './capabilities';
import { parseStack } from './frames';
import {
  clearQueue,
  dequeueCrash,
  gpcOn,
  enqueueCrash,
  fingerprintOf,
  isRetryable,
  readQueue,
} from './queue';
import { ticks } from './ringBuffer';

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
/** Fingerprints this page load already delivered: a queued card for the same crash is dropped, never sent twice. */
const delivered = new Set<string>();

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
  // Global Privacy Control: nothing leaves the tab and nothing is kept (the local error page still shows).
  if (gpcOn()) return false;
  if (sending || sent >= MAX_REPORTS_PER_PAGE) return false;
  try {
    const report = reportFor(error, kind);
    if (!report) return false;
    const key = localKey(report);
    if (seen.has(key)) return false;
    seen.add(key);
    sending = true;
    sent += 1;
    try {
      await sender(report);
    } catch (e) {
      // Offline or a 5xx: keep the (already allow-listed) card for the next online or visible event.
      if (isRetryable(e)) enqueueCrash(report);
      return false;
    }
    const fp = fingerprintOf(report);
    delivered.add(fp);
    dequeueCrash(fp);
    return true;
  } catch {
    return false;
  } finally {
    sending = false;
  }
}

let flushing = false;

/**
 * Send the cards the offline queue holds (OPS-CRASH-002), oldest first, one at a time. The page's rate limit still
 * applies (a flush attempt counts like a report, at most `MAX_REPORTS_PER_PAGE` per page load); a card whose
 * fingerprint was already delivered is dropped; the first retryable failure stops the flush and keeps the rest.
 * Returns how many were delivered. Never throws.
 */
export async function flushCrashQueue(): Promise<number> {
  if (gpcOn()) {
    clearQueue();
    return 0;
  }
  if (flushing || sending) return 0;
  if (typeof navigator !== 'undefined' && navigator.onLine === false) return 0;
  flushing = true;
  let count = 0;
  try {
    for (const entry of readQueue()) {
      if (delivered.has(entry.fp)) {
        dequeueCrash(entry.fp);
        continue;
      }
      if (sent >= MAX_REPORTS_PER_PAGE) break;
      sending = true;
      sent += 1;
      try {
        await sender(entry.report);
        delivered.add(entry.fp);
        dequeueCrash(entry.fp);
        count += 1;
      } catch (e) {
        // A 4xx will never be accepted; anything else may be, later.
        if (!isRetryable(e)) dequeueCrash(entry.fp);
        else break;
      } finally {
        sending = false;
      }
    }
  } catch {
    /* never throws */
  } finally {
    flushing = false;
    sending = false;
  }
  return count;
}

/** Where the page is, as a route NAME (set by the installer). */
export function setRouteName(fn: RouteName): void {
  routeName = fn;
}

/** Test seam: forget what was sent. */
export function resetCrashState(): void {
  sent = 0;
  sending = false;
  seen.clear();
  delivered.clear();
  flushing = false;
  sender = defaultSender;
  routeName = () => 'unknown';
}
