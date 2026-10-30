import { apiFetch, apiPost } from '../../api/client';
import type { Fetcher, LibraryPage, LibraryQuery } from '../queue/smart';
import { fromDto, type TrackDto } from '../queue/types';

// `GET /music/library?q=&mount=&album=&artist=&limit=&offset=` and `POST /music/tracks/{id}/presign` are
// music-index's routes (MUS-IDX-001); this module only calls them. The first version matches on the server
// (title, artist, album); the client-side search of wave 32 extends it.

interface TrackListDto {
  items: TrackDto[];
  total: number;
}

export function libraryUrl(q: LibraryQuery): string {
  const p = new URLSearchParams();
  if (q.q?.trim()) p.set('q', q.q.trim().slice(0, 100));
  if (q.mount) p.set('mount', q.mount);
  if (q.album) p.set('album', q.album);
  if (q.artist) p.set('artist', q.artist);
  if (q.limit) p.set('limit', String(q.limit));
  if (q.offset) p.set('offset', String(q.offset));
  const s = p.toString();
  return `/music/library${s ? `?${s}` : ''}`;
}

/** One page of the member's reachable library. */
export const fetchLibrary: Fetcher = async (q): Promise<LibraryPage> => {
  const d = await apiFetch<TrackListDto>(libraryUrl(q), { retries: 0 });
  return { items: d.items.map(fromDto), total: d.total };
};

/** A fresh URL for the track's file (a presigned `GET` by range; it expires, so it is asked for at play time). */
export async function presignTrack(trackId: string): Promise<string> {
  const r = await apiPost<{ url: string }>(`/music/tracks/${encodeURIComponent(trackId)}/presign`);
  return r.url;
}
