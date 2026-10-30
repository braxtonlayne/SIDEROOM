import { useRouter, useRouterState } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useRef } from 'react';
import { forgetPlayer, usePlayerLibraryOpen, usePlayerQueued } from './presence';
import { isPublicPath } from './scope';

const PlayerHost = lazy(() => import('./PlayerHost'));

/** Histories with the listener that ends the music on a public page; installed once per history, it outlives the gate. */
const watched = new WeakSet<object>();

// Browser tests start playback through this handle (the dev server only, never a production build, where the
// branch and its import are dropped). The store is loaded on demand so this file never pulls it into the
// entrance bundle.
if (import.meta.env.DEV || import.meta.env.MODE === 'test')
  void import('./store').then((actions) => {
    (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions = actions;
  });

/**
 * Mounts the host the first time something is queued, or the rail's music button asks for the library (MUS-PLY-002), on a signed-in app route and keeps it mounted while the
 * member moves between app routes, so the audio elements outlive every route change. On a public page (landing,
 * login, invite, profile, handbook) it renders nothing, never loads the player chunk, and drops whatever was
 * playing: arriving there means the session ended or the member left the app.
 *
 * The router renders this gate (its own small chunk) on app routes only; it imports only the presence flag and
 * the path rule, and the store, the engine, the skins and the music library load with the host (D-385).
 */
export function PlayerGate() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const isPublic = isPublicPath(pathname);
  const queued = usePlayerQueued();
  const libraryOpen = usePlayerLibraryOpen();
  const everQueued = useRef(false);
  if ((queued || libraryOpen) && !isPublic) everQueued.current = true;
  if (isPublic) everQueued.current = false;
  useEffect(() => {
    if (isPublic) forgetPlayer();
  }, [isPublic]);
  // The router stops rendering the gate on a public page (router.tsx), so the gate cannot see that move itself: a
  // history listener (installed once, never removed, so a remount or a strict-mode re-run changes nothing) ends the
  // music when the member lands on one.
  const router = useRouter();
  useEffect(() => {
    if (watched.has(router.history)) return;
    watched.add(router.history);
    router.history.subscribe(() => {
      if (isPublicPath(router.history.location.pathname)) forgetPlayer();
    });
  }, [router]);
  if (isPublic || !everQueued.current) return null;
  return (
    <Suspense fallback={null}>
      <PlayerHost />
    </Suspense>
  );
}
