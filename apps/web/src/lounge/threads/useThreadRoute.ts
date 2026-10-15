import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useThreads } from './threadStore';

export interface ThreadRoute {
  /** The channel the view shows (the thread's parent when the URL names a thread). */
  parentId: string;
  /** The open thread, if any. */
  threadId: string | undefined;
  /** The URL is `/channels/:guild/:thread`: the thread fills the view (G-31). */
  fullView: boolean;
  /** The id is being looked up (unknown to the store, guild loaded); show nothing yet. */
  pending: boolean;
}

/**
 * `/channels/:guild/:id` may name a thread (G-31, P10-LFE-2): from the thread store, or one `GET /channels/{id}`
 * for a thread the store has not seen (archived, opened from a link). A real channel id, or an id that
 * is not a thread, behaves as before.
 */
export function useThreadRoute(
  guildId: string | undefined,
  channelId: string | undefined,
  routeThreadId: string | undefined,
  channelIds: readonly string[],
): ThreadRoute {
  const known = !!channelId && channelIds.includes(channelId);
  const thread = useThreads((s) => (channelId ? s.byId[channelId] : undefined));
  const [tried, setTried] = useState<string | null>(null);
  const guildLoaded = channelIds.length > 0;
  const lookup = !!guildId && !!channelId && !routeThreadId && !known && !thread && guildLoaded;

  useEffect(() => {
    if (!lookup || !channelId || tried === channelId) return;
    let live = true;
    apiFetch<Parameters<ReturnType<typeof useThreads.getState>['upsert']>[0][number]>(
      `/channels/${channelId}`,
      { retries: 0 },
    )
      .then((w) => {
        if (live) useThreads.getState().upsert([w]);
      })
      .catch(() => undefined)
      .finally(() => live && setTried(channelId));
    return () => {
      live = false;
    };
  }, [lookup, channelId, tried]);

  if (channelId && routeThreadId)
    return { parentId: channelId, threadId: routeThreadId, fullView: false, pending: false };
  if (channelId && !known && thread)
    return { parentId: thread.parentId, threadId: channelId, fullView: true, pending: false };
  return {
    parentId: channelId ?? '',
    threadId: undefined,
    fullView: false,
    pending: lookup && tried !== channelId,
  };
}
