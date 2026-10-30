/**
 * Wave-30 integration (MUS-PLY-001 + MUS-LIB-001): the player and the music library meet here, once per page
 * load, when the player host first mounts. The host only mounts on a signed-in app route (`PlayerGate`), and
 * this module is imported by the host chunk only, so none of it runs or loads on a public page (D-385).
 *
 * - `PlayerPort`: music-lib's one door to the player, forwarded to the store's exported actions.
 * - The Library tab of the "Full Library Manager" skin gets music-lib's panel (its own lazy chunk).
 * - "Recently played" is recorded by the recent tracker, which reads the player through the port.
 */
import { lazy, type ComponentType } from 'react';
import { setPlayerPort, type PlayerPort, type PortState } from '../music/queue/port';
import { startRecentTracker } from '../music/queue/recent';
import {
  enqueue,
  play,
  registerLibraryPanel,
  setCrossfade,
  setEq,
  setEqEnabled,
  setShuffle,
  usePlayerStore,
  type PlayerState,
} from './store';

const LibraryPanel: ComponentType = lazy(() =>
  import('../music/panel/LibraryManagerPanel').then((m) => ({ default: m.LibraryManagerPanel })),
);

/** The port over the store. `getState` returns the same object while nothing it shows changed. */
export function createStorePort(): PlayerPort {
  let seen: Pick<
    PlayerState,
    'queue' | 'index' | 'eq' | 'eqEnabled' | 'crossfadeSec' | 'shuffle'
  > | null = null;
  let snap: PortState | null = null;
  const read = (): PortState => {
    const s = usePlayerStore.getState();
    if (
      !snap ||
      !seen ||
      seen.queue !== s.queue ||
      seen.index !== s.index ||
      seen.eq !== s.eq ||
      seen.eqEnabled !== s.eqEnabled ||
      seen.crossfadeSec !== s.crossfadeSec ||
      seen.shuffle !== s.shuffle
    ) {
      seen = {
        queue: s.queue,
        index: s.index,
        eq: s.eq,
        eqEnabled: s.eqEnabled,
        crossfadeSec: s.crossfadeSec,
        shuffle: s.shuffle,
      };
      snap = {
        queueIds: s.queue.map((t) => t.id),
        index: s.index,
        eq: s.eq,
        eqEnabled: s.eqEnabled,
        crossfadeSec: s.crossfadeSec,
        shuffle: s.shuffle,
      };
    }
    return snap;
  };
  return {
    // A PortTrack is a PlayerTrack (same fields; `resolveSrc` presigns right before the track starts).
    play: (tracks, startAt) => play(tracks, startAt),
    enqueue: (tracks, opts) => enqueue(tracks, opts),
    setEq,
    setEqEnabled,
    setCrossfade,
    setShuffle: (on) => setShuffle(on),
    getState: read,
    subscribe: (fn) => usePlayerStore.subscribe(() => fn()),
  };
}

export interface MusicWiringDeps {
  /** `POST /users/@me/music/recent` (injected by tests). */
  record?: (trackId: string) => Promise<unknown>;
}

let wired: (() => void) | null = null;

/** Connect the library to the player. Idempotent: the second call (a remount of the host) does nothing. */
export function wireMusicLibrary(deps: MusicWiringDeps = {}): void {
  if (wired) return;
  const record =
    deps.record ??
    ((id: string) => import('../music/playlists/api').then((m) => m.recordRecent(id)));
  const offPort = setPlayerPort(createStorePort());
  const offPanel = registerLibraryPanel(LibraryPanel);
  // After setPlayerPort: the tracker watches the port registered at this moment.
  const offRecent = startRecentTracker({ record });
  wired = () => {
    offRecent();
    offPanel();
    offPort();
  };
}

/** Test hook: undo the wiring. */
export function __unwireMusicLibrary(): void {
  wired?.();
  wired = null;
}
