// Non-Safari HLS for operator-listed hosts (PRF-UNF-001, D-518). A browser without native HLS plays a playlist through
// hls.js (the Cinema player's engine) only when the playlist's host is one the build lists: the PeerTube instances
// (`SR_PEERTUBE_HOSTS`) and the extra media hosts (`SR_MEDIA_HOSTS`), the same lists the app policy's `connect-src`
// names (D-517). hls.js and the engine load lazily, only when such a playlist is played, and every request it makes
// carries no credentials. Never call `attachHls` for an unlisted host; it refuses anyway.

/** The listed hosts, from the build (vite.config.ts `__SR_MEDIA_HOSTS__`). */
export function listedMediaHosts(): readonly string[] {
  return typeof __SR_MEDIA_HOSTS__ !== 'undefined' && Array.isArray(__SR_MEDIA_HOSTS__)
    ? __SR_MEDIA_HOSTS__
    : [];
}

/** Whether `url` is an https playlist address on a listed host (no credentials in it, the default port). */
export function canPlayListedHls(
  url: string | null | undefined,
  hosts: readonly string[] = listedMediaHosts(),
): boolean {
  if (!url) return false;
  try {
    const u = new URL(url);
    return (
      u.protocol === 'https:' &&
      !u.username &&
      !u.password &&
      u.port === '' &&
      hosts.includes(u.hostname.toLowerCase())
    );
  } catch {
    return false;
  }
}

/** Whether the browser plays HLS itself (Safari, iOS): then the element's own `src` is used and nothing is attached. */
export function nativeHls(video?: HTMLMediaElement | null): boolean {
  const el = video ?? (typeof document === 'undefined' ? null : document.createElement('video'));
  return !!el && el.canPlayType('application/vnd.apple.mpegurl') !== '';
}

/** hls.js cannot run here (no Media Source Extensions). */
export class HlsUnsupportedError extends Error {
  constructor() {
    super('hls.js is not supported in this browser');
    this.name = 'HlsUnsupportedError';
  }
}

/**
 * Play `url` in `video` through hls.js, credentials omitted. Resolves to a dispose function; rejects for an unlisted
 * host (nothing is loaded) or a browser without MSE ([HlsUnsupportedError]). `onError` hears fatal engine errors.
 */
export async function attachHls(
  video: HTMLVideoElement,
  url: string,
  opts: { hosts?: readonly string[]; onError?: () => void } = {},
): Promise<() => void> {
  if (!canPlayListedHls(url, opts.hosts ?? listedMediaHosts())) {
    throw new Error('not a listed media host');
  }
  const { default: Hls } = await import('hls.js');
  if (!Hls.isSupported()) throw new HlsUnsupportedError();
  const { createHlsEngine } = await import('../cinema/player/engine');
  const engine = await createHlsEngine(video, { omitCredentials: true });
  const off = engine.subscribe((e) => {
    if (e.type === 'error' && e.fatal) opts.onError?.();
  });
  engine.load(url);
  return () => {
    off();
    engine.destroy();
  };
}
