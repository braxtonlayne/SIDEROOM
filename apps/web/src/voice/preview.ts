/**
 * Stream preview thumbnails (VOX-CLI-008). While the viewer shares their screen, a small JPEG of it is uploaded now
 * and then (`POST /streams/{stream_key}/preview`, JSON `{ thumbnail: "data:image/jpeg;base64,..." }`); people who
 * have not joined the stream see it on the stream tile instead of an empty box. The server keeps only the newest
 * one in memory and accepts one upload per five seconds, at most 256 KiB.
 *
 * Capture and upload are injected (`PreviewDeps`), so tests run without a canvas or a network.
 */
import { apiFetch } from '../api/client';

/** First upload soon after the share starts, then at this pace (the server allows one per 5 s). */
export const PREVIEW_FIRST_MS = 3_000;
export const PREVIEW_INTERVAL_MS = 30_000;
/** Thumbnail width in pixels; the height follows the share's aspect ratio. */
export const PREVIEW_WIDTH = 480;
export const PREVIEW_QUALITY = 0.6;
/** The server's limit for a decoded thumbnail. */
export const PREVIEW_MAX_BYTES = 256 * 1024;

export interface PreviewDeps {
  /** A JPEG data URI of the stream's current frame, or null when no frame is available yet. */
  capture(stream: MediaStream): Promise<string | null>;
  upload(key: string, dataUri: string): Promise<void>;
}

/** Size in bytes of a base64 data URI's payload. */
export function dataUriBytes(uri: string): number {
  const comma = uri.indexOf(',');
  const b64 = comma < 0 ? '' : uri.slice(comma + 1);
  const pad = b64.endsWith('==') ? 2 : b64.endsWith('=') ? 1 : 0;
  return Math.max(0, Math.floor((b64.length * 3) / 4) - pad);
}

/** Draw the current frame of a stream onto a canvas and encode it as JPEG. */
export async function captureFrame(stream: MediaStream): Promise<string | null> {
  if (typeof document === 'undefined') return null;
  const video = document.createElement('video');
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;
  try {
    await video.play();
    if (!video.videoWidth) {
      await new Promise<void>((resolve) => {
        const t = setTimeout(resolve, 1_000);
        video.onloadeddata = () => {
          clearTimeout(t);
          resolve();
        };
      });
    }
    const w = video.videoWidth;
    const h = video.videoHeight;
    if (!w || !h) return null;
    const canvas = document.createElement('canvas');
    canvas.width = Math.min(PREVIEW_WIDTH, w);
    canvas.height = Math.round((canvas.width * h) / w);
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
    return canvas.toDataURL('image/jpeg', PREVIEW_QUALITY);
  } catch {
    return null;
  } finally {
    video.srcObject = null;
  }
}

export const browserPreviewDeps: PreviewDeps = {
  capture: captureFrame,
  upload: async (key, thumbnail) => {
    await apiFetch(`/streams/${encodeURIComponent(key)}/preview`, {
      method: 'POST',
      json: { thumbnail },
      retries: 0,
    });
  },
};

/** Uploads a thumbnail of one stream on a timer until stopped. */
export class PreviewUploader {
  private first: ReturnType<typeof setTimeout> | undefined;
  private every: ReturnType<typeof setInterval> | undefined;
  private busy = false;
  private gen = 0;

  constructor(private readonly deps: PreviewDeps = browserPreviewDeps) {}

  start(key: string, stream: MediaStream): void {
    this.stop();
    const gen = ++this.gen;
    const tick = () => void this.once(gen, key, stream);
    this.first = setTimeout(tick, PREVIEW_FIRST_MS);
    this.every = setInterval(tick, PREVIEW_INTERVAL_MS);
  }

  stop(): void {
    this.gen++;
    clearTimeout(this.first);
    clearInterval(this.every);
    this.first = this.every = undefined;
  }

  private async once(gen: number, key: string, stream: MediaStream): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      const uri = await this.deps.capture(stream);
      // Stopped (or restarted for another stream) while the frame was being drawn.
      if (gen !== this.gen || !uri || dataUriBytes(uri) > PREVIEW_MAX_BYTES) return;
      await this.deps.upload(key, uri);
    } catch {
      /* the next tick tries again; a missing preview is not an error worth showing */
    } finally {
      this.busy = false;
    }
  }
}
