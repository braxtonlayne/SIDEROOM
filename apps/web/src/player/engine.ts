/**
 * MUS-PLY-001: the engine binds the one persistent `<audio>` element and its Web Audio graph to the player
 * store. The store says what should happen; the engine makes the element do it and reports back.
 *
 * The PWA update flow (RDY-PWA-001) must not reload the page under a playing track: the engine installs an
 * update guard, and when the member accepted an update meanwhile, the reload runs at the next track boundary
 * (or when playback stops), after the queue is parked in sessionStorage so the reload resumes it paused.
 */
import { flushDeferredUpdate, isUpdateDeferred, setUpdateGuard } from '../pwa/state';
import {
  applyEq,
  applyVolume,
  buildGraph,
  defaultContextFactory,
  rampFade,
  type AudioGraph,
  type ContextFactory,
} from './graph';
import {
  installActions,
  updateMetadata,
  updatePlaybackState,
  updatePosition,
} from './mediaSession';
import {
  currentTrack,
  engineReport,
  next,
  nextIndex,
  usePlayerStore,
  type PlayerState,
} from './store';
import type { PlayerTrack } from './types';

export const RESUME_KEY = 'sr.player.resume';

interface Resume {
  queue: Pick<
    PlayerTrack,
    'id' | 'title' | 'artist' | 'album' | 'src' | 'artworkUrl' | 'durationSec'
  >[];
  index: number;
}

function saveResume(queue: PlayerTrack[], index: number): void {
  try {
    const slim: Resume['queue'] = queue.map(
      ({ id, title, artist, album, src, artworkUrl, durationSec }) => ({
        id,
        title,
        ...(artist !== undefined ? { artist } : {}),
        ...(album !== undefined ? { album } : {}),
        src,
        ...(artworkUrl !== undefined ? { artworkUrl } : {}),
        ...(durationSec !== undefined ? { durationSec } : {}),
      }),
    );
    sessionStorage.setItem(RESUME_KEY, JSON.stringify({ queue: slim, index } satisfies Resume));
  } catch {
    /* storage can be blocked; the queue is simply lost on reload */
  }
}

/** Put a parked queue back, paused (a browser does not let a fresh page start sound on its own). */
export function restoreResume(): boolean {
  try {
    const raw = sessionStorage.getItem(RESUME_KEY);
    if (!raw) return false;
    sessionStorage.removeItem(RESUME_KEY);
    const r = JSON.parse(raw) as Resume;
    if (!Array.isArray(r.queue) || r.queue.length === 0) return false;
    const queue = r.queue.filter((t) => typeof t?.id === 'string' && typeof t.src === 'string');
    if (queue.length === 0) return false;
    const index = Math.min(Math.max(0, Math.trunc(r.index) || 0), queue.length - 1);
    usePlayerStore.setState({ queue, index, status: 'paused', position: 0, error: null });
    return true;
  } catch {
    return false;
  }
}

export interface EngineOptions {
  contextFactory?: ContextFactory;
}

let active: PlayerEngine | null = null;
/** The live graph (for the VU bar), or null before the first track or without Web Audio. */
export const activeGraph = (): AudioGraph | null => active?.graph() ?? null;

export interface PlayerEngine {
  destroy(): void;
  /** The graph, once a track has started (null before, or without Web Audio). */
  graph(): AudioGraph | null;
}

export function createPlayerEngine(
  audio: HTMLAudioElement,
  opts: EngineOptions = {},
): PlayerEngine {
  const store = usePlayerStore;
  const makeContext = opts.contextFactory ?? defaultContextFactory;
  let graph: AudioGraph | null = null;
  let graphTried = false;
  let loadedKey: string | null = null;
  let loadToken = 0;
  let atBoundary = false;
  let fadingOut = false;
  let destroyed = false;

  const ensureGraph = () => {
    if (graph || graphTried) return graph;
    graphTried = true;
    try {
      const ctx = makeContext();
      if (ctx) {
        graph = buildGraph(ctx, audio);
        const s = store.getState();
        applyEq(graph, s.eq, s.eqEnabled);
        applyVolume(graph, s.volume, s.muted);
        audio.volume = 1;
      }
    } catch {
      graph = null; // the element alone still plays, only the EQ and the VU bar are gone
    }
    if (!graph) {
      const s = store.getState();
      audio.volume = s.muted ? 0 : s.volume;
    }
    return graph;
  };

  const syncVolume = (s: PlayerState) => {
    if (graph) applyVolume(graph, s.volume, s.muted);
    else audio.volume = s.muted ? 0 : s.volume;
  };

  const flushUpdate = (s: PlayerState, resumeIndex: number): boolean => {
    if (!isUpdateDeferred()) return false;
    saveResume(s.queue, resumeIndex);
    return flushDeferredUpdate();
  };

  const load = async () => {
    const s = store.getState();
    const track = currentTrack(s);
    if (!track) return;
    const token = ++loadToken;
    loadedKey = `${s.index}:${track.id}`;
    fadingOut = false;
    const g = ensureGraph();
    if (g) {
      void g.ctx.resume().catch(() => undefined);
      if (s.crossfadeSec > 0) {
        rampFade(g, 0, 0);
        rampFade(g, 1, Math.min(s.crossfadeSec, 12));
      } else rampFade(g, 1, 0);
    }
    let src = track.src;
    if (track.resolveSrc) {
      try {
        src = await track.resolveSrc(track);
      } catch {
        if (token === loadToken) engineReport.status('error', 'resolve');
        return;
      }
      if (token !== loadToken || destroyed) return;
    }
    audio.src = src;
    updateMetadata(track);
    if (store.getState().status === 'paused') {
      audio.load();
      return;
    }
    try {
      await audio.play();
    } catch (e) {
      if (token !== loadToken) return;
      const name = (e as { name?: string } | null)?.name;
      if (name === 'AbortError') return;
      // NotAllowedError: the browser wants a gesture. Stay paused so one press on Play starts it.
      engineReport.status(name === 'NotAllowedError' ? 'paused' : 'error', name ?? 'play');
    }
  };

  const onStore = (s: PlayerState, prev: PlayerState) => {
    if (destroyed) return;
    // Audio graph settings follow the store.
    if (s.volume !== prev.volume || s.muted !== prev.muted) syncVolume(s);
    if (graph && (s.eq !== prev.eq || s.eqEnabled !== prev.eqEnabled))
      applyEq(graph, s.eq, s.eqEnabled);
    if (s.seekRequest !== null && s.seekRequest !== prev.seekRequest) {
      try {
        audio.currentTime = s.seekRequest;
      } catch {
        /* not seekable yet */
      }
      engineReport.seekDone();
    }
    const track = currentTrack(s);
    if (track !== currentTrack(prev)) updateMetadata(track);
    if (s.status !== prev.status) updatePlaybackState(s.status);

    if (!track) {
      if (loadedKey !== null) {
        loadedKey = null;
        ++loadToken;
        audio.pause();
        audio.removeAttribute('src');
        audio.load();
      }
      return;
    }
    const key = `${s.index}:${track.id}`;
    const entering = s.status === 'loading' && prev.status !== 'loading';
    if (entering || (s.status === 'playing' && key !== loadedKey)) {
      void load();
      return;
    }
    if (s.status === 'playing' && audio.paused && loadedKey === key) {
      if (graph) void graph.ctx.resume().catch(() => undefined);
      void audio.play().catch(() => engineReport.status('paused'));
    } else if (s.status === 'paused' && !audio.paused) {
      audio.pause();
    }
    if (
      s.status !== prev.status &&
      (s.status === 'paused' || s.status === 'idle' || s.status === 'ended' || s.status === 'error')
    ) {
      flushUpdate(s, s.index);
    }
  };

  const onTime = () => {
    const s = store.getState();
    const d = Number.isFinite(audio.duration) ? audio.duration : 0;
    engineReport.time(audio.currentTime, d);
    updatePosition(audio.currentTime, d);
    // The fade-out before a boundary (the crossfade setting): only when something follows.
    if (
      graph &&
      s.crossfadeSec > 0 &&
      !fadingOut &&
      d > 0 &&
      nextIndex(s.index, s.queue.length, s.repeat, true) !== null &&
      d - audio.currentTime <= s.crossfadeSec
    ) {
      fadingOut = true;
      rampFade(graph, 0, Math.max(0.05, d - audio.currentTime));
    }
  };
  const onMeta = () => {
    if (Number.isFinite(audio.duration)) engineReport.time(audio.currentTime, audio.duration);
  };
  const onPlaying = () => {
    if (store.getState().status !== 'playing') engineReport.status('playing');
  };
  const onWaiting = () => {
    if (store.getState().status === 'playing') engineReport.status('loading');
  };
  const onEnded = () => {
    const s = store.getState();
    const to = nextIndex(s.index, s.queue.length, s.repeat, true);
    atBoundary = true;
    // A deferred app update reloads here, between tracks, with the rest of the queue parked.
    if (flushUpdate(s, to ?? 0)) {
      engineReport.status('paused');
      return;
    }
    atBoundary = false;
    next(true);
  };
  const onError = () => {
    engineReport.status('error', audio.error?.message || 'media');
  };

  audio.addEventListener('timeupdate', onTime);
  audio.addEventListener('loadedmetadata', onMeta);
  audio.addEventListener('durationchange', onMeta);
  audio.addEventListener('playing', onPlaying);
  audio.addEventListener('waiting', onWaiting);
  audio.addEventListener('ended', onEnded);
  audio.addEventListener('error', onError);
  const unsubscribe = store.subscribe(onStore);
  const removeActions = installActions();
  const removeGuard = setUpdateGuard(() => {
    const st = store.getState().status;
    return !atBoundary && (st === 'playing' || st === 'loading');
  });
  audio.preload = 'auto';
  // A queue parked by an update reload comes back paused; its first Play loads it.
  if (store.getState().queue.length > 0) updateMetadata(currentTrack());

  // The host mounts after the first play call (it loads on demand): pick up what is already queued.
  {
    const s = store.getState();
    if (currentTrack(s) && (s.status === 'loading' || s.status === 'playing')) void load();
  }

  const self: PlayerEngine = {
    graph: () => graph,
    destroy() {
      destroyed = true;
      if (active === self) active = null;
      unsubscribe();
      removeActions();
      removeGuard();
      audio.removeEventListener('timeupdate', onTime);
      audio.removeEventListener('loadedmetadata', onMeta);
      audio.removeEventListener('durationchange', onMeta);
      audio.removeEventListener('playing', onPlaying);
      audio.removeEventListener('waiting', onWaiting);
      audio.removeEventListener('ended', onEnded);
      audio.removeEventListener('error', onError);
      audio.pause();
    },
  };
  active = self;
  return self;
}
