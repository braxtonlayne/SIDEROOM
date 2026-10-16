/**
 * Go Live streams as the gateway reports them (VOX-SIG-010, crates/sr-gateway/src/voice_stream.rs): who is
 * watching whom, for the viewer count on a stream tile. The gateway sends `STREAM_CREATE` to the streamer when
 * the stream starts and to a viewer when they start watching, `STREAM_UPDATE` to the streamer and the viewers
 * whenever the audience changes, and `STREAM_DELETE` when the stream ends (to everyone) or a viewer leaves (to
 * that viewer only).
 *
 *   body: { stream_key, rtc_server_id, region, viewer_ids: string[], paused, max_viewers, quality: {height, fps} }
 *
 * The key is `guild:{guild}:{channel}:{user}`, or `call:{channel}:{user}` in a DM call; the owner is the last part.
 */
import { create } from 'zustand';

export interface StreamInfo {
  key: string;
  ownerId: string;
  viewerIds: string[];
  maxViewers: number;
  paused: boolean;
  height: number;
  fps: number;
}

interface StreamsState {
  /** Streams the viewer is told about, by the owner's user id. */
  byOwner: Record<string, StreamInfo>;
}

export const useStreams = create<StreamsState>()(() => ({ byOwner: {} }));

export const resetStreams = () => useStreams.setState({ byOwner: {} });

/** `guild:{g}:{c}:{u}` or `call:{c}:{u}` (DM calls have no guild). */
export function streamKey(guildId: string | null, channelId: string, ownerId: string): string {
  return guildId && guildId !== '@me'
    ? `guild:${guildId}:${channelId}:${ownerId}`
    : `call:${channelId}:${ownerId}`;
}

/** The owner's user id out of a stream key. */
export function ownerOfKey(key: string): string | null {
  const parts = key.split(':');
  const last = parts[parts.length - 1];
  return (parts[0] === 'guild' && parts.length === 4) || (parts[0] === 'call' && parts.length === 3)
    ? (last ?? null)
    : null;
}

const strings = (v: unknown): string[] =>
  Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];

/**
 * Apply a gateway dispatch. Returns what changed for the viewer so the controller can follow up: `ended` is a
 * stream key that is gone for us (the owner stopped, or we stopped watching), `started` one we now receive.
 */
export function applyStreamEvent(
  event: string,
  data: unknown,
  me: string | null,
): { ended?: string; started?: string } {
  if (event !== 'STREAM_CREATE' && event !== 'STREAM_UPDATE' && event !== 'STREAM_DELETE')
    return {};
  const d = (data ?? {}) as Record<string, unknown>;
  const key = typeof d.stream_key === 'string' ? d.stream_key : null;
  const owner = key ? ownerOfKey(key) : null;
  if (!key || !owner) return {};
  if (event === 'STREAM_DELETE') {
    // Sent to everyone when the owner stops, and to a viewer alone when they stop watching: either way the
    // audience figures we had are no longer ours to show.
    useStreams.setState((s) => {
      const next = { ...s.byOwner };
      delete next[owner];
      return { byOwner: next };
    });
    return { ended: key };
  }
  const q = (d.quality ?? {}) as Record<string, unknown>;
  const info: StreamInfo = {
    key,
    ownerId: owner,
    viewerIds: strings(d.viewer_ids),
    maxViewers: typeof d.max_viewers === 'number' ? d.max_viewers : 0,
    paused: d.paused === true,
    height: typeof q.height === 'number' ? q.height : 0,
    fps: typeof q.fps === 'number' ? q.fps : 0,
  };
  useStreams.setState((s) => ({ byOwner: { ...s.byOwner, [owner]: info } }));
  return event === 'STREAM_CREATE' && owner !== me ? { started: key } : {};
}
