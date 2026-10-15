/**
 * Incoming call state (VOX-UI-010; the outgoing ringing state waits for DM calls, VOX-SIG-008). DM and group calls ring: the server tells the person with
 * `CALL_CREATE` / `CALL_UPDATE` / `CALL_DELETE` (VOX-SIG-008, sr-gateway `voice_call.rs`):
 *
 *   CALL_CREATE  { channel_id, message_id, region, ringing: string[], initiator_id, group, voice_states }
 *   CALL_UPDATE  { channel_id, message_id, region, ringing: string[], initiator_id, group }
 *                (anyone no longer listed stops ringing)
 *   CALL_DELETE  { channel_id }                          (the call ended or nobody answered)
 *
 * The gateway does not say whether the caller started with video, so `video` stays false unless sent.
 *
 * A ring stops when the server says so; `RING_TIMEOUT_MS` is only a local fallback.
 */
import { create } from 'zustand';
import { useStore } from '../store/store';

/**
 * Local fallback only: the server stops the ring after 45 s and says so with CALL_UPDATE / CALL_DELETE, which
 * the overlay follows (G-39). This timer covers a missed dispatch.
 */
export const RING_TIMEOUT_MS = 45_000;
/** Pause between two repetitions of the ringtone. */
export const RING_INTERVAL_MS = 3_000;

export interface IncomingCall {
  channelId: string;
  callerId: string | null;
  /** The caller started with video. */
  video: boolean;
  /** A group DM rather than a one-to-one call. */
  group: boolean;
  startedAt: number;
}

interface CallsState {
  incoming: IncomingCall[];
  ring(call: IncomingCall): void;
  /** Stop ringing for a call (declined, accepted, ended or timed out). */
  dismiss(channelId: string): void;
}

export const useCalls = create<CallsState>()((set) => ({
  incoming: [],
  ring: (call) =>
    set((s) => ({ incoming: [...s.incoming.filter((c) => c.channelId !== call.channelId), call] })),
  dismiss: (channelId) =>
    set((s) => ({ incoming: s.incoming.filter((c) => c.channelId !== channelId) })),
}));

export const resetCalls = () => useCalls.setState({ incoming: [] });

const ids = (v: unknown): string[] =>
  Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];

/** Apply a gateway dispatch to the call state. Safe to call for every event. */
export function applyCallEvent(event: string, data: unknown, now = Date.now()): void {
  if (event !== 'CALL_CREATE' && event !== 'CALL_UPDATE' && event !== 'CALL_DELETE') return;
  const d = (data ?? {}) as Record<string, unknown>;
  const channelId = typeof d.channel_id === 'string' ? d.channel_id : null;
  if (!channelId) return;
  const calls = useCalls.getState();
  if (event === 'CALL_DELETE') return calls.dismiss(channelId);
  const me = useStore.getState().currentUserId;
  const ringing = ids(d.ringing);
  if (!me || !ringing.includes(me)) {
    // Not ringing for the viewer (or no longer): drop a stale ring for the same call.
    if (event === 'CALL_UPDATE' || calls.incoming.some((c) => c.channelId === channelId))
      calls.dismiss(channelId);
    return;
  }
  if (event === 'CALL_UPDATE' && calls.incoming.some((c) => c.channelId === channelId)) return;
  calls.ring({
    channelId,
    callerId: typeof d.initiator_id === 'string' ? d.initiator_id : null,
    video: d.video === true,
    group: d.group === true,
    startedAt: now,
  });
}
