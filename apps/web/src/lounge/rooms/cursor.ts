/**
 * MUS-JB-003: the client side of a 24/7 listening room. The server never streams audio: it publishes a cursor
 * (`LISTENING_ROOM_UPDATE`: which track, how far in, as of which server time) and each listener plays the file
 * locally from the server shelf, seeked to where the cursor says. Everything here is pure so the join rule can be
 * tested without a player, a clock or a network.
 */

export interface RoomTrack {
  item_id: string;
  track_id: string;
  title: string;
  artist: string;
  duration_ms: number;
}

export interface RoomUpdate {
  channel_id: string;
  guild_id: string;
  /** `cursor` is the 24/7 playlist; `stream` means a DJ is present and sends the music through the voice channel. */
  mode: 'cursor' | 'stream';
  /** Counts track changes (and loops): the same `seq` with a later `at_ms` is the same track, further on. */
  seq: number;
  /** Server unix time (ms) at which `position_ms` was true. */
  at_ms: number;
  position_ms: number;
  now_playing: RoomTrack | null;
  /** The next tracks in play order (at most three). */
  next: RoomTrack[];
  /** Playlist items left out because their length is unknown. */
  skipped: number;
}

const isRec = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const str = (v: unknown): string | null => (typeof v === 'string' && v !== '' ? v : null);
const int = (v: unknown): number | null =>
  typeof v === 'number' && Number.isFinite(v) ? Math.trunc(v) : null;

function parseTrack(raw: unknown): RoomTrack | null {
  if (!isRec(raw)) return null;
  const item = str(raw.item_id);
  const track = str(raw.track_id);
  const duration = int(raw.duration_ms);
  if (!item || !track || duration === null || duration <= 0) return null;
  return {
    item_id: item,
    track_id: track,
    title: typeof raw.title === 'string' ? raw.title : '',
    artist: typeof raw.artist === 'string' ? raw.artist : '',
    duration_ms: duration,
  };
}

/** Validate a gateway payload; anything malformed is dropped (`null`), never half-applied. */
export function parseRoomUpdate(raw: unknown): RoomUpdate | null {
  if (!isRec(raw)) return null;
  const channel = str(raw.channel_id);
  const guild = str(raw.guild_id);
  const seq = int(raw.seq);
  const at = int(raw.at_ms);
  const pos = int(raw.position_ms);
  if (!channel || !guild || seq === null || at === null || pos === null) return null;
  const given = raw.now_playing !== null && raw.now_playing !== undefined;
  const nowPlaying = given ? parseTrack(raw.now_playing) : null;
  if (given && !nowPlaying) return null;
  const next = Array.isArray(raw.next)
    ? raw.next.map(parseTrack).filter((t): t is RoomTrack => t !== null)
    : [];
  return {
    channel_id: channel,
    guild_id: guild,
    mode: raw.mode === 'stream' ? 'stream' : 'cursor',
    seq,
    at_ms: at,
    position_ms: Math.max(0, pos),
    now_playing: nowPlaying,
    next: next.slice(0, 3),
    skipped: Math.max(0, int(raw.skipped) ?? 0),
  };
}

/** Which of two updates for one room is newer: a higher `seq`, or the same `seq` seen later. */
export function newer(a: RoomUpdate, b: RoomUpdate): boolean {
  return a.seq !== b.seq ? a.seq > b.seq : a.at_ms > b.at_ms;
}

/** What a listener who joins now should do. */
export type JoinPlan =
  | { kind: 'idle' }
  | { kind: 'stream' }
  /** The cursor ran past the last known track before the next update; wait for it. */
  | { kind: 'wait' }
  | {
      kind: 'play';
      /** The track to start, then the ones after it for the player's queue. */
      tracks: RoomTrack[];
      /** Where to start the first one, in seconds. */
      seekSec: number;
      /** Milliseconds until the first one ends, for the card's progress. */
      endsInMs: number;
    };

/**
 * Join at the cursor. `nowMs` is the listener's estimate of the server clock. The cursor position is advanced
 * by the time since `at_ms`; when that passes the end of the track it moves on into the `next` list (the server
 * publishes at every change, so this only covers a late event). A cursor that ran past everything we know waits
 * for the next update instead of guessing a loop.
 */
export function planJoin(u: RoomUpdate, nowMs: number): JoinPlan {
  if (u.mode === 'stream') return { kind: 'stream' };
  if (!u.now_playing) return { kind: 'idle' };
  const queue = [u.now_playing, ...u.next];
  let pos = u.position_ms + Math.max(0, nowMs - u.at_ms);
  for (let i = 0; i < queue.length; i++) {
    const d = queue[i]!.duration_ms;
    if (pos < d) {
      return {
        kind: 'play',
        tracks: queue.slice(i),
        seekSec: Math.floor(pos) / 1000,
        endsInMs: d - pos,
      };
    }
    pos -= d;
  }
  return { kind: 'wait' };
}

/** Largest drift (seconds) between the player and the cursor that is left alone; beyond it the player seeks. */
export const DRIFT_TOLERANCE_SEC = 2.5;

/** For a listener already playing: what the next update asks of the player. */
export type FollowAction =
  | { kind: 'none' }
  | { kind: 'rejoin'; plan: JoinPlan }
  | { kind: 'seek'; seekSec: number };

/**
 * Follow an update. `playingTrackId` and `playerPositionSec` describe the player now. A different track means
 * rejoin; the same track more than the tolerance away from the cursor means seek; otherwise leave it alone
 * (a re-seek every 15 s would be audible).
 */
export function follow(
  u: RoomUpdate,
  nowMs: number,
  playingTrackId: string | null,
  playerPositionSec: number,
): FollowAction {
  const plan = planJoin(u, nowMs);
  if (plan.kind !== 'play') return { kind: 'rejoin', plan };
  if (plan.tracks[0]!.track_id !== playingTrackId) return { kind: 'rejoin', plan };
  if (Math.abs(plan.seekSec - playerPositionSec) > DRIFT_TOLERANCE_SEC)
    return { kind: 'seek', seekSec: plan.seekSec };
  return { kind: 'none' };
}
