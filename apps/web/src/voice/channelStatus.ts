/**
 * Voice channel status (VOX-UI-015): the line under a voice or stage channel's name, set by someone in it.
 * Backend: `PUT /channels/{id}/voice-status` `{ status: string | null }` (crates/sr-api/src/routes/voice_live.rs),
 * `VOICE_CHANNEL_STATUS_UPDATE` `{ id, guild_id, status }`, and `status` on the channel objects of `GUILD_CREATE`.
 * A status ends on its own when the last person leaves the channel.
 */
import { create } from 'zustand';
import { apiFetch } from '../api/client';

/** The longest status the server accepts, in characters. */
export const MAX_STATUS = 500;

interface StatusState {
  /** Status text by channel id; absent when none is set. */
  byChannel: Record<string, string>;
}

export const useChannelStatus = create<StatusState>()(() => ({ byChannel: {} }));

export const resetChannelStatus = () => useChannelStatus.setState({ byChannel: {} });

/** Apply a gateway dispatch. Safe to call for every event. */
export function applyStatusEvent(event: string, data: unknown): void {
  const d = (data ?? {}) as Record<string, unknown>;
  if (event === 'VOICE_CHANNEL_STATUS_UPDATE') {
    const id = typeof d.id === 'string' ? d.id : null;
    if (!id) return;
    const status = typeof d.status === 'string' && d.status ? d.status : null;
    useChannelStatus.setState((s) => {
      const next = { ...s.byChannel };
      if (status) next[id] = status;
      else delete next[id];
      return { byChannel: next };
    });
  } else if (event === 'GUILD_CREATE' && Array.isArray(d.channels)) {
    // A fresh GUILD_CREATE says everything about this server: drop what we held for its channels first.
    const ids = new Set<string>();
    const found: Record<string, string> = {};
    for (const raw of d.channels) {
      const c = (raw ?? {}) as Record<string, unknown>;
      if (typeof c.id !== 'string') continue;
      ids.add(c.id);
      if (typeof c.status === 'string' && c.status) found[c.id] = c.status;
    }
    useChannelStatus.setState((s) => {
      const next = Object.fromEntries(Object.entries(s.byChannel).filter(([id]) => !ids.has(id)));
      return { byChannel: { ...next, ...found } };
    });
  }
}

/** Set the status; an empty string or `null` clears it. */
export const setChannelStatus = (channelId: string, status: string | null) =>
  apiFetch<void>(`/channels/${channelId}/voice-status`, {
    method: 'PUT',
    json: { status },
    retries: 0,
  });
