/**
 * MUS-LIB-001: the track shape the library panel works with and its conversion to what the player plays.
 *
 * `LibraryTrack` is the camelCase form of the items of `GET /music/library` (music-index) and of the tracks in
 * playlists, favorites and recent (`PlaylistTrackView`): the two carry the same fields the panel needs.
 */
import type { QueueTrack } from './queue';

/** A track as the wire has it (snake_case). Only the fields the panel reads. */
export interface TrackDto {
  id: string;
  mount_id: string;
  title: string;
  artist?: string;
  album?: string;
  album_artist?: string;
  track_no?: number | null;
  disc_no?: number | null;
  year?: number | null;
  duration_ms?: number | null;
  genres?: string[];
  tags?: string[];
  cover_url?: string | null;
  /** `GET /music/library` only: the caller may write this track's tags (MUS-IDX-001). */
  can_edit?: boolean;
}

export interface LibraryTrack extends QueueTrack {
  mountId: string;
  durationMs?: number;
  coverUrl?: string;
  /** The caller may write tags on this track's mount (library listing only). */
  canEdit?: boolean;
}

export function fromDto(d: TrackDto): LibraryTrack {
  return {
    id: d.id,
    mountId: d.mount_id,
    title: d.title,
    artist: d.artist ?? '',
    album: d.album ?? '',
    albumArtist: d.album_artist ?? '',
    trackNo: d.track_no ?? null,
    discNo: d.disc_no ?? null,
    year: d.year ?? null,
    genres: d.genres ?? [],
    tags: d.tags ?? [],
    ...(d.duration_ms ? { durationMs: d.duration_ms } : {}),
    ...(d.cover_url ? { coverUrl: d.cover_url } : {}),
    ...(d.can_edit ? { canEdit: true } : {}),
  };
}

/** The player's track record (the same shape as `player/types.ts` `PlayerTrack`). */
export interface PortTrack {
  id: string;
  title: string;
  artist?: string;
  album?: string;
  /** Empty when `resolveSrc` supplies the URL right before the track starts. */
  src: string;
  artworkUrl?: string;
  durationSec?: number;
  resolveSrc?: (track: PortTrack) => Promise<string>;
}

/**
 * A library track as a player track. The file URL is a presign that expires, so it is fetched when the track
 * is about to start (`resolveSrc`), never stored in the queue.
 */
export function toPortTrack(
  t: LibraryTrack,
  presign: (trackId: string) => Promise<string>,
): PortTrack {
  return {
    id: t.id,
    title: t.title,
    ...(t.artist ? { artist: t.artist } : {}),
    ...(t.album ? { album: t.album } : {}),
    src: '',
    ...(t.coverUrl ? { artworkUrl: t.coverUrl } : {}),
    ...(t.durationMs ? { durationSec: t.durationMs / 1000 } : {}),
    resolveSrc: () => presign(t.id),
  };
}
