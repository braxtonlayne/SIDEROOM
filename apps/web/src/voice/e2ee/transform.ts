/**
 * VOX-E2E-003: the per-track frame transform, shared by `RTCRtpScriptTransform` (the worker's
 * `rtctransform` event) and the `createEncodedStreams` fallback (streams posted to the worker).
 */
import type { E2eeMedia } from './media';

/** What both kinds of encoded frame (audio and video) look like here. */
export interface EncodedFrameLike {
  data: ArrayBuffer;
  getMetadata?(): { mimeType?: string };
}

/** Options the main thread attaches to a transform. */
export interface TransformOptions {
  direction: 'send' | 'recv';
  /** The codec's MIME type when frame metadata lacks it (`audio/opus`, `video/VP8`...). */
  mime: string;
  /** recv: the sender's user id. */
  sender?: string;
}

export function frameMime(frame: EncodedFrameLike, fallback: string): string {
  const m = frame.getMetadata?.().mimeType;
  return m && m.length > 0 ? m : fallback;
}

/** Transform one frame in place; `false` means drop it. */
export function transformFrame(
  media: E2eeMedia,
  opts: TransformOptions,
  frame: EncodedFrameLike,
  now: () => number = Date.now,
): boolean {
  const mime = frameMime(frame, opts.mime);
  const input = new Uint8Array(frame.data);
  const out =
    opts.direction === 'send'
      ? media.encrypt(mime, input)
      : opts.sender
        ? media.decrypt(opts.sender, mime, input, now())
        : null;
  if (!out) return false;
  const buf = new ArrayBuffer(out.byteLength);
  new Uint8Array(buf).set(out);
  frame.data = buf;
  return true;
}

export function frameTransformStream(
  media: E2eeMedia,
  opts: TransformOptions,
  now: () => number = Date.now,
): TransformStream<EncodedFrameLike, EncodedFrameLike> {
  return new TransformStream({
    transform(frame, controller) {
      if (transformFrame(media, opts, frame, now)) controller.enqueue(frame);
    },
  });
}

/** Pipe `readable` through the transform into `writable` (both sides of one track). */
export function runTransform(
  media: E2eeMedia,
  opts: TransformOptions,
  readable: ReadableStream<EncodedFrameLike>,
  writable: WritableStream<EncodedFrameLike>,
  now: () => number = Date.now,
): Promise<void> {
  return readable.pipeThrough(frameTransformStream(media, opts, now)).pipeTo(writable);
}
