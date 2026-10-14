// The player session (CIN-PLY-001/007/010/011): ONE persistent <video> element, its engine, progress saving,
// signed-URL refresh and subtitle tracks, outside React. The full-screen watch route and the mini player only
// *attach* the element to a container, so playback keeps going while the viewer navigates (the same idea as the
// voice bridge). Watch parties reuse it: they drive `play/pause/seek/setRate` and subscribe to `store`.
import { useStore } from 'zustand';
import { createStore, type StoreApi } from 'zustand/vanilla';
import { startAssRenderer, type AssRenderer } from './ass';
import { setMiniPlayerActive } from './bridge';
import {
  DATA_SAVER_MAX_HEIGHT,
  createEngine,
  type EngineAudioTrack,
  type EngineEvent,
  type EngineFactory,
  type PlaybackEngine,
  type QualityLevel,
} from './engine';
import {
  DEFAULT_SUBTITLE_STYLE,
  HEARTBEAT_MS,
  clamp,
  formatClock,
  isWatched,
  refreshInMs,
  stepSpeed,
  type SubtitleStyle,
} from './model';
import { progressApi, type WatchProgressApi } from './progress';
import { refreshSource } from './api';
import { loadSigningMap, type UrlRewriter } from './signing';
import type { PlaybackInfo, RefreshedSource } from './types';

export interface PlayerState {
  status: 'idle' | 'loading' | 'ready' | 'error';
  mode: 'full' | 'mini';
  info: PlaybackInfo | null;
  engineKind: PlaybackEngine['kind'] | null;
  playing: boolean;
  waiting: boolean;
  ended: boolean;
  /** Playback was not started because the browser needs a user gesture first. */
  needsGesture: boolean;
  currentTime: number;
  duration: number;
  bufferedEnd: number;
  volume: number;
  muted: boolean;
  rate: number;
  levels: QualityLevel[];
  quality: 'auto' | number;
  playingLevel: number;
  dataSaver: boolean;
  audioTracks: EngineAudioTrack[];
  audioId: string | null;
  subtitleId: string | null;
  subtitleStyle: SubtitleStyle;
  /** The styled ASS renderer is drawing the current subtitle (otherwise native WebVTT). */
  assActive: boolean;
  autoplayNext: boolean;
  /** Episodes that started by themselves since the viewer last pressed something ("Are you still watching?"). */
  autoplayedCount: number;
  error: string | null;
  feedback: { id: number; text: string } | null;
}

interface Prefs {
  volume: number;
  muted: boolean;
  dataSaver: boolean;
  autoplayNext: boolean;
  subtitleStyle: SubtitleStyle;
  subtitleLang: string | null;
}
const PREFS_KEY = 'sr.cinema.player.v1';
const DEFAULT_PREFS: Prefs = {
  volume: 1,
  muted: false,
  dataSaver: false,
  autoplayNext: true,
  subtitleStyle: DEFAULT_SUBTITLE_STYLE,
  subtitleLang: null,
};
function loadPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<Prefs>;
      return {
        ...DEFAULT_PREFS,
        ...p,
        subtitleStyle: { ...DEFAULT_SUBTITLE_STYLE, ...(p.subtitleStyle ?? {}) },
      };
    }
  } catch {
    /* storage unavailable */
  }
  return DEFAULT_PREFS;
}
function savePrefs(p: Prefs) {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(p));
  } catch {
    /* best effort */
  }
}

export interface SessionDeps {
  engineFactory: EngineFactory;
  progress: () => WatchProgressApi;
  refresh: (mediaId: string) => Promise<RefreshedSource>;
  /** Builds the rewriter that re-signs requests from a fresh master playlist. */
  signing: (masterUrl: string) => Promise<UrlRewriter>;
}

export interface StartOptions {
  startAt?: number;
  /** Begin playing (when the browser allows it without a gesture). */
  autoplay?: boolean;
}

const FEEDBACK_MS = 900;

export class PlayerSession {
  readonly store: StoreApi<PlayerState>;
  private video: HTMLVideoElement | null = null;
  private park: HTMLElement | null = null;
  private engine: PlaybackEngine | null = null;
  private unsubEngine: (() => void) | null = null;
  private ass: AssRenderer | null = null;
  private trackEls: HTMLTrackElement[] = [];
  private token = 0;
  private heartbeat: ReturnType<typeof setInterval> | null = null;
  private refreshTimer: ReturnType<typeof setTimeout> | null = null;
  private feedbackTimer: ReturnType<typeof setTimeout> | null = null;
  private feedbackSeq = 0;
  private played = false;
  private lastSavedAt = -1;
  private lastCaptionId: string | null = null;
  private refreshing = false;
  private prefs: Prefs;
  private pageHooks = false;
  private autoNext = false;

  constructor(private readonly deps: SessionDeps) {
    this.prefs = loadPrefs();
    this.store = createStore<PlayerState>(() => ({
      status: 'idle',
      mode: 'full',
      info: null,
      engineKind: null,
      playing: false,
      waiting: false,
      ended: false,
      needsGesture: false,
      currentTime: 0,
      duration: 0,
      bufferedEnd: 0,
      volume: this.prefs.volume,
      muted: this.prefs.muted,
      rate: 1,
      levels: [],
      quality: 'auto',
      playingLevel: -1,
      dataSaver: this.prefs.dataSaver,
      audioTracks: [],
      audioId: null,
      subtitleId: null,
      subtitleStyle: this.prefs.subtitleStyle,
      assActive: false,
      autoplayNext: this.prefs.autoplayNext,
      autoplayedCount: 0,
      error: null,
      feedback: null,
    }));
  }

  get state() {
    return this.store.getState();
  }
  private set(p: Partial<PlayerState>) {
    this.store.setState(p);
  }

  /** The persistent element. Created on first use; parked in a hidden holder until something attaches it. */
  element(): HTMLVideoElement {
    if (this.video) return this.video;
    const v = document.createElement('video');
    v.className = 'cpl-video';
    v.playsInline = true;
    v.preload = 'auto';
    v.crossOrigin = 'anonymous';
    v.setAttribute('playsinline', '');
    v.disablePictureInPicture = false;
    this.video = v;
    this.park = document.createElement('div');
    this.park.setAttribute('data-cinema-player-park', '');
    this.park.style.cssText =
      'position:fixed;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);pointer-events:none;';
    document.body.appendChild(this.park);
    this.park.appendChild(v);
    this.bindVideo(v);
    this.bindPage();
    this.applyVolume();
    return v;
  }

  /** Moves the element into a container (the stage or the mini player). Re-attaching is a move, not a reload. */
  attach(container: HTMLElement) {
    const v = this.element();
    if (v.parentElement !== container) container.prepend(v);
  }
  /** Hands the element back to the hidden holder. Call in a layout-effect cleanup, before the container goes. */
  detach(container: HTMLElement) {
    if (this.video && this.video.parentElement === container && this.park)
      this.park.appendChild(this.video);
  }

  /* ----------------------------------------------------------- loading */

  matches(titleId: string, episodeId: string | null) {
    const i = this.state.info;
    return (
      !!i && this.state.status !== 'idle' && i.title_id === titleId && i.episode_id === episodeId
    );
  }

  async start(info: PlaybackInfo, opts: StartOptions = {}): Promise<void> {
    const video = this.element();
    setMiniPlayerActive(true);
    this.flush('close'); // the item being replaced keeps its position
    this.stopInternals(true);
    const auto = this.autoNext; // this item began by itself after the previous one ended
    this.autoNext = false;
    const token = ++this.token;
    this.played = false;
    this.lastSavedAt = -1;
    this.set({
      status: 'loading',
      info,
      engineKind: null,
      playing: false,
      waiting: true,
      ended: false,
      needsGesture: false,
      currentTime: opts.startAt ?? 0,
      duration: info.duration_s,
      bufferedEnd: 0,
      rate: 1,
      levels: [],
      quality: 'auto',
      playingLevel: -1,
      audioTracks: [],
      audioId: null,
      subtitleId: null,
      assActive: false,
      error: null,
      autoplayedCount: auto ? this.state.autoplayedCount + 1 : 0,
    });
    video.playbackRate = 1;
    this.applyVolume();
    try {
      const engine = await this.deps.engineFactory(video);
      if (token !== this.token) {
        engine.destroy();
        return;
      }
      this.engine = engine;
      this.unsubEngine = engine.subscribe((e) => this.onEngine(e));
      engine.setMaxHeight(this.state.dataSaver ? DATA_SAVER_MAX_HEIGHT : null);
      engine.load(info.master_url, opts.startAt);
      this.set({ status: 'ready', engineKind: engine.kind });
      this.scheduleRefresh(info.expires_at_ms, info.refresh_at_ms);
      this.pickDefaultSubtitle(info);
      if (opts.autoplay) void this.play();
      else this.set({ needsGesture: true });
    } catch {
      if (token === this.token)
        this.set({ status: 'error', error: 'We could not start playback.', waiting: false });
    }
  }

  private pickDefaultSubtitle(info: PlaybackInfo) {
    const lang = this.prefs.subtitleLang;
    const t =
      (lang && info.subtitle_tracks.find((s) => s.language === lang && !s.forced)) ||
      info.subtitle_tracks.find((s) => s.forced || s.is_default);
    if (t) void this.setSubtitle(t.id, { remember: false, announce: false });
  }

  /** Ends the current item (saving progress) and returns to idle. The mini player disappears. */
  close() {
    setMiniPlayerActive(false);
    this.flush('close');
    this.stopInternals(false);
    // A chain of auto-played episodes survives the hop to the next one; leaving for good resets it.
    this.set({
      status: 'idle',
      info: null,
      playing: false,
      autoplayedCount: this.autoNext ? this.state.autoplayedCount : 0,
      feedback: null,
    });
  }

  private stopInternals(keepStore: boolean) {
    this.token++;
    if (this.heartbeat) clearInterval(this.heartbeat);
    this.heartbeat = null;
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    this.refreshTimer = null;
    this.unsubEngine?.();
    this.unsubEngine = null;
    this.engine?.destroy();
    this.engine = null;
    this.ass?.dispose();
    this.ass = null;
    this.clearTracks();
    if (this.video) {
      this.video.pause();
      this.video.removeAttribute('src');
      this.video.load();
    }
    if (!keepStore) this.set({ status: 'idle' });
  }

  /* ---------------------------------------------------------- video events */

  private bindVideo(v: HTMLVideoElement) {
    const sync = () => {
      const end = v.buffered.length ? v.buffered.end(v.buffered.length - 1) : 0;
      this.set({
        currentTime: v.currentTime,
        duration:
          Number.isFinite(v.duration) && v.duration > 0
            ? v.duration
            : (this.state.info?.duration_s ?? 0),
        bufferedEnd: end,
      });
    };
    v.addEventListener('timeupdate', sync);
    v.addEventListener('durationchange', sync);
    v.addEventListener('progress', sync);
    v.addEventListener('seeked', sync);
    v.addEventListener('loadedmetadata', sync);
    v.addEventListener('play', () => {
      this.played = true;
      this.set({ playing: true, ended: false, needsGesture: false });
      this.startHeartbeat();
    });
    v.addEventListener('playing', () => this.set({ waiting: false, playing: true }));
    v.addEventListener('waiting', () => this.set({ waiting: true }));
    v.addEventListener('canplay', () => this.set({ waiting: false }));
    v.addEventListener('pause', () => {
      if (v.ended) return;
      this.set({ playing: false });
      this.stopHeartbeat();
      this.flush('pause');
    });
    v.addEventListener('ended', () => {
      this.set({ playing: false, ended: true, waiting: false });
      this.stopHeartbeat();
      this.flush('ended');
    });
    v.addEventListener('volumechange', () => {
      this.set({ volume: v.volume, muted: v.muted });
      this.prefs = { ...this.prefs, volume: v.volume, muted: v.muted };
      savePrefs(this.prefs);
    });
    v.addEventListener('ratechange', () => this.set({ rate: v.playbackRate }));
  }

  private bindPage() {
    if (this.pageHooks) return;
    this.pageHooks = true;
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') this.flush('hidden');
    });
    window.addEventListener('pagehide', () => this.flush('pagehide'));
  }

  private applyVolume() {
    if (!this.video) return;
    this.video.volume = clamp(this.prefs.volume, 0, 1);
    this.video.muted = this.prefs.muted;
  }

  /* ---------------------------------------------------------- progress */

  private startHeartbeat() {
    if (this.heartbeat) return;
    this.heartbeat = setInterval(() => this.flush('heartbeat'), HEARTBEAT_MS);
  }
  private stopHeartbeat() {
    if (this.heartbeat) clearInterval(this.heartbeat);
    this.heartbeat = null;
  }

  /** Saves the position (10 s heartbeat, pause, end, hidden tab, page hide, leaving). Never saves untouched items. */
  flush(reason: 'heartbeat' | 'pause' | 'ended' | 'hidden' | 'pagehide' | 'close' | 'seek') {
    const info = this.state.info;
    const v = this.video;
    if (!info || !v || !this.played || info.trailer || info.live) return;
    const duration = Number.isFinite(v.duration) && v.duration > 0 ? v.duration : info.duration_s;
    const position = v.ended ? duration : v.currentTime;
    if (reason !== 'ended' && Math.abs(position - this.lastSavedAt) < 1) return;
    this.lastSavedAt = position;
    const completed = v.ended || isWatched(position, duration);
    // Only the heartbeat says the picture is still moving; every other save ends the "watching" presence.
    const playing = reason === 'heartbeat' && !v.paused && !v.ended;
    void this.deps
      .progress()
      .save(
        {
          titleId: info.title_id,
          episodeId: info.episode_id,
          positionS: completed ? duration : position,
          durationS: duration,
          completed: v.ended || undefined,
          playing,
          title: info.title,
          episodeLabel: info.episode_label,
          posterUrl: info.poster_url,
        },
        { keepalive: reason === 'pagehide' || reason === 'hidden' },
      )
      .catch(() => undefined);
  }

  /* ------------------------------------------------- signed URL refresh */

  private scheduleRefresh(expiresAtMs: number, refreshAtMs?: number | null) {
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    this.refreshTimer = setTimeout(
      () => void this.refreshNow(false),
      refreshInMs(expiresAtMs, refreshAtMs, Date.now()),
    );
  }

  /** Fetches a fresh signed URL. `hard` also swaps the source (the old signature already stopped working). */
  async refreshNow(hard: boolean): Promise<void> {
    const info = this.state.info;
    const engine = this.engine;
    if (!info || !engine || this.refreshing) return;
    this.refreshing = true;
    const token = this.token;
    try {
      const next = await this.deps.refresh(info.media_id);
      if (token !== this.token) return;
      this.set({
        info: {
          ...info,
          master_url: next.master_url,
          expires_at_ms: next.expires_at_ms,
          refresh_at_ms: next.refresh_at_ms,
        },
      });
      // hls.js: every later request is re-signed from the fresh playlists, nothing restarts. Native HLS cannot
      // rewrite requests, so it swaps the source at the current position.
      if (hard || engine.kind === 'native') {
        const v = this.video!;
        const wasPlaying = !v.paused;
        engine.swapSource(next.master_url, v.currentTime);
        if (wasPlaying) void this.play();
      } else {
        const rewrite = await this.deps.signing(next.master_url);
        if (token !== this.token) return;
        engine.updateSigning(rewrite);
      }
      this.scheduleRefresh(next.expires_at_ms, next.refresh_at_ms);
    } catch {
      // Try again soon; playback continues on the current signature until it really lapses.
      if (token === this.token)
        this.refreshTimer = setTimeout(() => void this.refreshNow(false), 30_000);
    } finally {
      this.refreshing = false;
    }
  }

  private onEngine(e: EngineEvent) {
    switch (e.type) {
      case 'levels':
        this.set({ levels: e.levels });
        break;
      case 'level':
        this.set({ playingLevel: e.index });
        break;
      case 'audio':
        this.set({ audioTracks: e.tracks, audioId: e.active });
        break;
      case 'error':
        if (e.kind === 'auth') void this.refreshNow(true);
        else if (e.fatal && e.kind !== 'network')
          this.set({ status: 'error', error: 'Playback stopped because of a media error.' });
        break;
    }
  }

  /* ------------------------------------------------------------ controls */

  async play(): Promise<void> {
    const v = this.video;
    if (!v) return;
    try {
      await v.play();
      this.set({ needsGesture: false });
    } catch (e) {
      // NotAllowedError: the browser wants a gesture first; the big Play button covers it.
      if ((e as { name?: string } | null)?.name === 'NotAllowedError')
        this.set({ needsGesture: true });
    }
  }
  pause() {
    this.video?.pause();
  }
  toggle() {
    if (!this.video) return;
    if (this.video.paused || this.video.ended) {
      void this.play();
      this.say('Play');
    } else {
      this.pause();
      this.say('Pause');
    }
  }
  seek(t: number, announce = true) {
    const v = this.video;
    if (!v) return;
    const dur = this.state.duration || this.state.info?.duration_s || 0;
    const to = clamp(t, 0, dur > 0 ? dur : t);
    v.currentTime = to;
    this.set({ currentTime: to, ended: false });
    if (announce) this.say(formatClock(to));
  }
  seekBy(s: number) {
    const v = this.video;
    if (!v) return;
    this.seek(v.currentTime + s, false);
    this.say(`${s > 0 ? '+' : '−'}${Math.abs(s)} s`);
  }
  seekFraction(f: number) {
    const dur = this.state.duration || this.state.info?.duration_s || 0;
    if (dur > 0) this.seek(dur * f);
  }
  setVolume(vol: number, announce = true) {
    const v = this.video;
    if (!v) return;
    const c = clamp(vol, 0, 1);
    v.volume = c;
    if (c > 0 && v.muted) v.muted = false;
    if (announce) this.say(`Volume ${Math.round(c * 100)}%`);
  }
  volumeBy(d: number) {
    if (this.video) this.setVolume(Math.round((this.video.volume + d) * 100) / 100);
  }
  toggleMute() {
    const v = this.video;
    if (!v) return;
    v.muted = !v.muted;
    this.say(v.muted ? 'Muted' : 'Unmuted');
  }
  setRate(r: number) {
    if (!this.video) return;
    this.video.playbackRate = r;
    this.say(`${r}×`);
  }
  stepRate(dir: 1 | -1) {
    if (this.video) this.setRate(stepSpeed(this.video.playbackRate, dir));
  }

  setQuality(q: 'auto' | number) {
    this.engine?.setQuality(q);
    this.set({ quality: q });
    const lvl = typeof q === 'number' ? this.state.levels.find((l) => l.index === q) : null;
    this.say(lvl ? `${lvl.height}p` : 'Quality: auto');
  }
  setDataSaver(on: boolean) {
    this.engine?.setMaxHeight(on ? DATA_SAVER_MAX_HEIGHT : null);
    this.prefs = { ...this.prefs, dataSaver: on };
    savePrefs(this.prefs);
    this.set({ dataSaver: on });
    this.say(on ? 'Data saver on' : 'Data saver off');
  }
  setAudioTrack(id: string) {
    this.engine?.setAudioTrack(id);
    this.set({ audioId: id });
  }
  setAutoplayNext(on: boolean) {
    this.prefs = { ...this.prefs, autoplayNext: on };
    savePrefs(this.prefs);
    this.set({ autoplayNext: on });
  }
  /** A viewer action on the player counts as "still watching". */
  confirmStillWatching() {
    this.set({ autoplayedCount: 0 });
  }
  /** The next start() is an episode that begins by itself (counts toward "Are you still watching?"). */
  queueAutoNext(auto: boolean) {
    this.autoNext = auto;
  }
  setMode(mode: 'full' | 'mini') {
    this.set({ mode });
  }

  /* ----------------------------------------------------------- subtitles */

  private clearTracks() {
    for (const el of this.trackEls) el.remove();
    this.trackEls = [];
  }

  async setSubtitle(
    id: string | null,
    opts: { remember?: boolean; announce?: boolean } = {},
  ): Promise<void> {
    const { remember = true, announce = true } = opts;
    const info = this.state.info;
    const v = this.video;
    if (!info || !v) return;
    this.ass?.dispose();
    this.ass = null;
    this.clearTracks();
    this.set({ subtitleId: id, assActive: false });
    if (id === null) {
      if (announce) this.say('Subtitles off');
      if (remember) {
        this.prefs = { ...this.prefs, subtitleLang: null };
        savePrefs(this.prefs);
      }
      return;
    }
    const t = info.subtitle_tracks.find((s) => s.id === id);
    if (!t) return;
    this.lastCaptionId = id;
    if (remember) {
      this.prefs = { ...this.prefs, subtitleLang: t.language };
      savePrefs(this.prefs);
    }
    if (announce) this.say(`Subtitles: ${t.label}`);
    if (t.ass_url) {
      const r = await startAssRenderer(v, t.ass_url);
      if (this.state.subtitleId !== id) {
        r?.dispose();
        return;
      }
      if (r) {
        this.ass = r;
        this.set({ assActive: true });
        return;
      }
    }
    // WebVTT through a native text track (fallback for ASS too).
    const el = document.createElement('track');
    el.kind = 'subtitles';
    el.label = t.label;
    el.srclang = t.language;
    el.src = t.vtt_url;
    el.default = true;
    v.appendChild(el);
    this.trackEls.push(el);
    const show = () => {
      if (el.track) el.track.mode = 'showing';
    };
    show();
    el.addEventListener('load', show);
  }

  toggleCaptions() {
    const info = this.state.info;
    if (!info || info.subtitle_tracks.length === 0) {
      this.say('No subtitles');
      return;
    }
    if (this.state.subtitleId) void this.setSubtitle(null);
    else {
      const fallback = info.subtitle_tracks[0]!.id;
      void this.setSubtitle(this.lastCaptionId ?? fallback);
    }
  }

  setSubtitleStyle(patch: Partial<SubtitleStyle>) {
    const style = { ...this.state.subtitleStyle, ...patch };
    this.prefs = { ...this.prefs, subtitleStyle: style };
    savePrefs(this.prefs);
    this.set({ subtitleStyle: style });
  }

  /* ------------------------------------------------------------ feedback */

  say(text: string) {
    if (this.feedbackTimer) clearTimeout(this.feedbackTimer);
    this.set({ feedback: { id: ++this.feedbackSeq, text } });
    this.feedbackTimer = setTimeout(() => this.set({ feedback: null }), FEEDBACK_MS);
  }
}

/* ----------------------------------------------------------- singleton */

let session: PlayerSession | null = null;
const defaultDeps = (): SessionDeps => ({
  engineFactory: createEngine,
  progress: () => progressApi,
  refresh: refreshSource,
  signing: (masterUrl) => loadSigningMap(masterUrl),
});

/** The app-wide session (the mini player and the watch route share it). */
export function playerSession(): PlayerSession {
  session ??= new PlayerSession(defaultDeps());
  return session;
}

/** Tests: install a session with fake dependencies, or `null` to drop it. */
export function setPlayerSession(s: PlayerSession | null) {
  if (session && session !== s) session.close();
  session = s;
}

export function usePlayer<T>(selector: (s: PlayerState) => T): T {
  return useStore(playerSession().store, selector);
}
