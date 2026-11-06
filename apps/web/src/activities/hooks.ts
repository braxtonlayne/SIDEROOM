// Live state of Activities in the Lounge: the running instance of a voice channel (one REST read when the
// channel opens, then kept fresh by `SR_ACTIVITY_INSTANCE_UPDATE`) and the shelf of approved Activities.
// Plain state, no query cache (the Lounge has none), like the Watch Party hooks.
import { useCallback, useEffect, useRef, useState } from 'react';
import { onDispatch } from '../store/events';
import { httpActivityApi, type ActivityApi, type LobbyApi } from './api';
import type { ActivityInstance, InstanceUpdateEvent, Lobby, Shelf } from './types';

export interface ChannelActivityState {
  /** `[activities] enabled` on the server; false until the first answer. */
  enabled: boolean;
  instance: ActivityInstance | null;
}

export function useChannelActivity(
  channelId: string | undefined,
  api: ActivityApi = httpActivityApi,
): ChannelActivityState {
  const [state, setState] = useState<ChannelActivityState>({ enabled: false, instance: null });

  useEffect(() => {
    setState({ enabled: false, instance: null });
    if (!channelId) return;
    let dead = false;
    api
      .inChannel(channelId)
      .then((r) => !dead && setState({ enabled: r.enabled, instance: r.instance }))
      .catch(() => undefined); // 404: not a channel Activities run in
    const off = onDispatch((event, data) => {
      if (event !== 'SR_ACTIVITY_INSTANCE_UPDATE') return;
      const e = data as InstanceUpdateEvent;
      if (e.channel_id !== channelId) return;
      setState({ enabled: true, instance: e.instance });
    });
    return () => {
      dead = true;
      off();
    };
  }, [channelId, api]);

  return state;
}

/** The approved Activities; loaded when the launcher opens. */
export function useShelf(open: boolean, api: ActivityApi = httpActivityApi) {
  const [shelf, setShelf] = useState<Shelf | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!open) return;
    let dead = false;
    setFailed(false);
    api
      .shelf()
      .then((s) => !dead && setShelf(s))
      .catch(() => !dead && setFailed(true));
    return () => {
      dead = true;
    };
  }, [open, api]);
  return { shelf, failed };
}

const enabledByApi = new WeakMap<ActivityApi, Promise<boolean>>();

/**
 * Whether the server runs Activities (`[activities] enabled`). One read per session, shared by every caller,
 * so the composer can offer "Start an Activity" without a request per channel.
 */
export function useActivitiesEnabled(api: ActivityApi = httpActivityApi): boolean {
  const [on, setOn] = useState(false);
  useEffect(() => {
    let dead = false;
    let p = enabledByApi.get(api);
    if (!p) {
      p = api
        .shelf()
        .then((s) => s.enabled)
        .catch(() => false);
      enabledByApi.set(api, p);
    }
    void p.then((v) => {
      if (!dead) setOn(v);
    });
    return () => {
      dead = true;
    };
  }, [api]);
  return on;
}

/** At most one lobby refetch a second, however many instance events arrive (D-485). */
export const LOBBY_REFETCH_MS = 1000;

/**
 * The Arcade's "Active lobbies" (ARC-LOB-001, D-485): fetched on mount, and again (coalesced to one request a second)
 * whenever an `SR_ACTIVITY_INSTANCE_UPDATE` arrives, the event every viewer of a channel already gets. A failed fetch
 * keeps the last list (the first failure leaves it empty, so the section stays hidden); Activities off is empty too.
 */
export function useLobbies(
  enabled: boolean,
  api: Pick<LobbyApi, 'lobbies'> = httpActivityApi,
): { items: Lobby[]; refetch: () => void } {
  const [items, setItems] = useState<Lobby[]>([]);
  const [tick, setTick] = useState(0);
  const refetch = useCallback(() => setTick((n) => n + 1), []);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!enabled) return;
    let dead = false;
    api
      .lobbies()
      .then((r) => !dead && setItems(r.enabled ? r.items : []))
      .catch(() => undefined);
    return () => {
      dead = true;
    };
  }, [enabled, api, tick]);

  useEffect(() => {
    if (!enabled) return;
    const off = onDispatch((event) => {
      if (event !== 'SR_ACTIVITY_INSTANCE_UPDATE' || timer.current) return;
      timer.current = setTimeout(() => {
        timer.current = null;
        refetch();
      }, LOBBY_REFETCH_MS);
    });
    return () => {
      off();
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, [enabled, refetch]);

  return { items: enabled ? items : [], refetch };
}
