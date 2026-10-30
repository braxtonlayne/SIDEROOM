/**
 * MUS-PLY-001: the player store. One module-level zustand store holds what is queued, where it is, and the
 * member's choices (volume, skin, dock, EQ). The `<audio>` element and the Web Audio graph live in `engine.ts`
 * and follow this store; nothing else may touch them. Other features (music-lib, the jukebox) drive playback
 * through the exported actions in this file and never through the store itself.
 */
import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';
import { registerPlayerForget, setPlayerQueued } from './presence';
import {
  CROSSFADE_MAX_SEC,
  EQ_BANDS,
  EQ_RANGE_DB,
  PLAYER_DOCKS,
  PLAYER_SKINS,
  REPEAT_MODES,
  type LibraryPanelComponent,
  type PlayerDock,
  type PlayerSkin,
  type PlayerStatus,
  type PlayerTrack,
  type RepeatMode,
} from './types';

export interface PlayerState {
  queue: PlayerTrack[];
  /** Index into `queue` of the current track, -1 when nothing is loaded. */
  index: number;
  status: PlayerStatus;
  /** Seconds, updated by the engine about four times a second. */
  position: number;
  duration: number;
  /** 0..1 */
  volume: number;
  muted: boolean;
  skin: PlayerSkin;
  dock: PlayerDock;
  repeat: RepeatMode;
  /** Gain in dB per band of `EQ_BANDS`. */
  eq: number[];
  eqEnabled: boolean;
  /** Seconds of fade-out and fade-in around a track change, 0..12. */
  crossfadeSec: number;
  /** A seek the engine has not applied yet (seconds), or null. */
  seekRequest: number | null;
  libraryPanel: LibraryPanelComponent | null;
  /** The reason of the last failure, shown by the skins. */
  error: string | null;
}

export const initialPlayerState: PlayerState = {
  queue: [],
  index: -1,
  status: 'idle',
  position: 0,
  duration: 0,
  volume: 0.8,
  muted: false,
  skin: 'retro',
  dock: 'bottom-end',
  repeat: 'off',
  eq: EQ_BANDS.map(() => 0),
  eqEnabled: true,
  crossfadeSec: 0,
  seekRequest: null,
  libraryPanel: null,
  error: null,
};

const clamp = (n: number, lo: number, hi: number) =>
  Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : lo;

/** Only the member's choices survive a reload; a queue holds presigned URLs that expire. */
interface Persisted {
  volume: number;
  muted: boolean;
  skin: PlayerSkin;
  dock: PlayerDock;
  repeat: RepeatMode;
  eq: number[];
  eqEnabled: boolean;
  crossfadeSec: number;
}

/** Validate stored choices so a hand-edited or newer blob cannot break the player. */
export function sanitizePersisted(raw: unknown): Partial<Persisted> {
  if (!raw || typeof raw !== 'object') return {};
  const r = raw as Record<string, unknown>;
  const out: Partial<Persisted> = {};
  if (typeof r.volume === 'number') out.volume = clamp(r.volume, 0, 1);
  if (typeof r.muted === 'boolean') out.muted = r.muted;
  if (PLAYER_SKINS.includes(r.skin as PlayerSkin)) out.skin = r.skin as PlayerSkin;
  if (PLAYER_DOCKS.includes(r.dock as PlayerDock)) out.dock = r.dock as PlayerDock;
  if (REPEAT_MODES.includes(r.repeat as RepeatMode)) out.repeat = r.repeat as RepeatMode;
  if (
    Array.isArray(r.eq) &&
    r.eq.length === EQ_BANDS.length &&
    r.eq.every((x) => typeof x === 'number')
  )
    out.eq = (r.eq as number[]).map((x) => clamp(x, -EQ_RANGE_DB, EQ_RANGE_DB));
  if (typeof r.eqEnabled === 'boolean') out.eqEnabled = r.eqEnabled;
  if (typeof r.crossfadeSec === 'number')
    out.crossfadeSec = clamp(r.crossfadeSec, 0, CROSSFADE_MAX_SEC);
  return out;
}

export const PLAYER_STORAGE_KEY = 'sr.player.v1';

export const usePlayerStore = create<PlayerState>()(
  persist(() => initialPlayerState, {
    name: PLAYER_STORAGE_KEY,
    version: 1,
    storage: createJSONStorage(() => localStorage),
    partialize: (s) => ({
      volume: s.volume,
      muted: s.muted,
      skin: s.skin,
      dock: s.dock,
      repeat: s.repeat,
      eq: s.eq,
      eqEnabled: s.eqEnabled,
      crossfadeSec: s.crossfadeSec,
    }),
    merge: (persisted, current) => ({ ...current, ...sanitizePersisted(persisted) }),
  }),
);

const get = usePlayerStore.getState;
const set = usePlayerStore.setState;

/**
 * Which index follows `index`. `auto` is a track that ended by itself: repeat-one replays it and the end of the
 * queue stops. A manual Next wraps only when repeat is on.
 */
export function nextIndex(
  index: number,
  length: number,
  repeat: RepeatMode,
  auto: boolean,
): number | null {
  if (length === 0) return null;
  if (auto && repeat === 'one') return index;
  if (index + 1 < length) return index + 1;
  return repeat === 'off' ? null : 0;
}

function start(index: number) {
  const track = get().queue[index];
  if (!track) return;
  set({
    index,
    status: 'loading',
    position: 0,
    duration: track.durationSec ?? 0,
    seekRequest: null,
    error: null,
  });
}

// ---- Public actions: the only way other features drive the player ----

/**
 * Play something. With tracks: replace the queue and start at `startAt`. With a number: jump to that queue
 * position. With nothing: resume the current track (or restart a finished queue).
 */
export function play(what?: PlayerTrack | readonly PlayerTrack[] | number, startAt = 0): void {
  if (typeof what === 'number') {
    start(what);
    return;
  }
  if (what) {
    const list = Array.isArray(what) ? [...what] : [what as PlayerTrack];
    if (list.length === 0) return;
    set({ queue: list });
    start(clamp(Math.trunc(startAt), 0, list.length - 1));
    return;
  }
  const s = get();
  if (s.queue.length === 0) return;
  if (s.status === 'ended') start(0);
  else if (s.index < 0) start(0);
  else if (s.status !== 'loading') set({ status: 'playing', error: null });
}

export function pause(): void {
  const s = get();
  if (s.status === 'playing' || s.status === 'loading') set({ status: 'paused' });
}

export function toggle(): void {
  const s = get();
  if (s.status === 'playing' || s.status === 'loading') pause();
  else play();
}

/** Add tracks to the end of the queue, or right after the current one (`next`). Starts nothing by itself. */
export function enqueue(
  tracks: PlayerTrack | readonly PlayerTrack[],
  opts: { next?: boolean } = {},
): void {
  const add = Array.isArray(tracks) ? [...tracks] : [tracks as PlayerTrack];
  if (add.length === 0) return;
  const s = get();
  const at = opts.next ? Math.max(0, s.index + 1) : s.queue.length;
  set({ queue: [...s.queue.slice(0, at), ...add, ...s.queue.slice(at)] });
}

/** Stop and empty the queue (the skins' close button). */
export function clear(): void {
  set({
    queue: [],
    index: -1,
    status: 'idle',
    position: 0,
    duration: 0,
    seekRequest: null,
    error: null,
  });
}

/** Where a queue parked for an app-update reload waits (see engine.ts). */
export const RESUME_KEY = 'sr.player.resume';

/**
 * Sign-out, or a move onto a public page: stop playback and forget the queue, including the one parked in
 * sessionStorage for an update reload. The host unmounts right after, which pauses the element.
 */
export function stopAndForget(): void {
  clear();
  try {
    sessionStorage.removeItem(RESUME_KEY);
  } catch {
    /* storage can be blocked */
  }
}

export function removeAt(i: number): void {
  const s = get();
  if (i < 0 || i >= s.queue.length) return;
  const queue = s.queue.filter((_, k) => k !== i);
  if (queue.length === 0) {
    clear();
    return;
  }
  if (i < s.index) set({ queue, index: s.index - 1 });
  else if (i === s.index) {
    set({ queue });
    start(Math.min(i, queue.length - 1));
  } else set({ queue });
}

/** Advance. `auto` is true when the engine calls it because a track ended. */
export function next(auto = false): void {
  const s = get();
  const to = nextIndex(s.index, s.queue.length, s.repeat, auto);
  if (to === null) {
    if (s.queue.length > 0) set({ status: 'ended', position: 0 });
    return;
  }
  start(to);
}

/** Previous: restart the track when more than three seconds in, otherwise the one before it. */
export function prev(): void {
  const s = get();
  if (s.queue.length === 0) return;
  if (s.position > 3 || s.index <= 0) {
    seek(0);
    return;
  }
  start(s.index - 1);
}

export function seek(seconds: number): void {
  const s = get();
  if (s.index < 0) return;
  const to = clamp(seconds, 0, s.duration > 0 ? s.duration : Number.MAX_SAFE_INTEGER);
  set({ seekRequest: to, position: to });
}

export function setVolume(v: number): void {
  set({ volume: clamp(v, 0, 1), muted: false });
}

export function toggleMute(): void {
  set({ muted: !get().muted });
}

export function setSkin(skin: PlayerSkin): void {
  if (PLAYER_SKINS.includes(skin)) set({ skin });
}

export function setDock(dock: PlayerDock): void {
  if (PLAYER_DOCKS.includes(dock)) set({ dock });
}

export function setRepeat(repeat: RepeatMode): void {
  set({ repeat });
}

export function cycleRepeat(): void {
  const i = REPEAT_MODES.indexOf(get().repeat);
  setRepeat(REPEAT_MODES[(i + 1) % REPEAT_MODES.length] ?? 'off');
}

export function setEq(bands: readonly number[]): void {
  if (bands.length !== EQ_BANDS.length) return;
  set({ eq: bands.map((b) => clamp(b, -EQ_RANGE_DB, EQ_RANGE_DB)) });
}

export function setEqEnabled(on: boolean): void {
  set({ eqEnabled: on });
}

export function setCrossfade(seconds: number): void {
  set({ crossfadeSec: clamp(seconds, 0, CROSSFADE_MAX_SEC) });
}

/**
 * Give the "Full Library Manager" skin its content. music-lib calls this once at start-up with the panel
 * component; the returned function removes it again.
 */
export function registerLibraryPanel(panel: LibraryPanelComponent): () => void {
  set({ libraryPanel: panel });
  return () => {
    if (get().libraryPanel === panel) set({ libraryPanel: null });
  };
}

// ---- Reports from the engine (not for other features) ----

export const engineReport = {
  time(position: number, duration: number) {
    set({
      position,
      duration: duration > 0 && Number.isFinite(duration) ? duration : get().duration,
    });
  },
  status(status: PlayerStatus, error: string | null = null) {
    set({ status, error });
  },
  seekDone() {
    set({ seekRequest: null });
  },
};

export const currentTrack = (s: PlayerState = get()): PlayerTrack | undefined => s.queue[s.index];

// The entrance-side gate (PlayerGate) reads only the presence flag, never this module (D-385).
setPlayerQueued(get().queue.length > 0);
usePlayerStore.subscribe((st) => setPlayerQueued(st.queue.length > 0));
registerPlayerForget(stopAndForget);

/** Test hook: back to the initial state. */
export function __resetPlayerStore(): void {
  set({ ...initialPlayerState });
}
