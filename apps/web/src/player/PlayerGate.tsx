import { lazy, Suspense, useRef } from 'react';
import { usePlayerStore } from './store';

const PlayerHost = lazy(() => import('./PlayerHost'));

/**
 * Mounts the host the first time something is queued and keeps it mounted for the rest of the session, so
 * the audio element outlives every route change. Before that it costs nothing but this check.
 */
export function PlayerGate() {
  const queued = usePlayerStore((s) => s.queue.length > 0);
  const everQueued = useRef(false);
  if (queued) everQueued.current = true;
  if (!everQueued.current) return null;
  return (
    <Suspense fallback={null}>
      <PlayerHost />
    </Suspense>
  );
}
