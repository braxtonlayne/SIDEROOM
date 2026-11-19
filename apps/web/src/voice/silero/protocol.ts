/** D-1050: the messages between the client (main thread) and the worker. */
export type ToWorker =
  | { type: 'init'; modelUrl: string; wasmUrl: string }
  | { type: 'frame'; frame: Float32Array }
  | { type: 'reset' };

export type FromWorker =
  | { type: 'ready' }
  | { type: 'error'; reason: 'model-fetch' | 'runtime-load' | 'session' }
  | { type: 'prob'; prob: number; ms: number; meanMs: number; maxMs: number }
  | { type: 'frameError' };
