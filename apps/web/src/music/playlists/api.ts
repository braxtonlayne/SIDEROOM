import { apiFetch, apiPost } from '../../api/client';
import { fromDto, type LibraryTrack, type TrackDto } from '../queue/types';

// Shapes checked against crates/sr-api/src/routes/music/{mod,playlists,favorites,recent}.rs (MUS-LIB-001).

export interface PlaylistView {
  id: string;
  owner_type: 'user' | 'guild';
  owner_id: string;
  name: string;
  description: string;
  created_by?: string;
  item_count: number;
  created_ms: number;
  updated_ms: number;
  can_edit: boolean;
}

interface PlaylistItemDto {
  item_id: string;
  position: number;
  track: TrackDto;
}

interface PlaylistDetailDto {
  playlist: PlaylistView;
  items: PlaylistItemDto[];
  hidden: number;
}

export interface PlaylistItem {
  itemId: string;
  position: number;
  track: LibraryTrack;
}

export interface PlaylistDetail {
  playlist: PlaylistView;
  items: PlaylistItem[];
  /** Items left out because their bucket is not readable by the viewer. */
  hidden: number;
}

interface StampedDto {
  track: TrackDto;
  at_ms: number;
}

export interface StampedList {
  items: { track: LibraryTrack; atMs: number }[];
  hidden: number;
}

const enc = encodeURIComponent;
const stamped = (d: { items: StampedDto[]; hidden: number }): StampedList => ({
  items: d.items.map((i) => ({ track: fromDto(i.track), atMs: i.at_ms })),
  hidden: d.hidden,
});

// ---- playlists ----

export const listOwnPlaylists = () =>
  apiFetch<PlaylistView[]>('/users/@me/music/playlists', { retries: 0 });

export const listGuildPlaylists = (guildId: string) =>
  apiFetch<PlaylistView[]>(`/guilds/${enc(guildId)}/music/playlists`, { retries: 0 });

export const createPlaylist = (name: string, guildId?: string) =>
  apiPost<PlaylistView>(
    guildId ? `/guilds/${enc(guildId)}/music/playlists` : '/users/@me/music/playlists',
    { name },
  );

export async function getPlaylist(id: string): Promise<PlaylistDetail> {
  const d = await apiFetch<PlaylistDetailDto>(`/music/playlists/${enc(id)}`, { retries: 0 });
  return {
    playlist: d.playlist,
    hidden: d.hidden,
    items: d.items.map((i) => ({
      itemId: i.item_id,
      position: i.position,
      track: fromDto(i.track),
    })),
  };
}

export const renamePlaylist = (id: string, name: string) =>
  apiFetch<PlaylistView>(`/music/playlists/${enc(id)}`, {
    method: 'PATCH',
    json: { name },
    retries: 0,
  });

export const deletePlaylist = (id: string) =>
  apiFetch<void>(`/music/playlists/${enc(id)}`, { method: 'DELETE', retries: 0 });

export const addToPlaylist = (id: string, trackIds: readonly string[]) =>
  apiPost<PlaylistView>(`/music/playlists/${enc(id)}/items`, { track_ids: trackIds });

export const removeFromPlaylist = (id: string, itemId: string) =>
  apiFetch<void>(`/music/playlists/${enc(id)}/items/${enc(itemId)}`, {
    method: 'DELETE',
    retries: 0,
  });

export const moveInPlaylist = (id: string, itemId: string, position: number) =>
  apiFetch<void>(`/music/playlists/${enc(id)}/items/${enc(itemId)}`, {
    method: 'PATCH',
    json: { position },
    retries: 0,
  });

// ---- favorites and recent ----

export async function listFavorites(): Promise<StampedList> {
  return stamped(
    await apiFetch<{ items: StampedDto[]; hidden: number }>('/users/@me/music/favorites', {
      retries: 0,
    }),
  );
}

export const addFavorite = (trackId: string) =>
  apiFetch<void>(`/users/@me/music/favorites/${enc(trackId)}`, { method: 'PUT', retries: 0 });

export const removeFavorite = (trackId: string) =>
  apiFetch<void>(`/users/@me/music/favorites/${enc(trackId)}`, { method: 'DELETE', retries: 0 });

export async function listRecent(limit = 50): Promise<StampedList> {
  return stamped(
    await apiFetch<{ items: StampedDto[]; hidden: number }>(
      `/users/@me/music/recent?limit=${limit}`,
      { retries: 0 },
    ),
  );
}

export const recordRecent = (trackId: string) =>
  apiPost<void>('/users/@me/music/recent', { track_id: trackId });

export const clearRecent = () =>
  apiFetch<void>('/users/@me/music/recent', { method: 'DELETE', retries: 0 });
