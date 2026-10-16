/**
 * The worker's message handling, separate from the worker globals so it can be tested.
 */
import type { E2eeMedia } from './media';
import type { WorkerIn, WorkerOut } from './protocol';
import { runTransform, type EncodedFrameLike, type TransformOptions } from './transform';

export function handleWorkerMessage(
  media: E2eeMedia,
  msg: WorkerIn,
  post: (m: WorkerOut) => void,
  now: () => number = Date.now,
): void {
  switch (msg.type) {
    case 'prepareSend':
      media.prepareSend(msg.epoch, msg.secret);
      msg.secret.fill(0);
      break;
    case 'execute':
      media.execute(msg.epoch);
      break;
    case 'setReceive':
      media.setReceive(msg.epoch, msg.userIds, msg.secrets, now());
      msg.secrets.fill(0);
      break;
    case 'stream':
      void runTransform(media, msg.options, msg.readable, msg.writable, now).catch(() => {
        // The track ended or the peer connection closed.
      });
      return;
    case 'close':
      media.close();
      break;
  }
  post({ type: 'stats', sendEpoch: media.sendEpoch, receiveEpoch: media.receiveEpoch });
}

/** `rtctransform` event: `RTCRtpScriptTransform` hands the worker one track's streams. */
export interface RtcTransformerLike {
  readable: ReadableStream<EncodedFrameLike>;
  writable: WritableStream<EncodedFrameLike>;
  options?: TransformOptions;
}

export function handleRtcTransform(
  media: E2eeMedia,
  t: RtcTransformerLike,
  now: () => number = Date.now,
): void {
  const opts = t.options;
  if (!opts || (opts.direction !== 'send' && opts.direction !== 'recv')) {
    // Unknown wiring: pass nothing through rather than plaintext.
    void t.readable.cancel().catch(() => {});
    return;
  }
  void runTransform(media, opts, t.readable, t.writable, now).catch(() => {});
}
