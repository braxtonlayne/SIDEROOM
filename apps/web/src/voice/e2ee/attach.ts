/**
 * VOX-E2E-003: attach the E2EE worker to a sender or receiver. `RTCRtpScriptTransform` where
 * available; Chrome's `createEncodedStreams` otherwise (the peer connection must then be built
 * with `encodedInsertableStreams: true`, see `peerConfigFor`).
 */
import type { TransformMode } from './support';
import type { TransformOptions } from './transform';

export interface WorkerLike {
  postMessage(msg: unknown, transfer?: Transferable[]): void;
}

/** RTCRtpSender or RTCRtpReceiver, as far as attaching is concerned. */
export interface TransformTarget {
  transform?: unknown;
  createEncodedStreams?(): { readable: ReadableStream; writable: WritableStream };
}

type ScriptTransformCtor = new (worker: WorkerLike, options: TransformOptions) => unknown;

export function attachTransform(
  target: TransformTarget,
  worker: WorkerLike,
  options: TransformOptions,
  mode: TransformMode,
  ScriptTransform: ScriptTransformCtor | undefined = (
    globalThis as unknown as { RTCRtpScriptTransform?: ScriptTransformCtor }
  ).RTCRtpScriptTransform,
): boolean {
  if (mode === 'script-transform' && ScriptTransform) {
    target.transform = new ScriptTransform(worker, options);
    return true;
  }
  if (mode === 'encoded-streams' && target.createEncodedStreams) {
    const { readable, writable } = target.createEncodedStreams();
    worker.postMessage({ type: 'stream', options, readable, writable }, [
      readable as unknown as Transferable,
      writable as unknown as Transferable,
    ]);
    return true;
  }
  return false;
}

/** Extra RTCPeerConnection options the mode needs. */
export function peerConfigFor(mode: TransformMode): Record<string, unknown> {
  return mode === 'encoded-streams' ? { encodedInsertableStreams: true } : {};
}

/** Start the worker (Vite bundles it as a module worker). */
export function startE2eeWorker(): Worker {
  return new Worker(new URL('./worker.ts', import.meta.url), { type: 'module', name: 'sr-e2ee' });
}
