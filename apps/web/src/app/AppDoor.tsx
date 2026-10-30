import { useRouter, useSearch } from '@tanstack/react-router';
import { useEffect } from 'react';
import { safeNext } from '../lib/enterApp';
import { useUiStore } from '../state/session';

/**
 * The neutral door of the app (D-385): `/app` (what every public surface hands a signed-in member to), and `/`
 * for an installed app or a bookmark. It renders nothing and replaces itself with the member's last section,
 * or with `?goto=` when a public surface sent them to a page of the app (an invite or a template).
 */
export function AppDoor() {
  const router = useRouter();
  const { goto } = useSearch({ strict: false }) as { goto?: string };
  useEffect(() => {
    const wanted = safeNext(goto);
    const inApp = wanted && wanted !== '/' && wanted !== '/app' && !wanted.startsWith('/app?');
    router.history.replace(inApp ? wanted : useUiStore.getState().lastSection);
  }, [router, goto]);
  return null;
}
