import { useRouter, useRouterState } from '@tanstack/react-router';
import { lazy, Suspense, useEffect, useRef } from 'react';
import { isDrawerShortcut, matchCustomShortcut } from '../../lounge/shortcuts';
import { isPublicPath } from '../../player/scope';
import { usePrefsStore } from '../../settings/user/prefs';
import { useStore } from '../../store';
import { blobHasLayout, layoutHint } from '../gadgets/sync';
import { closeDrawer, openDrawer, useDrawerRequest } from './openState';

const DrawerHost = lazy(() => import('./DrawerHost'));

/** Histories with the listener that closes the drawer on a public page; installed once per history. */
const watched = new WeakSet<object>();

/**
 * UTL-SHL-001: mounts the drawer host the first time something needs it (Mod+K, a remembered gadget or pin, the
 * synced layout) on a signed-in app route and keeps it mounted while the member moves between app routes. Same
 * pattern as `player/PlayerGate.tsx`: the router renders this gate on app routes only, and the host, the drawer,
 * the gadgets and every tool load with the host chunk. Nothing of it is reachable from a public surface (D-385).
 * On a public page it renders nothing and closes the drawer.
 */
export function DrawerGate() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const isPublic = isPublicPath(pathname);
  const req = useDrawerRequest();
  const remembered = useStore((s) => blobHasLayout(s.userSettings?.settings)) || layoutHint();
  const everNeeded = useRef(false);
  if ((req.open || remembered) && !isPublic) everNeeded.current = true;
  if (isPublic) everNeeded.current = false;

  // Mod+K on every app route (the Lounge shell ignores this key).
  useEffect(() => {
    if (isPublic) return;
    const onKey = (e: KeyboardEvent) => {
      const id = matchCustomShortcut(e, usePrefsStore.getState().prefs.keybinds);
      if (!id || !isDrawerShortcut(id)) return;
      e.preventDefault();
      openDrawer('goto');
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isPublic]);

  useEffect(() => {
    if (isPublic) closeDrawer();
  }, [isPublic]);
  // The router stops rendering the gate on a public page, so the gate cannot see that move itself.
  const router = useRouter();
  useEffect(() => {
    if (watched.has(router.history)) return;
    watched.add(router.history);
    router.history.subscribe(() => {
      if (isPublicPath(router.history.location.pathname)) closeDrawer();
    });
  }, [router]);

  if (isPublic || !everNeeded.current) return null;
  return (
    <Suspense fallback={null}>
      <DrawerHost />
    </Suspense>
  );
}
