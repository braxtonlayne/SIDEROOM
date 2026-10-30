/**
 * UTL-IMG-001: frames from a `<video>` element for the GIF maker. The video is seeked frame by frame and drawn
 * onto a canvas, so it works in every browser that can play the file (WebCodecs would be faster, but seeking is
 * the path that behaves the same everywhere). The caps (15 s, 20 MiB) are enforced again in the Rust encoder.
 */
import { drawableToBitmap } from './canvasIo';

export const MAX_SECONDS = 15;

export interface FramePlan {
  /** Seek times in seconds. */
  times: number[];
  /** GIF frame delay in hundredths of a second. */
  delayCs: number;
}

/**
 * Which moments of the video become frames. The clip is `length` seconds from `start`, never past the end of
 * the video and never longer than 15 s; the rate is capped so the delay is at least 2 cs (50 fps).
 */
export function planFrames(duration: number, start: number, length: number, fps: number): FramePlan {
  const s = Math.max(0, Math.min(start, Math.max(0, duration - 0.1)));
  const len = Math.max(0.1, Math.min(length, MAX_SECONDS, duration - s));
  const delayCs = Math.max(2, Math.round(100 / Math.max(1, Math.min(50, fps))));
  const step = delayCs / 100;
  // The frames' delays add up to the clip length; keep that at most 15 s even after rounding the delay.
  const count = Math.max(1, Math.min(Math.floor(len / step), Math.floor(MAX_SECONDS / step)));
  const times = Array.from({ length: count }, (_, i) => s + i * step);
  return { times, delayCs };
}

export function seekTo(video: HTMLVideoElement, t: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const done = () => {
      video.removeEventListener('seeked', done);
      video.removeEventListener('error', fail);
      resolve();
    };
    const fail = () => {
      video.removeEventListener('seeked', done);
      video.removeEventListener('error', fail);
      reject(new Error('seek-failed'));
    };
    video.addEventListener('seeked', done);
    video.addEventListener('error', fail);
    video.currentTime = t;
  });
}

/** Read the frame at each time as RGBA at `width` x `height`, handing each to `onFrame`. */
export async function captureFrames(
  video: HTMLVideoElement,
  times: readonly number[],
  width: number,
  height: number,
  onFrame: (rgba: Uint8ClampedArray, index: number) => void,
  signal?: AbortSignal,
): Promise<void> {
  for (let i = 0; i < times.length; i++) {
    if (signal?.aborted) throw new Error('cancelled');
    await seekTo(video, times[i]!);
    const b = drawableToBitmap(video, width, height, Math.max(width, height));
    onFrame(b.data, i);
  }
}
