// Test doubles for the player: jsdom has no media pipeline, so HTMLMediaElement is replaced by a small state
// machine, and the HLS engine by a recorder.
import { vi } from 'vitest';
import type { EngineEvent, PlaybackEngine, QualityLevel } from '../engine';
import type { UrlRewriter } from '../signing';
import { PlayerSession, setPlayerSession } from '../session';
import type { WatchProgressApi, WatchProgressInput } from '../progress';
import type { PlaybackInfo } from '../types';

interface MediaState {
  paused: boolean;
  ended: boolean;
  time: number;
  duration: number;
  rate: number;
  volume: number;
  muted: boolean;
  buffered: number;
}
const states = new WeakMap<HTMLMediaElement, MediaState>();
function st(el: HTMLMediaElement): MediaState {
  let s = states.get(el);
  if (!s) {
    s = {
      paused: true,
      ended: false,
      time: 0,
      duration: NaN,
      rate: 1,
      volume: 1,
      muted: false,
      buffered: 0,
    };
    states.set(el, s);
  }
  return s;
}
const ev = (el: HTMLMediaElement, type: string) => el.dispatchEvent(new Event(type));

export function installMediaMocks() {
  const proto = HTMLMediaElement.prototype;
  const def = (name: string, desc: PropertyDescriptor) =>
    Object.defineProperty(proto, name, { configurable: true, ...desc });
  def('paused', {
    get(this: HTMLMediaElement) {
      return st(this).paused;
    },
  });
  def('ended', {
    get(this: HTMLMediaElement) {
      return st(this).ended;
    },
  });
  def('duration', {
    get(this: HTMLMediaElement) {
      return st(this).duration;
    },
  });
  def('currentTime', {
    get(this: HTMLMediaElement) {
      return st(this).time;
    },
    set(this: HTMLMediaElement, v: number) {
      st(this).time = v;
      st(this).ended = false;
      ev(this, 'seeked');
    },
  });
  def('playbackRate', {
    get(this: HTMLMediaElement) {
      return st(this).rate;
    },
    set(this: HTMLMediaElement, v: number) {
      st(this).rate = v;
      ev(this, 'ratechange');
    },
  });
  def('volume', {
    get(this: HTMLMediaElement) {
      return st(this).volume;
    },
    set(this: HTMLMediaElement, v: number) {
      st(this).volume = v;
      ev(this, 'volumechange');
    },
  });
  def('muted', {
    get(this: HTMLMediaElement) {
      return st(this).muted;
    },
    set(this: HTMLMediaElement, v: boolean) {
      st(this).muted = v;
      ev(this, 'volumechange');
    },
  });
  def('buffered', {
    get(this: HTMLMediaElement) {
      const end = st(this).buffered;
      return { length: end > 0 ? 1 : 0, start: () => 0, end: () => end };
    },
  });
  def('play', {
    value(this: HTMLMediaElement) {
      st(this).paused = false;
      st(this).ended = false;
      ev(this, 'play');
      ev(this, 'playing');
      return Promise.resolve();
    },
  });
  def('pause', {
    value(this: HTMLMediaElement) {
      if (st(this).paused) return;
      st(this).paused = true;
      ev(this, 'pause');
    },
  });
  def('load', { value: () => undefined });
}

/** Drive the fake element like a stream would. */
export const media = {
  setDuration(el: HTMLMediaElement, d: number) {
    st(el).duration = d;
    ev(el, 'durationchange');
  },
  tick(el: HTMLMediaElement, t: number) {
    st(el).time = t;
    ev(el, 'timeupdate');
  },
  buffer(el: HTMLMediaElement, end: number) {
    st(el).buffered = end;
    ev(el, 'progress');
  },
  end(el: HTMLMediaElement) {
    const s = st(el);
    s.ended = true;
    s.paused = true;
    s.time = Number.isFinite(s.duration) ? s.duration : s.time;
    ev(el, 'ended');
  },
  isPaused: (el: HTMLMediaElement) => st(el).paused,
  rate: (el: HTMLMediaElement) => st(el).rate,
  time: (el: HTMLMediaElement) => st(el).time,
};

export class FakeEngine implements PlaybackEngine {
  kind: PlaybackEngine['kind'] = 'fake';
  calls: [string, ...unknown[]][] = [];
  listeners = new Set<(e: EngineEvent) => void>();
  destroyed = false;
  load = vi.fn((url: string, startAt?: number) => void this.calls.push(['load', url, startAt]));
  updateSigning = vi.fn((fn: UrlRewriter) => void this.calls.push(['updateSigning', fn]));
  swapSource = vi.fn((url: string, at: number) => void this.calls.push(['swapSource', url, at]));
  setQuality = vi.fn((q: 'auto' | number) => void this.calls.push(['setQuality', q]));
  setMaxHeight = vi.fn((h: number | null) => void this.calls.push(['setMaxHeight', h]));
  setAudioTrack = vi.fn((id: string) => void this.calls.push(['setAudioTrack', id]));
  subscribe(fn: (e: EngineEvent) => void) {
    this.listeners.add(fn);
    return () => void this.listeners.delete(fn);
  }
  destroy() {
    this.destroyed = true;
  }
  emit(e: EngineEvent) {
    for (const f of [...this.listeners]) f(e);
  }
}

export const LEVELS: QualityLevel[] = [
  { index: 0, height: 480, bitrate: 1_500_000 },
  { index: 1, height: 720, bitrate: 3_000_000 },
  { index: 2, height: 1080, bitrate: 6_000_000 },
];

export function fakeProgress() {
  const saved: WatchProgressInput[] = [];
  const api: WatchProgressApi = {
    save: vi.fn((p: WatchProgressInput) => {
      saved.push(p);
      return Promise.resolve();
    }),
    get: vi.fn(() => Promise.resolve(null)),
  };
  return { api, saved };
}

export function makeInfo(o: Partial<PlaybackInfo> = {}): PlaybackInfo {
  return {
    title_id: 't1',
    episode_id: null,
    media_id: 'm1',
    title: 'Akira',
    episode_label: null,
    kind: 'movie',
    poster_url: null,
    age_restricted: false,
    duration_s: 6000,
    master_url: 'https://cdn.test/m1/master.m3u8?sig=a&exp=1',
    expires_at_ms: Date.now() + 6 * 3600_000,
    refresh_at_ms: null,
    subtitle_tracks: [
      { id: 's-en', language: 'en', label: 'English', vtt_url: 'https://cdn.test/m1/en.vtt' },
      { id: 's-ja', language: 'ja', label: 'Japanese', vtt_url: 'https://cdn.test/m1/ja.vtt' },
    ],
    markers: [],
    sprites_url: null,
    next: null,
    progress: null,
    ...o,
  };
}

export function makeSession(
  o: { refresh?: () => Promise<{ master_url: string; expires_at_ms: number }> } = {},
) {
  const engine = new FakeEngine();
  const progress = fakeProgress();
  const refresh = vi.fn(
    o.refresh ??
      (() =>
        Promise.resolve({
          master_url: 'https://cdn.test/m1/master.m3u8?sig=b&exp=2',
          expires_at_ms: Date.now() + 6 * 3600_000,
        })),
  );
  const signing = vi.fn((url: string): Promise<UrlRewriter> => {
    void url;
    return Promise.resolve((u: string) => u);
  });
  const session = new PlayerSession({
    engineFactory: () => Promise.resolve(engine),
    progress: () => progress.api,
    refresh,
    signing,
  });
  setPlayerSession(session);
  return { session, engine, progress, refresh, signing };
}

/** Call in afterEach: drops the session and its parked <video>. */
export function resetSession() {
  setPlayerSession(null);
  document.querySelectorAll('[data-cinema-player-park]').forEach((n) => n.remove());
  localStorage.clear();
}
