import { useStore } from '../../store';

/**
 * The server's `public_updates_channel_id` while `enabled` (G-30), read from the guild object the store already
 * holds (G-76: `GET /guilds/{id}`, `GUILD_CREATE` and `GUILD_UPDATE` carry it), so there is no extra request. A server
 * without Community, or one the store has not loaded yet, gives null and no badge shows.
 */
export function usePublicUpdatesChannel(guildId: string, enabled: boolean): string | null {
  const id = useStore((s) => s.guilds[guildId]?.public_updates_channel_id ?? null);
  return enabled ? id : null;
}
