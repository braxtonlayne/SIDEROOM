/**
 * UTL-DEV-001: sync a timer to a voice channel.
 *
 * Backend: `PUT /channels/{id}/voice-timer` `{ timer: WireTimer | null, label: string }` (needs the same
 * rights as `PUT /channels/{id}/voice-status`: SET_VOICE_CHANNEL_STATUS while connected, or MANAGE_CHANNELS).
 * The server puts `label` on the channel's status line and dispatches `VOICE_TIMER_UPDATE`
 * `{ id, guild_id, user_id, server_ms, timer }` to the members who can see the channel. Nothing is stored: the
 * timer ends with the status, when the last person leaves.
 *
 * Receivers read the clock of the server, not the sender's: `skewMs` is how far ahead the server was when the
 * event arrived, and the displayed time uses `Date.now() + skewMs`.
 */
import { create } from 'zustand';
import { apiFetch } from '../../api/client';
import { onDispatch } from '../../store/events';
import { type WireTimer, parseWire } from './timerCore';

export interface SharedTimer {
  timer: WireTimer;
  /** Who started it. */
  userId: string | null;
  /** Server clock minus local clock, measured when the update arrived. */
  skewMs: number;
}

interface VoiceTimerState {
  byChannel: Record<string, SharedTimer>;
}

export const useVoiceTimers = create<VoiceTimerState>()(() => ({ byChannel: {} }));

export const resetVoiceTimers = () => useVoiceTimers.setState({ byChannel: {} });

/** Apply a gateway dispatch. Safe to call for every event. */
export function applyVoiceTimerEvent(event: string, data: unknown, localNow = Date.now()): void {
  const d = (data ?? {}) as Record<string, unknown>;
  const id = typeof d.id === 'string' ? d.id : null;
  if (!id) return;
  // A timer ends with the channel's status line (the last person left, or someone cleared it).
  if (event === 'VOICE_CHANNEL_STATUS_UPDATE') {
    if (!d.status) {
      useVoiceTimers.setState((s) => {
        if (!(id in s.byChannel)) return s;
        const next = { ...s.byChannel };
        delete next[id];
        return { byChannel: next };
      });
    }
    return;
  }
  if (event !== 'VOICE_TIMER_UPDATE') return;
  const timer = parseWire(d.timer);
  useVoiceTimers.setState((s) => {
    const next = { ...s.byChannel };
    if (timer) {
      const serverMs = typeof d.server_ms === 'number' ? d.server_ms : localNow;
      next[id] = {
        timer,
        userId: typeof d.user_id === 'string' ? d.user_id : null,
        skewMs: serverMs - localNow,
      };
    } else {
      delete next[id];
    }
    return { byChannel: next };
  });
}

/** Keep the shared timers in step with the gateway while the caller is mounted. Returns the unsubscribe. */
export function startVoiceTimerListener(): () => void {
  return onDispatch((event, data) => applyVoiceTimerEvent(event, data));
}

/** Publish (or, with `null`, stop publishing) a timer on a voice channel. */
export const publishVoiceTimer = (channelId: string, timer: WireTimer | null, label: string) =>
  apiFetch<void>(`/channels/${channelId}/voice-timer`, {
    method: 'PUT',
    json: { timer, label },
    retries: 0,
  });
