import type { CrashKind } from './allowList';
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
  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);
  window.addEventListener(TICK_EVENT, onTick);
  return () => {
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
    window.removeEventListener(TICK_EVENT, onTick);
  };
}
