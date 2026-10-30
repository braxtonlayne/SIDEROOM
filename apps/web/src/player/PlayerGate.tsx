import { useRouterState } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useRef } from 'react';
import { forgetPlayer, usePlayerQueued } from './presence';
import { isPublicPath } from './scope';

const PlayerHost = lazy(() => import('./PlayerHost'));

// Browser tests start playback through this handle (the dev server only, never a production build, where the
// branch and its import are dropped). The store is loaded on demand so this file never pulls it into the
// entrance bundle.
if (import.meta.env.DEV || import.meta.env.MODE === 'test')
  void import('./store').then((actions) => {
    (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions = actions;
  });

/**
 * Mounts the host the first time something is queued on a signed-in app route and keeps it mounted while the
 * member moves between app routes, so the audio elements outlive every route change. On a public page (landing,
 * login, invite, profile, handbook) it renders nothing, never loads the player chunk, and drops whatever was
 * playing: arriving there means the session ended or the member left the app.
 *
 * This component is in the entrance bundle (the root layout), so it imports only the presence flag and the path
 * rule; the store, the engine, the skins and the music library load with the host (D-385).
 */
export function PlayerGate() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const isPublic = isPublicPath(pathname);
  const queued = usePlayerQueued();
  const everQueued = useRef(false);
  if (queued && !isPublic) everQueued.current = true;
  if (isPublic) everQueued.current = false;
  useEffect(() => {
    if (isPublic) forgetPlayer();
  }, [isPublic]);
  if (isPublic || !everQueued.current) return null;
  return (
    <Suspense fallback={null}>
      <PlayerHost />
    </Suspense>
  );
}
