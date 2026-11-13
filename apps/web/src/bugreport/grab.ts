import type { CapturedFrame } from './openState';

const FRAME_TIMEOUT_MS = 4000;

/**
 * Plays the capture stream in a detached `<video>`, waits for its first decoded frame and draws that one frame to a
 * canvas. The caller (`captureTab`) stops every track afterwards.
 */
export async function grabFrame(stream: MediaStream): Promise<CapturedFrame> {
  const video = document.createElement('video');
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;
  try {
    await Promise.race([
      (async () => {
        await video.play();
        const v = video as HTMLVideoElement & {
          requestVideoFrameCallback?: (cb: () => void) => number;
        };
        if (typeof v.requestVideoFrameCallback === 'function') {
          await new Promise<void>((res) => v.requestVideoFrameCallback!(() => res()));
        } else if (video.videoWidth === 0) {
          await new Promise<void>((res) =>
            video.addEventListener('loadeddata', () => res(), { once: true }),
          );
        }
      })(),
      new Promise<never>((_, rej) =>
        setTimeout(() => rej(new Error('No frame from the capture')), FRAME_TIMEOUT_MS),
      ),
    ]);
    const w = video.videoWidth;
    const h = video.videoHeight;
    if (!(w > 0 && h > 0)) throw new Error('Empty frame');
    const canvas = document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('No 2d canvas');
    ctx.drawImage(video, 0, 0, w, h);
    return { source: canvas, w, h };
  } finally {
    video.pause();
    video.srcObject = null;
  }
}
