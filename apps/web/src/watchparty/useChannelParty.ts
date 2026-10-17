// The live watch party of a channel or DM, for the cards in the Lounge: one REST read when the first card of the
// channel mounts, then kept fresh by the gateway (`WATCH_PARTY_UPDATE`). The strip above the channel and every party
// card in its history share one read and one subscription per channel (reference counted). Plain state, no query
// cache (the Lounge has none).
import { useCallback, useSyncExternalStore } from 'react';
import { onDispatch } from '../store/events';
import { httpPartyApi } from './api';
import type { Party, PartyUpdateEvent } from './types';

interface Entry {
  party: Party | null;
  users: number;
  listeners: Set<() => void>;
  stop: () => void;
}

const entries = new Map<string, Entry>();

function open(channelId: string): Entry {
  const entry: Entry = { party: null, users: 0, listeners: new Set(), stop: () => undefined };
  const set = (p: Party | null) => {
    entry.party = p;
    entry.listeners.forEach((l) => l());
  };
  let dead = false;
  httpPartyApi
    .inChannel(channelId)
    .then((p) => !dead && set(p.ended ? null : p))
    .catch(() => undefined); // 404: no party here
  const off = onDispatch((event, data) => {
    if (event !== 'WATCH_PARTY_UPDATE') return;
    const e = data as PartyUpdateEvent;
    if (!('party' in e) || e.party.channel_id !== channelId) return;
    set(e.party.ended ? null : e.party);
  });
  entry.stop = () => {
    dead = true;
    off();
  };
  return entry;
}

const NONE: Party | null = null;

export function useChannelParty(channelId: string | undefined): Party | null {
  // A stable subscribe: a new function every render would resubscribe (and re-read) on every render.
  const subscribe = useCallback(
    (notify: () => void) => {
      if (!channelId) return () => undefined;
      let entry = entries.get(channelId);
      if (!entry) {
        entry = open(channelId);
        entries.set(channelId, entry);
      }
      entry.users += 1;
      entry.listeners.add(notify);
      return () => {
        const e = entries.get(channelId);
        if (!e) return;
        e.listeners.delete(notify);
        e.users -= 1;
        if (e.users <= 0) {
          e.stop();
          entries.delete(channelId);
        }
      };
    },
    [channelId],
  );
  return useSyncExternalStore(
    subscribe,
    () => (channelId ? (entries.get(channelId)?.party ?? NONE) : NONE),
    () => NONE,
  );
}
