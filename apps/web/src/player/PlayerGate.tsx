import { useRouterState } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useRef } from 'react';
import { isPublicPath } from './scope';
import * as actions from './store';
import { stopAndForget, usePlayerStore } from './store';

const PlayerHost = lazy(() => import('./PlayerHost'));

// Browser tests start playback through this handle (the dev server only, never a production build).
if (import.meta.env.DEV || import.meta.env.MODE === 'test')
  (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions = actions;

/**
 * Mounts the host the first time something is queued on a signed-in app route and keeps it mounted while the
 * member moves between app routes, so the audio element outlives every route change. On a public page (landing,
 * login, invite, profile, handbook) it renders nothing, never loads the player chunk, and drops whatever was
 * playing: arriving there means the session ended or the member left the app.
 */
export function PlayerGate() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const isPublic = isPublicPath(pathname);
  const queued = usePlayerStore((s) => s.queue.length > 0);
  const everQueued = useRef(false);
  if (queued && !isPublic) everQueued.current = true;
  if (isPublic) everQueued.current = false;
  useEffect(() => {
    if (isPublic) stopAndForget();
  }, [isPublic]);
  if (isPublic || !everQueued.current) return null;
  return (
    <Suspense fallback={null}>
      <PlayerHost />
    </Suspense>
  );
}
