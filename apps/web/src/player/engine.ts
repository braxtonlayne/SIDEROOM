/**
 * MUS-PLY-001: the engine binds the persistent `<audio>` elements and their Web Audio graph to the player
 * store. The store says what should happen; the engine makes the elements do it and reports back.
 *
 * Wave-30 integration (MUS-LIB-001): with Web Audio and a second element the engine runs music-lib's
 * `GaplessDecks` (`music/queue/gapless.ts`). The next track is resolved and prefetched into the idle deck while
 * the current one plays, and the hand-over is scheduled on the audio clock: a sample-accurate cut, or the
 * 0 to 12 s equal-power crossfade of the store's `crossfadeSec`. Without Web Audio (or without a second
 * element) one element plays and a track change is a plain cut on `ended`.
 *
 * The PWA update flow (RDY-PWA-001) must not reload the page under a playing track: the engine installs an
 * update guard, and when the member accepted an update meanwhile, the reload runs at the next track boundary
 * (or when playback stops), after the queue is parked in sessionStorage so the reload resumes it paused.
 */
import { GaplessDecks, type GaplessTrack } from '../music/queue/gapless';
import { flushDeferredUpdate, isUpdateDeferred, setUpdateGuard } from '../pwa/state';
import {
  applyEq,
  applyVolume,
  buildGraph,
  defaultContextFactory,
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
  RESUME_KEY,
  usePlayerStore,
  type PlayerState,
} from './store';
import type { PlayerTrack } from './types';

export { RESUME_KEY };

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
  /** The second deck: with Web Audio it makes gapless playback and the crossfade possible. */
  nextAudio?: HTMLAudioElement;
  /** Timers of the gapless engine (tests pass fakes). */
  timers?: {
    setInterval(fn: () => void, ms: number): unknown;
    clearInterval(handle: unknown): void;
  };
}

let active: PlayerEngine | null = null;
/** The live graph (for the VU bar), or null before the first track or without Web Audio. */
export const activeGraph = (): AudioGraph | null => active?.graph() ?? null;

export interface PlayerEngine {
  destroy(): void;
  /** The graph, once a track has started (null before, or without Web Audio). */
  graph(): AudioGraph | null;
  /** The element that plays the current track (it alternates between the decks in gapless mode). */
  activeAudio(): HTMLAudioElement;
  /** Whether the two-deck gapless engine is running (Web Audio and a second element). */
  gapless(): boolean;
}

/** Web Audio hands back silence for a cross-origin file loaded without CORS, so a bucket URL asks for CORS. */
function setCors(el: HTMLAudioElement, src: string): void {
  try {
    const cross = new URL(src, window.location.href).origin !== window.location.origin;
    el.crossOrigin = cross ? 'anonymous' : null;
  } catch {
    el.crossOrigin = null;
  }
}

const keyOf = (index: number, track: PlayerTrack) => `${index}:${track.id}`;

export function createPlayerEngine(
  audio: HTMLAudioElement,
  opts: EngineOptions = {},
): PlayerEngine {
  const store = usePlayerStore;
  const makeContext = opts.contextFactory ?? defaultContextFactory;
  const elements: HTMLAudioElement[] = opts.nextAudio ? [audio, opts.nextAudio] : [audio];
  let graph: AudioGraph | null = null;
  let graphTried = false;
  let decks: GaplessDecks | null = null;
  let loadedKey: string | null = null;
  let loadToken = 0;
  /** The key the decks handed over to; the store's matching `loading` must not reload it. */
  let handedOver: string | null = null;
  /** The queue entry the decks hold as their current track (set once `start` ran, or at a hand-over). */
  let deckKey: string | null = null;
  /** What was prefetched into the idle deck, and the request in flight for it. */
  let nextKey: string | null = null;
  let nextToken = 0;
  let atBoundary = false;
  let destroyed = false;

  const playing = (): HTMLAudioElement => (decks ? (elements[decks.activeDeck] ?? audio) : audio);

  const ensureGraph = () => {
    if (graph || graphTried) return graph;
    graphTried = true;
    try {
      const ctx = makeContext();
      if (ctx) {
        graph = buildGraph(ctx, elements);
        const s = store.getState();
        applyEq(graph, s.eq, s.eqEnabled);
        applyVolume(graph, s.volume, s.muted);
        for (const el of elements) el.volume = 1;
        const [a, b] = elements;
        const [ga, gb] = graph.deckGains;
        if (a && b && ga && gb) {
          decks = new GaplessDecks({
            clock: ctx,
            decks: [
              { audio: a, gain: ga },
              { audio: b, gain: gb },
            ],
            crossfadeSec: () => store.getState().crossfadeSec,
            onAdvance,
            onEnded: onDecksEnded,
            ...(opts.timers ? { timers: opts.timers } : {}),
          });
        }
      }
    } catch {
      graph = null; // the element alone still plays, only the EQ and the VU bar are gone
      decks = null;
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

  const resolve = async (track: PlayerTrack): Promise<string> =>
    track.resolveSrc ? track.resolveSrc(track) : track.src;

  const asGapless = (track: PlayerTrack, src: string): GaplessTrack => ({
    id: track.id,
    src,
    ...(track.durationSec ? { durationSec: track.durationSec } : {}),
  });

  /** Name what follows the current track to the decks, resolving its URL first. */
  const refreshNext = () => {
    if (!decks || destroyed || decks.transitioning) return;
    const s = store.getState();
    const cur = currentTrack(s);
    if (!cur || deckKey !== keyOf(s.index, cur)) return;
    const to = nextIndex(s.index, s.queue.length, s.repeat, true);
    const track = to === null ? undefined : s.queue[to];
    // A deferred app update reloads at this boundary, so nothing is lined up behind it.
    const key = track && to !== null && !isUpdateDeferred() ? keyOf(to, track) : null;
    if (key === nextKey) return;
    nextKey = key;
    const token = ++nextToken;
    if (!track || key === null) {
      decks.setNext(null);
      return;
    }
    void resolve(track).then(
      (src) => {
        if (token !== nextToken || destroyed || !decks || decks.transitioning) return;
        const idle = elements[decks.activeDeck === 0 ? 1 : 0];
        if (idle) setCors(idle, src);
        decks.setNext(asGapless(track, src));
      },
      () => {
        // The next URL could not be had now: the track change falls back to `ended`, which loads it.
        if (token === nextToken) nextKey = null;
      },
    );
  };

  const load = async () => {
    const s = store.getState();
    const track = currentTrack(s);
    if (!track) return;
    const token = ++loadToken;
    loadedKey = keyOf(s.index, track);
    handedOver = null;
    deckKey = null;
    nextKey = null;
    ++nextToken;
    const g = ensureGraph();
    if (g) void g.ctx.resume().catch(() => undefined);
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
    updateMetadata(track);
    const paused = store.getState().status === 'paused';
    try {
      if (decks) {
        // `start` plays on the active deck; the bucket must allow GET with Range from the app origin.
        setCors(playing(), src);
        const started = decks.start(asGapless(track, src), 0, { paused });
        deckKey = loadedKey;
        refreshNext();
        await started;
        return;
      }
      setCors(audio, src);
      audio.src = src;
      if (paused) {
        audio.load();
        return;
      }
      await audio.play();
    } catch (e) {
      if (token !== loadToken) return;
      const name = (e as { name?: string } | null)?.name;
      if (name === 'AbortError') return;
      // NotAllowedError: the browser wants a gesture. Stay paused so one press on Play starts it.
      engineReport.status(name === 'NotAllowedError' ? 'paused' : 'error', name ?? 'play');
    }
  };

  /** The decks switched to the prefetched track: move the store along without reloading anything. */
  function onAdvance(track: GaplessTrack) {
    const s = store.getState();
    const to = nextIndex(s.index, s.queue.length, s.repeat, true);
    const t = to === null ? undefined : s.queue[to];
    nextKey = null;
    if (to === null || !t || t.id !== track.id) {
      // The queue changed under the hand-over: play what the store says instead.
      next(true);
      return;
    }
    // A deferred app update reloads here, between tracks, with the rest of the queue parked.
    if (isUpdateDeferred()) {
      atBoundary = true;
      decks?.pause();
      if (flushUpdate(s, to)) {
        engineReport.status('paused');
        return;
      }
      atBoundary = false;
      void decks?.resume();
    }
    handedOver = keyOf(to, t);
    next(true);
  }

  function onDecksEnded() {
    endedAtBoundary();
  }

  const endedAtBoundary = () => {
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

  const onStore = (s: PlayerState, prev: PlayerState) => {
    if (destroyed) return;
    // Audio graph settings follow the store.
    if (s.volume !== prev.volume || s.muted !== prev.muted) syncVolume(s);
    if (graph && (s.eq !== prev.eq || s.eqEnabled !== prev.eqEnabled))
      applyEq(graph, s.eq, s.eqEnabled);
    if (s.seekRequest !== null && s.seekRequest !== prev.seekRequest) {
      try {
        if (decks) decks.seek(s.seekRequest);
        else audio.currentTime = s.seekRequest;
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
        deckKey = null;
        nextKey = null;
        handedOver = null;
        ++loadToken;
        ++nextToken;
        decks?.stop();
        for (const el of elements) {
          el.pause();
          el.removeAttribute('src');
          el.load();
        }
      }
      return;
    }
    const key = keyOf(s.index, track);
    const entering = s.status === 'loading' && prev.status !== 'loading';
    if (entering && handedOver === key) {
      // The decks already play this track (gapless hand-over).
      handedOver = null;
      loadedKey = key;
      deckKey = key;
      engineReport.status('playing');
      refreshNext();
      return;
    }
    if (entering || (s.status === 'playing' && key !== loadedKey)) {
      void load();
      return;
    }
    const el = playing();
    if (s.status === 'playing' && el.paused && loadedKey === key) {
      if (graph) void graph.ctx.resume().catch(() => undefined);
      const p = decks ? decks.resume() : el.play();
      void Promise.resolve(p).catch(() => engineReport.status('paused'));
    } else if (s.status === 'paused' && !el.paused) {
      if (decks) decks.pause();
      else el.pause();
    }
    if (
      s.status !== prev.status &&
      (s.status === 'paused' || s.status === 'idle' || s.status === 'ended' || s.status === 'error')
    ) {
      flushUpdate(s, s.index);
    }
    // What follows depends on the queue, the position in it and the repeat mode.
    if (s.queue !== prev.queue || s.index !== prev.index || s.repeat !== prev.repeat) refreshNext();
  };

  // Element events: only the deck that plays the current track reports.
  const fromPlaying = (fn: () => void) => (e: Event) => {
    if (e.target === playing() || e.currentTarget === playing()) fn();
  };
  const onTime = fromPlaying(() => {
    const el = playing();
    const d = Number.isFinite(el.duration) ? el.duration : 0;
    engineReport.time(el.currentTime, d);
    updatePosition(el.currentTime, d);
  });
  const onMeta = fromPlaying(() => {
    const el = playing();
    if (Number.isFinite(el.duration)) engineReport.time(el.currentTime, el.duration);
  });
  const onPlaying = fromPlaying(() => {
    if (store.getState().status !== 'playing') engineReport.status('playing');
  });
  const onWaiting = fromPlaying(() => {
    if (store.getState().status === 'playing') engineReport.status('loading');
  });
  // With the decks, `ended` is theirs (they hand over or call onDecksEnded).
  const onEnded = fromPlaying(() => {
    if (!decks) endedAtBoundary();
  });
  const onError = fromPlaying(() => {
    engineReport.status('error', playing().error?.message || 'media');
  });

  const events: [string, (e: Event) => void][] = [
    ['timeupdate', onTime],
    ['loadedmetadata', onMeta],
    ['durationchange', onMeta],
    ['playing', onPlaying],
    ['waiting', onWaiting],
    ['ended', onEnded],
    ['error', onError],
  ];
  for (const el of elements) for (const [type, fn] of events) el.addEventListener(type, fn);
  const unsubscribe = store.subscribe(onStore);
  const removeActions = installActions();
  const removeGuard = setUpdateGuard(() => {
    const st = store.getState().status;
    return !atBoundary && (st === 'playing' || st === 'loading');
  });
  for (const el of elements) el.preload = 'auto';
  // A queue parked by an update reload comes back paused; its first Play loads it.
  if (store.getState().queue.length > 0) updateMetadata(currentTrack());

  // The host mounts after the first play call (it loads on demand): pick up what is already queued.
  {
    const s = store.getState();
    if (currentTrack(s) && (s.status === 'loading' || s.status === 'playing')) void load();
  }

  const self: PlayerEngine = {
    graph: () => graph,
    activeAudio: playing,
    gapless: () => decks !== null,
    destroy() {
      destroyed = true;
      if (active === self) active = null;
      unsubscribe();
      removeActions();
      removeGuard();
      decks?.destroy();
      for (const el of elements) {
        for (const [type, fn] of events) el.removeEventListener(type, fn);
        el.pause();
      }
    },
  };
  active = self;
  return self;
}
