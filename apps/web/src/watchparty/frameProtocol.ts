// The postMessage command protocols of the two providers Watch Together can drive (PRF-UNF-001). No provider
// script is ever loaded: the iframe is the provider's own embed page, and these are the plain messages its player
// documents for the host page. Pure functions only; `framePlayer.ts` runs them against a real iframe.
//
// Only the privacy-enhanced hosts are accepted (`youtube-nocookie.com`, Vimeo with `dnt=1`). The CSP `frame-src`
// of the app policy lists the same two origins and nothing else by default (`csp.ts`, pinned by tests).

export type FrameProvider = 'youtube' | 'vimeo';

/** The two origins a synced frame may come from. */
export const FRAME_ORIGINS: Record<FrameProvider, string> = {
  youtube: 'https://www.youtube-nocookie.com',
  vimeo: 'https://player.vimeo.com',
};

/** What a provider's frame reports, in one shape. */
export type FrameEvent =
  | { type: 'ready' }
  | { type: 'time'; seconds: number; duration?: number }
  | { type: 'state'; state: 'unstarted' | 'playing' | 'paused' | 'buffering' | 'resumed' | 'ended' }
  | { type: 'rate'; rate: number };

export type FrameCommand =
  | { cmd: 'play' }
  | { cmd: 'pause' }
  | { cmd: 'seek'; seconds: number }
  | { cmd: 'rate'; rate: number };

/** The provider of an embed address, or `null` when it is not one of the two privacy-enhanced embeds. */
export function providerOfSrc(src: string | null | undefined): FrameProvider | null {
  if (!src) return null;
  let u: URL;
  try {
    u = new URL(src);
  } catch {
    return null;
  }
  if (u.protocol !== 'https:' || u.username || u.password || u.port) return null;
  if (u.origin === FRAME_ORIGINS.youtube && /^\/embed\/[\w-]{11}$/.test(u.pathname))
    return 'youtube';
  if (u.origin === FRAME_ORIGINS.vimeo && /^\/video\/\d{1,12}$/.test(u.pathname)) return 'vimeo';
  return null;
}

/**
 * An embed address a card may show after the click: the two privacy-enhanced providers, or a PeerTube-style
 * `/videos/embed/<id>` page on an https host (the CSP only frames the instances the operator named).
 */
export function isCardEmbedSrc(src: string | null | undefined): boolean {
  if (providerOfSrc(src)) return true;
  if (!src) return false;
  try {
    const u = new URL(src);
    return (
      u.protocol === 'https:' &&
      !u.username &&
      !u.password &&
      !u.port &&
      /^\/videos\/embed\/[\w-]{8,40}$/.test(u.pathname) &&
      !u.search
    );
  } catch {
    return false;
  }
}

/** The address a card loads after the click: the provider's embed with autoplay. No script access. */
export function cardEmbedSrc(src: string): string {
  const u = new URL(src);
  u.searchParams.set('autoplay', '1');
  if (providerOfSrc(src) === 'youtube') {
    u.searchParams.set('rel', '0');
    u.searchParams.set('playsinline', '1');
  }
  return u.toString();
}

/**
 * The address a synced frame loads: the stored canonical embed plus the parameters that switch on the provider's
 * message API and hide its own controls (the party bar is the only control, so the two cannot disagree).
 */
export function syncedFrameSrc(provider: FrameProvider, src: string, origin: string): string {
  const u = new URL(src);
  u.searchParams.set('playsinline', '1');
  if (provider === 'youtube') {
    u.searchParams.set('enablejsapi', '1');
    u.searchParams.set('origin', origin);
    u.searchParams.set('controls', '0');
    u.searchParams.set('disablekb', '1');
    u.searchParams.set('fs', '0');
    u.searchParams.set('rel', '0');
    u.searchParams.set('modestbranding', '1');
    u.searchParams.set('iv_load_policy', '3');
  } else {
    u.searchParams.set('api', '1');
    u.searchParams.set('autopause', '0');
    u.searchParams.set('keyboard', '0');
    // Honoured on the paid plans only; the party bar still drives the player either way.
    u.searchParams.set('controls', '0');
  }
  return u.toString();
}

/** The message that makes the frame start reporting, sent after the frame loaded (YouTube) or said `ready` (Vimeo). */
export function listenMessages(provider: FrameProvider, phase: 'load' | 'ready'): string[] {
  if (provider === 'youtube')
    return phase === 'load'
      ? [JSON.stringify({ event: 'listening', id: 1, channel: 'widget' })]
      : [];
  if (phase === 'load') return [];
  return [
    'play',
    'pause',
    'ended',
    'timeupdate',
    'seeked',
    'bufferstart',
    'bufferend',
    'playbackratechange',
  ].map((value) => JSON.stringify({ method: 'addEventListener', value }));
}

/** The wire message for a command. */
export function commandMessage(provider: FrameProvider, c: FrameCommand): string {
  if (provider === 'youtube') {
    const call = (func: string, args: unknown[]) =>
      JSON.stringify({ event: 'command', func, args });
    switch (c.cmd) {
      case 'play':
        return call('playVideo', []);
      case 'pause':
        return call('pauseVideo', []);
      case 'seek':
        return call('seekTo', [c.seconds, true]);
      case 'rate':
        return call('setPlaybackRate', [c.rate]);
    }
  }
  switch (c.cmd) {
    case 'play':
      return JSON.stringify({ method: 'play' });
    case 'pause':
      return JSON.stringify({ method: 'pause' });
    case 'seek':
      return JSON.stringify({ method: 'setCurrentTime', value: c.seconds });
    case 'rate':
      return JSON.stringify({ method: 'setPlaybackRate', value: c.rate });
  }
}

/** YouTube offers these rates only; anything else snaps, which would defeat a small nudge. */
export const YOUTUBE_RATES = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];

const YT_STATE: Record<
  number,
  'unstarted' | 'playing' | 'paused' | 'buffering' | 'ended' | undefined
> = {
  [-1]: 'unstarted',
  0: 'ended',
  1: 'playing',
  2: 'paused',
  3: 'buffering',
  5: 'unstarted',
};

const asObject = (data: unknown): Record<string, unknown> | null => {
  let v = data;
  if (typeof v === 'string') {
    try {
      v = JSON.parse(v);
    } catch {
      return null;
    }
  }
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
};

const num = (v: unknown): number | undefined =>
  typeof v === 'number' && Number.isFinite(v) ? v : undefined;

/** The events one incoming message carries (usually zero or one). Unknown messages are ignored. */
export function parseMessage(provider: FrameProvider, data: unknown): FrameEvent[] {
  const m = asObject(data);
  if (!m) return [];
  const out: FrameEvent[] = [];
  if (provider === 'youtube') {
    const event = m.event;
    const info = m.info as Record<string, unknown> | number | undefined;
    if (event === 'onReady') out.push({ type: 'ready' });
    if (event === 'onStateChange' && typeof info === 'number') {
      const state = YT_STATE[info];
      if (state) out.push({ type: 'state', state });
    }
    if (
      (event === 'infoDelivery' || event === 'initialDelivery') &&
      info &&
      typeof info === 'object'
    ) {
      const t = num(info.currentTime);
      if (t !== undefined) {
        const duration = num(info.duration);
        out.push({ type: 'time', seconds: t, ...(duration !== undefined ? { duration } : {}) });
      }
      const ps = num(info.playerState);
      const state = ps === undefined ? undefined : YT_STATE[ps];
      if (state) out.push({ type: 'state', state });
      const rate = num(info.playbackRate);
      if (rate !== undefined) out.push({ type: 'rate', rate });
    }
    return out;
  }
  const data2 = (m.data ?? {}) as Record<string, unknown>;
  switch (m.event) {
    case 'ready':
      out.push({ type: 'ready' });
      break;
    case 'play':
      out.push({ type: 'state', state: 'playing' });
      break;
    case 'pause':
      out.push({ type: 'state', state: 'paused' });
      break;
    case 'ended':
      out.push({ type: 'state', state: 'ended' });
      break;
    case 'bufferstart':
      out.push({ type: 'state', state: 'buffering' });
      break;
    case 'bufferend':
      // The stall is over; the player is in whatever play/pause state it was in before.
      out.push({ type: 'state', state: 'resumed' });
      break;
    case 'playbackratechange': {
      const rate = num(data2.playbackRate);
      if (rate !== undefined) out.push({ type: 'rate', rate });
      break;
    }
    default:
      break;
  }
  if (
    ['play', 'pause', 'ended', 'timeupdate', 'seeked', 'bufferstart', 'bufferend'].includes(
      String(m.event),
    )
  ) {
    const seconds = num(data2.seconds);
    if (seconds !== undefined) {
      const duration = num(data2.duration);
      out.push({ type: 'time', seconds, ...(duration !== undefined ? { duration } : {}) });
    }
  }
  return out;
}
