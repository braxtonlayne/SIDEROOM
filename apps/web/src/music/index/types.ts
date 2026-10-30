// The lazy tagger's shapes (MUS-IDX-001).
//
// The `Music*` types mirror the facade the music-wasm agent publishes in packages/wasm/src/index.d.ts
// (MUS-WASM-001: `MusicApi`, `MusicTags`, `MusicNeedMoovError`). They are declared here, structurally, so this
// folder builds and is tested before that branch merges; once it has, `MusicApi` from '@sideroom/wasm' is
// assignable to `TagReader` as is.

export interface MusicCover {
  mime: string;
  bytes: Uint8Array;
}

export interface MusicTags {
  title: string | null;
  artist: string | null;
  album: string | null;
  albumArtist: string | null;
  track: number | null;
  disc: number | null;
  year: number | null;
  genre: string | null;
  durationHint: number | null;
  cover: MusicCover | null;
}

/** What the tagger needs of the facade. */
export interface TagReader {
  /** The first bytes of the file; throws on unsupported input, `{ code: 'NEED_MOOV', offset }` for a late `moov`. */
  readTags(head: Uint8Array): MusicTags;
  /** The last 128 bytes of an MP3 (ID3v1). */
  readTagsTail(tail: Uint8Array): MusicTags;
}

/** A row of `GET /music/library` (the fields the tagger uses). */
export interface TrackRow {
  id: string;
  mount_id: string;
  key: string;
  size: number;
  container: string;
  tags_state: 'pending' | 'read' | 'none' | 'failed';
}

export interface TrackPage {
  items: TrackRow[];
  total: number;
}

/** Body of `PATCH /music/tracks/{id}` as the tagger sends it. */
export interface TagPatch {
  source: 'file';
  tags_state: 'read' | 'none' | 'failed';
  title?: string;
  artist?: string;
  album?: string;
  album_artist?: string;
  track_no?: number;
  disc_no?: number;
  year?: number;
  duration_ms?: number;
  genres?: string[];
  cover_key?: string;
}

/** The server calls the tagger makes. */
export interface TaggerApi {
  /** Tracks whose tags are still to be read (`pending=true`), by album. */
  listPending(mountId: string | undefined, limit: number, offset: number): Promise<TrackPage>;
  /** `POST /music/tracks/{id}/presign`. */
  presign(trackId: string): Promise<{ url: string }>;
  patch(trackId: string, body: TagPatch): Promise<void>;
  /** `POST /music/library/rescan`. */
  rescan(mountId: string): Promise<{ queued: number }>;
  /** Stores `blob` as `key` on the mount through the bucket upload path; resolves when the object exists. */
  uploadCover(mountId: string, key: string, blob: Blob): Promise<void>;
}

export interface TaggerProgress {
  /** Tracks seen in this run. */
  total: number;
  done: number;
  /** Tags written (including "no tags in this file"). */
  tagged: number;
  /** Could not be read; "Rescan tags" tries again. */
  failed: number;
  running: boolean;
}
