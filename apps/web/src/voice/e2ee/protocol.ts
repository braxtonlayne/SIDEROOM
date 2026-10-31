/**
 * Messages from the main thread to the E2EE media worker. Key material only ever travels
 * main thread -> worker (never back), and the worker never logs it.
 */
import type { MediaStats } from './media';
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
  /** ACT-CH-001: seal one activity data message / open one from `sender`; answered with `dataDone`. */
  | { type: 'dataSeal'; id: number; bytes: Uint8Array }
  | { type: 'dataOpen'; id: number; sender: string; bytes: Uint8Array }
  | { type: 'close' };

export type WorkerOut =
  | { type: 'ready' }
  /** The answer to `dataSeal` / `dataOpen`; `bytes` is `null` when the message must be dropped. */
  | { type: 'dataDone'; id: number; bytes: Uint8Array | null }
  | { type: 'error'; message: string }
  | {
      type: 'stats';
      sendEpoch: number;
      receiveEpoch: number;
      /** Frame counters so far (no key material). */
      frames?: MediaStats;
    };
