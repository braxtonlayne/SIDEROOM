// Live state of Activities in the Lounge: the running instance of a voice channel (one REST read when the
// channel opens, then kept fresh by `SR_ACTIVITY_INSTANCE_UPDATE`) and the shelf of approved Activities.
// Plain state, no query cache (the Lounge has none), like the Watch Party hooks.
import { useEffect, useState } from 'react';
import { onDispatch } from '../store/events';
import { httpActivityApi, type ActivityApi } from './api';
import type { ActivityInstance, InstanceUpdateEvent, Shelf } from './types';

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
