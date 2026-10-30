// The lazy tagger's shapes (MUS-IDX-001).
//
// The tag types are the `sr-music-wasm` facade's own (MUS-WASM-001, `packages/wasm/src/index.d.ts`); only
// types are imported here, so nothing of the facade is bundled until the tagger loads it.
import type { MusicApi } from '@sideroom/wasm';

export type { MusicCover, MusicNeedMoovError, MusicTags } from '@sideroom/wasm';

/**
 * What the tagger needs of the facade: `readTags` (the first bytes of the file; throws a
 * `MusicNeedMoovError` for a late `moov`) and `readTagsTail` (the last 128 bytes of an MP3, ID3v1).
 */
export type TagReader = Pick<MusicApi, 'readTags' | 'readTagsTail'>;

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
