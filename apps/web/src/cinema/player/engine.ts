// HLS engine wrapper (CIN-PLY-001). The player talks to `PlaybackEngine` only: hls.js (MSE) on most browsers,
// the browser's own HLS on Safari/iOS, and a fake in tests. hls.js is imported lazily so it only ships with
// the watch route (it is the bulk of the player chunk).
import type { UrlRewriter } from './signing';

export interface QualityLevel {
  index: number;
  height: number;
  bitrate: number;
}
export interface EngineAudioTrack {
  id: string;
  label: string;
  language: string;
}

export type EngineEvent =
  | { type: 'levels'; levels: QualityLevel[] }
  | { type: 'level'; index: number }
  | { type: 'audio'; tracks: EngineAudioTrack[]; active: string | null }
  /** `auth`: a segment/playlist request was refused (expired signature), the session should refresh the URL. */
  | {
      type: 'error';
      fatal: boolean;
      kind: 'network' | 'media' | 'auth' | 'other';
      status?: number;
    };

export interface PlaybackEngine {
  readonly kind: 'hls.js' | 'native' | 'fake';
  load(url: string, startAt?: number): void;
  /** Continue with freshly signed files: every request made after this goes through `rewrite` first. */
  updateSigning(rewrite: UrlRewriter): void;
  /** Hard swap to a new URL at the given position (used when the signature already lapsed, and on Safari). */
  swapSource(url: string, startAt: number): void;
  /** 'auto' = ABR. A number is a level index chosen by the viewer. */
  setQuality(q: 'auto' | number): void;
  /** Data saver: ABR never picks above this height (null lifts the cap). */
  setMaxHeight(height: number | null): void;
  setAudioTrack(id: string): void;
  subscribe(fn: (e: EngineEvent) => void): () => void;
  destroy(): void;
}

export type EngineFactory = (video: HTMLVideoElement) => Promise<PlaybackEngine>;

export const DATA_SAVER_MAX_HEIGHT = 480;

/** Highest ladder rung at or below `maxHeight` (the lowest rung when all are above). -1 = no cap. */
export function capLevelIndex(levels: QualityLevel[], maxHeight: number | null): number {
  if (maxHeight === null || levels.length === 0) return -1;
  let best = -1;
  for (const l of levels)
    if (l.height <= maxHeight && (best === -1 || l.height >= levels[best]!.height)) best = l.index;
  if (best !== -1) return best;
  return levels.reduce((a, l) => (l.height < levels[a]!.height ? l.index : a), 0);
}

class Emitter {
  private fns = new Set<(e: EngineEvent) => void>();
  subscribe(fn: (e: EngineEvent) => void) {
    this.fns.add(fn);
    return () => void this.fns.delete(fn);
  }
  emit(e: EngineEvent) {
    for (const f of [...this.fns]) f(e);
  }
  clear() {
    this.fns.clear();
  }
}

/* ------------------------------------------------------------- native */

interface NativeAudioTrackList {
  length: number;
  [i: number]: { id: string; label: string; language: string; enabled: boolean };
  addEventListener?: (t: string, f: () => void) => void;
}

/** Safari / iOS: the video element plays the HLS URL itself. No quality control is available there. */
export function createNativeEngine(video: HTMLVideoElement): PlaybackEngine {
  const bus = new Emitter();
  const tracks = () => (video as unknown as { audioTracks?: NativeAudioTrackList }).audioTracks;
  const publishAudio = () => {
    const list = tracks();
    if (!list || list.length === 0) return;
    const out: EngineAudioTrack[] = [];
    let active: string | null = null;
    for (let i = 0; i < list.length; i++) {
      const t = list[i]!;
      out.push({
        id: String(i),
        label: t.label || t.language || `Track ${i + 1}`,
        language: t.language,
      });
      if (t.enabled) active = String(i);
    }
    bus.emit({ type: 'audio', tracks: out, active });
  };
  const onMeta = () => publishAudio();
  const onError = () => {
    const code = video.error?.code;
    // MEDIA_ERR_NETWORK (2) is what an expired signature looks like to the native stack.
    bus.emit({ type: 'error', fatal: true, kind: code === 2 ? 'auth' : 'media' });
  };
  video.addEventListener('loadedmetadata', onMeta);
  video.addEventListener('error', onError);
  return {
    kind: 'native',
    load(url, startAt) {
      video.src = url;
      if (startAt && startAt > 0) {
        video.addEventListener(
          'loadedmetadata',
          () => {
            video.currentTime = startAt;
          },
          { once: true },
        );
      }
    },
    updateSigning() {
      /* Native requests cannot be rewritten: the caller swaps the source at a quiet moment instead. */
    },
    swapSource(url, startAt) {
      const wasPlaying = !video.paused;
      video.src = url;
      video.addEventListener(
        'loadedmetadata',
        () => {
          video.currentTime = startAt;
          if (wasPlaying) void video.play().catch(() => undefined);
        },
        { once: true },
      );
    },
    setQuality() {},
    setMaxHeight() {},
    setAudioTrack(id) {
      const list = tracks();
      if (!list) return;
      for (let i = 0; i < list.length; i++) list[i]!.enabled = String(i) === id;
      publishAudio();
    },
    subscribe: (fn) => bus.subscribe(fn),
    destroy() {
      video.removeEventListener('loadedmetadata', onMeta);
      video.removeEventListener('error', onError);
      bus.clear();
      video.removeAttribute('src');
      video.load();
    },
  };
}

/* -------------------------------------------------------------- hls.js */

export async function createHlsEngine(video: HTMLVideoElement): Promise<PlaybackEngine> {
  const { default: Hls, Events, ErrorTypes } = await import('hls.js');
  const bus = new Emitter();
  let rewriter: UrlRewriter | null = null;
  let levels: QualityLevel[] = [];
  let maxHeight: number | null = null;
  let manual: 'auto' | number = 'auto';

  const rewrite = (url: string) => (rewriter ? rewriter(url) : url);
  const hls = new Hls({
    startFragPrefetch: true,
    // hls.js uses XHR by default; both hooks are set so the loader choice never matters.
    xhrSetup: (xhr, url) => {
      const to = rewrite(url);
      if (to !== url) xhr.open('GET', to, true);
    },
    fetchSetup: (context, init) => new Request(rewrite(context.url), init),
  });

  const applyCap = () => {
    // A viewer's explicit choice wins over the data saver; the cap only bounds ABR.
    hls.autoLevelCapping = manual === 'auto' ? capLevelIndex(levels, maxHeight) : -1;
  };

  hls.on(Events.MANIFEST_PARSED, (_e, data) => {
    levels = data.levels.map((l, index) => ({ index, height: l.height, bitrate: l.bitrate }));
    applyCap();
    bus.emit({ type: 'levels', levels });
  });
  hls.on(Events.LEVEL_SWITCHED, (_e, data) => bus.emit({ type: 'level', index: data.level }));
  const publishAudio = () =>
    bus.emit({
      type: 'audio',
      tracks: hls.audioTracks.map((t, i) => ({
        id: String(i),
        label: t.name || t.lang || `Track ${i + 1}`,
        language: t.lang ?? '',
      })),
      active: String(hls.audioTrack),
    });
  hls.on(Events.AUDIO_TRACKS_UPDATED, publishAudio);
  hls.on(Events.AUDIO_TRACK_SWITCHED, publishAudio);
  hls.on(Events.ERROR, (_e, data) => {
    const status = data.response?.code;
    if (
      data.type === ErrorTypes.NETWORK_ERROR &&
      (status === 401 || status === 403 || status === 410)
    ) {
      bus.emit({ type: 'error', fatal: data.fatal, kind: 'auth', status });
      return;
    }
    if (!data.fatal) return;
    if (data.type === ErrorTypes.MEDIA_ERROR) {
      hls.recoverMediaError();
      return;
    }
    if (data.type === ErrorTypes.NETWORK_ERROR) {
      bus.emit({ type: 'error', fatal: true, kind: 'network', status });
      hls.startLoad();
      return;
    }
    bus.emit({ type: 'error', fatal: true, kind: 'other' });
  });
  hls.attachMedia(video);

  return {
    kind: 'hls.js',
    load(url, startAt) {
      hls.config.startPosition = startAt && startAt > 0 ? startAt : -1;
      hls.loadSource(url);
    },
    updateSigning(fn) {
      rewriter = fn;
    },
    swapSource(url, startAt) {
      rewriter = null; // the new master's playlists carry their own fresh signatures
      hls.config.startPosition = startAt;
      hls.loadSource(url);
    },
    setQuality(q) {
      manual = q;
      if (q === 'auto') {
        hls.nextLevel = -1;
        hls.currentLevel = -1;
      } else {
        // `nextLevel` switches at the next fragment boundary: no flush, no stall.
        hls.nextLevel = q;
      }
      applyCap();
    },
    setMaxHeight(h) {
      maxHeight = h;
      applyCap();
    },
    setAudioTrack(id) {
      hls.audioTrack = Number(id);
    },
    subscribe: (fn) => bus.subscribe(fn),
    destroy() {
      bus.clear();
      hls.destroy();
    },
  };
}

/** Native HLS where the browser has it (Safari, iOS), hls.js everywhere else. */
export const createEngine: EngineFactory = async (video) => {
  if (video.canPlayType('application/vnd.apple.mpegurl') !== '') return createNativeEngine(video);
  return createHlsEngine(video);
};
