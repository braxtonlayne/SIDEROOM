import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';

const cache = new Map<string, string | null>();

/** Test hook. */
export const resetUpdatesChannelCache = () => cache.clear();

/**
 * The server's `public_updates_channel_id` (Community settings), read once per server while `enabled`
 * (G-30). Failures and servers without Community give null, so no badge shows.
 */
export function usePublicUpdatesChannel(guildId: string, enabled: boolean): string | null {
  const [id, setId] = useState<string | null>(cache.get(guildId) ?? null);
  useEffect(() => {
    if (!enabled) return;
    if (cache.has(guildId)) {
      setId(cache.get(guildId) ?? null);
      return;
    }
    let live = true;
    apiFetch<{ public_updates_channel_id?: string | null }>(`/guilds/${guildId}/sr/community`, {
      retries: 0,
    })
      .then((c) => {
        const v = c.public_updates_channel_id ?? null;
        cache.set(guildId, v);
        if (live) setId(v);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [guildId, enabled]);
  return enabled ? id : null;
}
