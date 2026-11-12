export type CaptureResult<F> =
  { ok: true; frame: F } | { ok: false; reason: 'unsupported' | 'declined' | 'failed' };

type DisplayMedia = Pick<MediaDevices, 'getDisplayMedia'>;

export function canCapture(md: Partial<DisplayMedia> | null | undefined): boolean {
  return !!md && typeof md.getDisplayMedia === 'function';
}

/**
 * One frame of the current tab through the browser's own capture. Call it directly inside the key handler: nothing is
 * awaited before `getDisplayMedia`, so the key press is still the user gesture. Every track is stopped in a `finally`.
 */
export async function captureTab<F>(
  md: Partial<DisplayMedia> | null | undefined,
  grabFrame: (stream: MediaStream) => Promise<F>,
): Promise<CaptureResult<F>> {
  if (!md || typeof md.getDisplayMedia !== 'function') return { ok: false, reason: 'unsupported' };
  let stream: MediaStream | null = null;
  try {
    stream = await md.getDisplayMedia({
      video: { displaySurface: 'browser' },
      preferCurrentTab: true,
      selfBrowserSurface: 'include',
      audio: false,
    } as DisplayMediaStreamOptions);
    return { ok: true, frame: await grabFrame(stream) };
  } catch (e) {
    const name = (e as { name?: unknown } | null)?.name;
    if (!stream && (name === 'NotAllowedError' || name === 'AbortError')) {
      return { ok: false, reason: 'declined' };
    }
    return { ok: false, reason: 'failed' };
  } finally {
    try {
      for (const t of stream?.getTracks() ?? []) t.stop();
    } catch {
      /* a track that cannot stop is already gone */
    }
  }
}
