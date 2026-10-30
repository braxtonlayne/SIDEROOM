// The voice jukebox as the server states it (crates/sr-api/src/routes/voice/jukebox.rs, MUS-JB-002).

export type JukeboxMode = 'stream' | 'synced';

export interface JukeboxItem {
  id: string;
  state: 'playing' | 'queued' | 'played' | 'skipped';
  added_by?: string;
  /** False means "not shared with you": silence for this member, and no title. */
  playable: boolean;
  title?: string;
  artist?: string;
  duration_ms?: number;
}

export interface JukeboxState {
  channel_id: string;
  active: boolean;
  mode: JukeboxMode;
  dj_id?: string;
  version: number;
  /** The server clock when this was read, unix ms. */
  server_now_ms: number;
  /** Position in the current track at `server_now_ms`. */
  position_ms: number;
  /** `server_now_ms - position_ms` while the track plays; absent while paused or idle. */
  started_at_ms?: number;
  paused: boolean;
  current?: JukeboxItem;
  queue: JukeboxItem[];
  votes: number;
  votes_needed: number;
  my_vote: boolean;
  listeners: number;
  i_am_dj: boolean;
  can_queue: boolean;
  can_control: boolean;
}

/** `JUKEBOX_UPDATE`: ids and the version only; the state is read per viewer. */
export interface JukeboxUpdateEvent {
  channel_id: string;
  guild_id: string;
  version: number;
}

/** A track of the member's library (`GET /music/library`) as the picker lists it. */
export interface LibraryTrack {
  id: string;
  title: string;
  artist: string;
  album: string;
  duration_ms?: number | null;
}

export interface LibraryPage {
  items: LibraryTrack[];
  total: number;
}

/** The seam the controller takes, so tests pass a fake server. */
export interface JukeboxApi {
  state(channelId: string): Promise<JukeboxState>;
  setMode(channelId: string, mode: JukeboxMode): Promise<JukeboxState>;
  enqueue(channelId: string, trackIds: string[]): Promise<JukeboxState>;
  remove(channelId: string, itemId: string): Promise<JukeboxState>;
  voteSkip(channelId: string): Promise<JukeboxState>;
  withdrawVote(channelId: string): Promise<JukeboxState>;
  skipNow(channelId: string): Promise<JukeboxState>;
  ended(channelId: string, itemId: string): Promise<JukeboxState>;
  setDj(channelId: string, userId: string | null): Promise<JukeboxState>;
  setPaused(channelId: string, paused: boolean): Promise<JukeboxState>;
  listen(channelId: string): Promise<JukeboxState>;
  leave(channelId: string): Promise<JukeboxState>;
  /** A presigned URL for the playing (or next) track; rejects with status 404 when it is not shared. */
  presign(channelId: string, itemId: string): Promise<{ url: string }>;
  library(q: string): Promise<LibraryPage>;
}
