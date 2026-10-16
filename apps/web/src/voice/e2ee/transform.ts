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
  /** The codec's MIME type when frame metadata lacks it: `audio/opus`, or `video/*` to tell VP8 and H.264 apart by their bytes. */
  mime: string;
  /** recv: the sender's user id. */
  sender?: string;
}

/**
 * The SFU negotiates VP8 and H.264 for video. When the browser does not say which one a frame
 * is (older encoded-frame metadata has no `mimeType`), tell them apart by the bytes the
 * transform leaves in the clear: an H.264 access unit starts with an Annex B start code, a VP8
 * frame never does (its first byte is odd on interframes and carries show_frame on keyframes).
 */
export function sniffVideoMime(data: Uint8Array): string {
  const h264 =
    (data[0] === 0 && data[1] === 0 && data[2] === 1) ||
    (data[0] === 0 && data[1] === 0 && data[2] === 0 && data[3] === 1);
  return h264 ? 'video/H264' : 'video/VP8';
}

export function frameMime(frame: EncodedFrameLike, fallback: string, data?: Uint8Array): string {
  const m = frame.getMetadata?.().mimeType;
  if (m && m.length > 0) return m;
  if (fallback === 'video/*') return sniffVideoMime(data ?? new Uint8Array(frame.data));
  return fallback;
}

/** Transform one frame in place; `false` means drop it. */
export function transformFrame(
  media: E2eeMedia,
  opts: TransformOptions,
  frame: EncodedFrameLike,
  now: () => number = Date.now,
): boolean {
  const input = new Uint8Array(frame.data);
  const mime = frameMime(frame, opts.mime, input);
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
