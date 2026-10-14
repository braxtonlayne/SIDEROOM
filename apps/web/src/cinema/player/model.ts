// Pure player logic: no DOM, no network. Unit tested in model.test.ts.
import type { Marker, NextEpisode, PlaybackInfo, SpriteCue, WatchProgress } from './types';

export const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

/** 75 -> "1:15", 3725 -> "1:02:05". Negative and non-finite values read as 0:00. */
export function formatClock(seconds: number): string {
  const s = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`;
}

/** Spoken form for aria-valuetext: "1 hour 2 minutes 5 seconds". */
export function spokenClock(seconds: number): string {
  const s = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const part = (n: number, w: string) => (n ? `${n} ${w}${n === 1 ? '' : 's'}` : '');
  return (
    [part(h, 'hour'), part(m, 'minute'), part(sec, 'second')].filter(Boolean).join(' ') ||
    '0 seconds'
  );
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  let v = n;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/* ------------------------------------------------------------ progress */

export const WATCHED_FRACTION = 0.9;
export const HEARTBEAT_MS = 10_000;

export const isWatched = (position: number, duration: number) =>
  duration > 0 && position / duration >= WATCHED_FRACTION;

/**
 * Show the "Resume from 42:10 / Start over" prompt? Not for finished items, not for the first moments, not
 * when the link already carries an explicit start time, and not within the last seconds.
 */
export function resumeOffer(
  p: WatchProgress | null,
  startParam: number | undefined,
): number | null {
  if (startParam !== undefined || !p || p.completed) return null;
  if (p.position_s < 15) return null;
  if (
    p.duration_s > 0 &&
    (isWatched(p.position_s, p.duration_s) || p.duration_s - p.position_s < 20)
  )
    return null;
  return Math.floor(p.position_s);
}

/* ----------------------------------------------------------- markers */

export function activeMarker(markers: Marker[], t: number): Marker | null {
  return (
    markers.find(
      (m) => (m.kind === 'intro' || m.kind === 'credits') && t >= m.start_s && t < m.end_s - 1,
    ) ?? null
  );
}
export const skipLabel = (m: Marker) => (m.kind === 'intro' ? 'Skip intro' : 'Skip credits');

export const NEXT_CARD_FALLBACK_S = 20;
export const NEXT_COUNTDOWN_S = 10;
export const STILL_WATCHING_AFTER = 3;

/** When the next-episode card appears: the credits marker, else the last 20 seconds. Null without a next episode. */
export function nextCardStart(
  info: Pick<PlaybackInfo, 'markers' | 'duration_s' | 'next'>,
): number | null {
  if (!info.next) return null;
  const credits = info.markers.find((m) => m.kind === 'credits');
  if (credits) return credits.start_s;
  return info.duration_s > NEXT_CARD_FALLBACK_S ? info.duration_s - NEXT_CARD_FALLBACK_S : null;
}

export const nextLine = (n: NextEpisode) => `S${n.season} · E${n.number} · ${n.title}`;

/* ------------------------------------------------------------ sprites */

/** Parses a WebVTT sprite map: cue payload `sheet.jpg#xywh=160,90,160,90`. Relative URLs resolve against `base`. */
export function parseSpriteVtt(text: string, base: string): SpriteCue[] {
  const cues: SpriteCue[] = [];
  const lines = text.replace(/\r/g, '').split('\n');
  const ts = (s: string) => {
    const parts = s.split(':').map(Number);
    if (parts.some((p) => !Number.isFinite(p))) return NaN;
    return parts.reduce((acc, p) => acc * 60 + p, 0);
  };
  const origin = 'http://sprite.invalid';
  const baseUrl = new URL(base, origin);
  for (let i = 0; i < lines.length; i++) {
    const m = /^\s*([\d:.]+)\s+-->\s+([\d:.]+)/.exec(lines[i] ?? '');
    if (!m) continue;
    const payload = (lines[i + 1] ?? '').trim();
    const hash = /^(.*)#xywh=(\d+),(\d+),(\d+),(\d+)$/.exec(payload);
    const start = ts(m[1]!);
    const end = ts(m[2]!);
    if (!hash || Number.isNaN(start) || Number.isNaN(end)) continue;
    const abs = new URL(hash[1]!, baseUrl);
    const url = abs.origin === origin ? `${abs.pathname}${abs.search}` : abs.toString();
    cues.push({ start, end, url, x: +hash[2]!, y: +hash[3]!, w: +hash[4]!, h: +hash[5]! });
  }
  return cues;
}

export function spriteAt(cues: SpriteCue[], t: number): SpriteCue | null {
  if (!cues.length) return null;
  let lo = 0;
  let hi = cues.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (cues[mid]!.start <= t) lo = mid;
    else hi = mid - 1;
  }
  return cues[lo]!;
}

/* ---------------------------------------------------------- shortcuts */

export const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 1.75, 2] as const;

export function stepSpeed(current: number, dir: 1 | -1): number {
  const i = SPEEDS.findIndex((s) => s >= current - 1e-6);
  const base = i === -1 ? SPEEDS.length - 1 : i;
  return SPEEDS[clamp(base + dir, 0, SPEEDS.length - 1)]!;
}

export type PlayerAction =
  | { type: 'toggle' }
  | { type: 'seekBy'; seconds: number }
  | { type: 'seekFraction'; fraction: number }
  | { type: 'volumeBy'; delta: number }
  | { type: 'mute' }
  | { type: 'fullscreen' }
  | { type: 'captions' }
  | { type: 'speed'; dir: 1 | -1 };

export interface KeyLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

/** CIN-PLY-003: space/k, j/l, arrows, f, m, c, 0-9, < and >. Modified keys belong to the browser. */
export function actionForKey(e: KeyLike): PlayerAction | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  switch (e.key) {
    case ' ':
    case 'k':
    case 'K':
      return { type: 'toggle' };
    case 'j':
    case 'J':
      return { type: 'seekBy', seconds: -10 };
    case 'l':
    case 'L':
      return { type: 'seekBy', seconds: 10 };
    case 'ArrowLeft':
      return { type: 'seekBy', seconds: -5 };
    case 'ArrowRight':
      return { type: 'seekBy', seconds: 5 };
    case 'ArrowUp':
      return { type: 'volumeBy', delta: 0.05 };
    case 'ArrowDown':
      return { type: 'volumeBy', delta: -0.05 };
    case 'f':
    case 'F':
      return { type: 'fullscreen' };
    case 'm':
    case 'M':
      return { type: 'mute' };
    case 'c':
    case 'C':
      return { type: 'captions' };
    case '<':
      return { type: 'speed', dir: -1 };
    case '>':
      return { type: 'speed', dir: 1 };
    default:
      if (/^[0-9]$/.test(e.key)) return { type: 'seekFraction', fraction: Number(e.key) / 10 };
      return null;
  }
}

/* ----------------------------------------------------------- subtitles */

export type SubSize = 'small' | 'medium' | 'large' | 'xlarge';
export type SubColor = 'white' | 'yellow' | 'cyan' | 'green';
export type SubBackground = 'none' | 'translucent' | 'solid';
export interface SubtitleStyle {
  size: SubSize;
  color: SubColor;
  background: SubBackground;
}
export const DEFAULT_SUBTITLE_STYLE: SubtitleStyle = {
  size: 'medium',
  color: 'white',
  background: 'translucent',
};

const SIZE_EM: Record<SubSize, string> = {
  small: '0.8em',
  medium: '1em',
  large: '1.35em',
  xlarge: '1.8em',
};
const COLOR: Record<SubColor, string> = {
  white: '#ffffff',
  yellow: '#ffe14d',
  cyan: '#7be9ff',
  green: '#8dff9a',
};
const BG: Record<SubBackground, string> = {
  none: 'transparent',
  translucent: 'rgba(0, 0, 0, 0.6)',
  solid: '#000000',
};
/** CSS custom properties read by the `video::cue` rule in player.css. */
export function cueVars(s: SubtitleStyle): Record<string, string> {
  return {
    '--sr-cue-size': SIZE_EM[s.size],
    '--sr-cue-color': COLOR[s.color],
    '--sr-cue-bg': BG[s.background],
  };
}

/* ----------------------------------------------------- signed URL refresh */

export const REFRESH_LEAD_MS = 5 * 60_000;
/** Refresh when five minutes remain, or at half-life for short-lived URLs; never sooner than 5 s. */
export function refreshDelayMs(expiresAtMs: number, nowMs: number): number {
  const remaining = expiresAtMs - nowMs;
  const lead = remaining > 2 * REFRESH_LEAD_MS ? REFRESH_LEAD_MS : remaining / 2;
  return Math.max(5_000, remaining - lead);
}
/** The server's own `refresh_at` when it sent one, else `refreshDelayMs`. */
export function refreshInMs(
  expiresAtMs: number,
  refreshAtMs: number | null | undefined,
  nowMs: number,
) {
  return refreshAtMs ? Math.max(5_000, refreshAtMs - nowMs) : refreshDelayMs(expiresAtMs, nowMs);
}

/* ------------------------------------------------------------- route */

export interface WatchSearch {
  /** Episode id (`?episode=`, the name the title page uses; `?ep=` is accepted too). */
  episode?: string;
  variant?: string;
  /** Play the title's trailer. */
  trailer?: boolean;
  /** Start position in seconds. */
  t?: number;
}
const ID = /^[A-Za-z0-9_-]{1,64}$/;
const idOf = (v: unknown) => {
  if (typeof v !== 'string' && typeof v !== 'number') return undefined;
  const id = String(v);
  return ID.test(id) ? id : undefined;
};
/** The router turns numeric strings into numbers, so ids are stringified back. */
export function parseWatchSearch(s: Record<string, unknown>): WatchSearch {
  const out: WatchSearch = {};
  const episode = idOf(s.episode ?? s.ep);
  if (episode) out.episode = episode;
  const variant = idOf(s.variant);
  if (variant) out.variant = variant;
  if (s.trailer === true || s.trailer === 1 || s.trailer === '1' || s.trailer === 'true')
    out.trailer = true;
  const t = typeof s.t === 'number' ? s.t : typeof s.t === 'string' ? Number(s.t) : NaN;
  if (Number.isFinite(t) && t >= 0) out.t = Math.floor(t);
  return out;
}

export const watchHref = (titleId: string, episode?: string | null, t?: number) => {
  const q = new URLSearchParams();
  if (episode) q.set('episode', episode);
  if (t !== undefined) q.set('t', String(Math.floor(t)));
  const s = q.toString();
  return `/cinema/watch/${encodeURIComponent(titleId)}${s ? `?${s}` : ''}`;
};
