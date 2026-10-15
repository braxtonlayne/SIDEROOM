import { useCallback, useEffect, useRef, useState } from 'react';
import { onDispatch } from '../../store/events';
import { listEvents, type ScheduledEvent } from './api';

const NAMES = new Set([
  'GUILD_SCHEDULED_EVENT_CREATE',
  'GUILD_SCHEDULED_EVENT_UPDATE',
  'GUILD_SCHEDULED_EVENT_DELETE',
  'GUILD_SCHEDULED_EVENT_USER_ADD',
  'GUILD_SCHEDULED_EVENT_USER_REMOVE',
]);

/** Open (scheduled or running) events of a server; refetched when the gateway says one changed. */
export function useGuildEvents(guildId: string | undefined, enabled = true) {
  const [events, setEvents] = useState<ScheduledEvent[] | null>(null);
  const [error, setError] = useState(false);
  const alive = useRef(true);
  const guild = useRef(guildId);
  guild.current = guildId;

  const reload = useCallback(async () => {
    const id = guild.current;
    if (!id) return;
    try {
      const list = await listEvents(id);
      if (alive.current && guild.current === id) {
        setEvents(list);
        setError(false);
      }
    } catch {
      if (alive.current && guild.current === id) setError(true);
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    setEvents(null);
    if (guildId && enabled) void reload();
    return () => {
      alive.current = false;
    };
  }, [guildId, enabled, reload]);

  useEffect(() => {
    if (!guildId || !enabled) return undefined;
    return onDispatch((name, data) => {
      if (!NAMES.has(name)) return;
      const g = (data as { guild_id?: string } | null)?.guild_id;
      if (g === guildId) void reload();
    });
  }, [guildId, enabled, reload]);

  return { events, error, reload };
}
