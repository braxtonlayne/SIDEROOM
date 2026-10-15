import { useEffect } from 'react';
import { usePrefsStore } from '../settings/user/prefs';
import { useStore } from '../store';
import type { ReadState } from '../store';
import { setAppBadge } from './badge';
import { setTabBadge } from './titleBadge';

/** Total @mentions across every channel the viewer has a read state for. */
export function badgeCount(readStates: Record<string, ReadState>): number {
  let n = 0;
  for (const rs of Object.values(readStates)) n += Math.max(0, rs.mention_count || 0);
  return n;
}

/**
 * Keeps the installed app's icon badge in step with the Lounge unread store (XC-PWA-005), honouring the
 * "unread badge" notification preference. Clears the badge when switched off or when the Lounge closes.
 */
export function useAppBadge(): void {
  const readStates = useStore((s) => s.readStates);
  const enabled = usePrefsStore((s) => s.prefs.unreadBadge);
  const count = enabled ? badgeCount(readStates) : 0;
  useEffect(() => {
    setAppBadge(count);
    setTabBadge(count);
  }, [count]);
  useEffect(
    () => () => {
      setAppBadge(0);
      setTabBadge(0);
    },
    [],
  );
}
