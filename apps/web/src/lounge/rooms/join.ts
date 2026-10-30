import { follow, planJoin, type RoomTrack, type RoomUpdate } from './cursor';
import { setListening } from './store';

/** The slice of the player this feature drives; the real one is `player/store` (loaded on demand). */
export interface RoomPlayer {
  playTracks(
    tracks: { id: string; title: string; artist: string; durationSec: number; src: string }[],
    startSec: number,
  ): void;
  seek(sec: number): void;
  stop(): void;
  /** What the player is on now. */
  current(): { trackId: string | null; positionSec: number };
}

export interface JoinDeps {
  player: RoomPlayer;
  /** `POST /music/tracks/{id}/presign`: a URL the member may read now (the shelf grant is checked there). */
  presign(trackId: string): Promise<string>;
  /** The listener's estimate of the server clock, ms. */
  now(): number;
}

async function resolved(t: RoomTrack, presign: JoinDeps['presign']) {
  return {
    id: t.track_id,
    title: t.title,
    artist: t.artist,
    durationSec: t.duration_ms / 1000,
    src: await presign(t.track_id),
  };
}

/**
 * Join a room: start local playback at the cursor. Every URL is resolved before anything starts (a refused
 * presign means this member may not read the server shelf, and nothing plays); the tracks after the first are
 * queued too, so the player runs on by itself until the next update arrives. Returns what happened so the card
 * can say it.
 */
export async function joinRoom(
  u: RoomUpdate,
  deps: JoinDeps,
): Promise<'playing' | 'idle' | 'stream' | 'wait'> {
  const plan = planJoin(u, deps.now());
  if (plan.kind !== 'play') return plan.kind;
  const queue = await Promise.all(plan.tracks.map((t) => resolved(t, deps.presign)));
  deps.player.playTracks(queue, plan.seekSec);
  setListening(u.channel_id);
  return 'playing';
}

/** Keep a joined listener on the cursor when an update arrives. */
export async function followRoom(u: RoomUpdate, deps: JoinDeps): Promise<void> {
  const cur = deps.player.current();
  const action = follow(u, deps.now(), cur.trackId, cur.positionSec);
  if (action.kind === 'seek') deps.player.seek(action.seekSec);
  else if (action.kind === 'rejoin') {
    if (action.plan.kind === 'play') await joinRoom(u, deps);
    else if (action.plan.kind === 'stream' || action.plan.kind === 'idle') leaveRoom(deps);
  }
}

export function leaveRoom(deps: Pick<JoinDeps, 'player'>): void {
  deps.player.stop();
  setListening(null);
}
