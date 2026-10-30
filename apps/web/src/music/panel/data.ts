import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo } from 'react';
import {
  addFavorite,
  addToPlaylist,
  listFavorites,
  listGuildPlaylists,
  listOwnPlaylists,
  removeFavorite,
  type PlaylistView,
} from '../playlists/api';
import type { LibraryTrack } from '../queue/types';

export const KEYS = {
  favorites: ['music', 'favorites'] as const,
  recent: ['music', 'recent'] as const,
  own: ['music', 'playlists', 'own'] as const,
  guild: (id: string) => ['music', 'playlists', 'guild', id] as const,
  playlist: (id: string) => ['music', 'playlist', id] as const,
};

/** The member's favorites, and the set of their ids for the stars. */
export function useFavorites() {
  const q = useQuery({ queryKey: KEYS.favorites, queryFn: listFavorites, staleTime: 30_000 });
  const ids = useMemo(() => new Set(q.data?.items.map((i) => i.track.id) ?? []), [q.data]);
  return { ...q, ids };
}

export function useToggleFavorite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ track, on }: { track: LibraryTrack; on: boolean }) =>
      on ? addFavorite(track.id) : removeFavorite(track.id),
    onSettled: () => qc.invalidateQueries({ queryKey: KEYS.favorites }),
  });
}

/** Playlists a track can be added to: the member's own, plus the chosen server's when they may edit it. */
export function useEditablePlaylists(guildId: string | null) {
  const own = useQuery({ queryKey: KEYS.own, queryFn: listOwnPlaylists, staleTime: 30_000 });
  const guild = useQuery({
    queryKey: guildId ? KEYS.guild(guildId) : ['music', 'playlists', 'guild', 'none'],
    queryFn: () => listGuildPlaylists(guildId as string),
    enabled: guildId !== null,
    staleTime: 30_000,
  });
  const all: PlaylistView[] = [
    ...(own.data ?? []),
    ...(guild.data ?? []).filter((p) => p.can_edit),
  ];
  return { own, guild, all };
}

export function useAddToPlaylist() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ playlistId, track }: { playlistId: string; track: LibraryTrack }) =>
      addToPlaylist(playlistId, [track.id]),
    onSettled: (_d, _e, v) => {
      void qc.invalidateQueries({ queryKey: KEYS.playlist(v.playlistId) });
      void qc.invalidateQueries({ queryKey: ['music', 'playlists'] });
    },
  });
}
