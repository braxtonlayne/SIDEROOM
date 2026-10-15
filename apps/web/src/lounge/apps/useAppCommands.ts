import { useEffect, useState } from 'react';
import type { CommandIndex } from './commands';
import { fetchCommandIndex } from './interactions';

const TTL_MS = 60_000;
const cache = new Map<string, { at: number; index: CommandIndex }>();
const inflight = new Map<string, Promise<CommandIndex | undefined>>();

/** Forget cached command lists (after an app was added or removed). */
export function invalidateAppCommands(): void {
  cache.clear();
}

function load(channelId: string): Promise<CommandIndex | undefined> {
  const hit = cache.get(channelId);
  if (hit && Date.now() - hit.at < TTL_MS) return Promise.resolve(hit.index);
  const running = inflight.get(channelId);
  if (running) return running;
  const p = fetchCommandIndex(channelId)
    .then((index) => {
      cache.set(channelId, { at: Date.now(), index });
      return index;
    })
    .catch(() => undefined)
    .finally(() => inflight.delete(channelId));
  inflight.set(channelId, p);
  return p;
}

/**
 * The apps and commands usable in a channel (DEV-BOT-008). `undefined` until loaded, in direct messages
 * (apps work in servers only) and when the request failed: the composer then simply offers no app commands.
 */
export function useAppCommands(
  channelId: string | undefined,
  enabled: boolean,
): CommandIndex | undefined {
  const [state, setState] = useState<{ channelId: string; index: CommandIndex } | undefined>();
  useEffect(() => {
    if (!channelId || !enabled) return;
    let live = true;
    void load(channelId).then((index) => {
      if (live && index) setState({ channelId, index });
    });
    return () => {
      live = false;
    };
  }, [channelId, enabled]);
  return enabled && state && state.channelId === channelId ? state.index : undefined;
}
