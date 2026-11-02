import type { CrashKind } from './allowList';
import { CRASH_QUEUE_KEY } from './enums';
import { recordTick } from './ringBuffer';

/**
 * The small, eager half of the crash telemetry (OPS-CRASH-001): the page hooks and the ring buffer. Building and
 * sending a report is `./report`, which loads on the first crash, so the first-paint bundle only carries the hooks.
 */

/** The event an in-page game runtime dispatches to feed the ring without importing app code. */
export const TICK_EVENT = 'sr-crash-tick';

let routeName: () => string = () => 'unknown';

/** Report a failure (allow-listed, once per distinct crash, at most 5 per page load). Never throws. */
export async function reportCrash(error: unknown, kind: CrashKind): Promise<boolean> {
  try {
    const m = await import('./report');
    m.setRouteName(routeName);
    return await m.reportCrash(error, kind);
  } catch {
    return false;
  }
}

/**
 * Send the cards the offline queue holds (OPS-CRASH-002). Reads nothing but the existence of the queue key before it
 * loads the report chunk, so a browser with no queued crash never fetches that chunk for this. Never throws.
 */
export async function flushCrashQueue(): Promise<void> {
  try {
    if (!localStorage.getItem(CRASH_QUEUE_KEY)) return;
    const m = await import('./report');
    m.setRouteName(routeName);
    await m.flushCrashQueue();
  } catch {
    /* storage off or chunk unavailable: nothing to flush */
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

/**
 * Hook the page: unhandled errors, unhandled promise rejections and ticks from in-page runtimes. Call once, after
 * sign-in (the gated app bundle is the only caller, D-385). Returns the function that removes the hooks.
 */
export function installCrashTelemetry(options: { routeName: () => string }): () => void {
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
  const onOnline = () => void flushCrashQueue();
  const onVisible = () => {
    if (document.visibilityState === 'visible') void flushCrashQueue();
  };
  window.addEventListener('online', onOnline);
  document.addEventListener('visibilitychange', onVisible);
  // Cards queued by an earlier page load go out as soon as the app is up.
  void flushCrashQueue();
  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);
  window.addEventListener(TICK_EVENT, onTick);
  return () => {
    window.removeEventListener('online', onOnline);
    document.removeEventListener('visibilitychange', onVisible);
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
    window.removeEventListener(TICK_EVENT, onTick);
  };
}
