import { lazy, Suspense, useRef } from 'react';
import * as actions from './store';
import { usePlayerStore } from './store';

// Browser tests start playback through this handle (the dev server only, never a production build).
if (import.meta.env.DEV || import.meta.env.MODE === 'test')
  (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions = actions;

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
