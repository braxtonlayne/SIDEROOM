import { useCallback } from 'react';
import { getPlayerPort } from '../queue/port';
import { buildSmartQueue, type SmartKind } from '../queue/smart';
import { toPortTrack, type LibraryTrack } from '../queue/types';
import { fetchLibrary, presignTrack } from '../search/api';

/**
 * The actions every track list offers, all through the player port. Shuffle is the player's mode, not a
 * per-list option: a queue started while it is on opens with the chosen track and shuffles the rest.
 */
export function useMusicActions() {
  const toPort = useCallback((t: LibraryTrack) => toPortTrack(t, presignTrack), []);

  const play = useCallback(
    (tracks: readonly LibraryTrack[], startAt = 0) => {
      if (tracks.length === 0) return;
      getPlayerPort().play(tracks.map(toPort), startAt);
    },
    [toPort],
  );

  const enqueue = useCallback(
    (tracks: readonly LibraryTrack[], next = false) => {
      if (tracks.length === 0) return;
      getPlayerPort().enqueue(tracks.map(toPort), { next });
    },
    [toPort],
  );

  const playSmart = useCallback(
    async (kind: SmartKind, seed: LibraryTrack) => {
      const r = await buildSmartQueue(kind, seed, fetchLibrary);
      play(r.tracks, r.startAt);
    },
    [play],
  );

  return { play, enqueue, playSmart };
}
