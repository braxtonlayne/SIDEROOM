/**
 * Messages from the main thread to the E2EE media worker. Key material only ever travels
 * main thread -> worker (never back), and the worker never logs it.
 */
import type { EncodedFrameLike, TransformOptions } from './transform';

export type WorkerIn =
  | { type: 'prepareSend'; epoch: number; secret: Uint8Array }
  | { type: 'execute'; epoch: number }
  | { type: 'setReceive'; epoch: number; userIds: string[]; secrets: Uint8Array }
  /** The `createEncodedStreams` fallback: one track's streams, transferred. */
  | {
      type: 'stream';
      options: TransformOptions;
      readable: ReadableStream<EncodedFrameLike>;
      writable: WritableStream<EncodedFrameLike>;
    }
  | { type: 'close' };

export type WorkerOut =
  | { type: 'ready' }
  | { type: 'error'; message: string }
  | { type: 'stats'; sendEpoch: number; receiveEpoch: number };
