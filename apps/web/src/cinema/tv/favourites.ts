// Channel favourites (TV-UI-007, TV-ENG-001): the member's starred channels live on the server
// (`GET /tv/favorites`, `PUT|DELETE /tv/favorites/{channel}`, table tv_favorites). Toggling is optimistic and
// rolls back when the write fails.
//
// Earlier builds kept the list in the synced user-settings blob under `cinema.tv.favourites`. When the server
// list is empty and the blob still has entries, they are moved over once (best effort); the blob is left alone.
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useRef } from 'react';
import { apiFetch } from '../../api/client';
import { tvKeys } from './api';

interface SettingsResponse {
  version: number;
  settings: Record<string, unknown>;
}

export const FAVOURITES_KEY = 'cinema.tv.favourites';

/** Reads the legacy `cinema.tv.favourites` (nested, or the flat dotted key) out of the blob. Never throws. */
export function favouritesFrom(settings: Record<string, unknown> | undefined): string[] {
  const flat = settings?.[FAVOURITES_KEY];
  let raw: unknown = flat;
  if (raw === undefined) {
    const cinema = settings?.cinema;
    const tv = cinema && typeof cinema === 'object' ? (cinema as Record<string, unknown>).tv : null;
    raw = tv && typeof tv === 'object' ? (tv as Record<string, unknown>).favourites : undefined;
  }
  return Array.isArray(raw) ? raw.filter((x): x is string => typeof x === 'string') : [];
}

const enc = encodeURIComponent;

export function useFavourites() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: tvKeys.favourites(),
    queryFn: async () =>
      (await apiFetch<{ items: string[] }>('/tv/favorites', { retries: 0 })).items,
    staleTime: 60_000,
  });
  const list = useMemo(() => q.data ?? [], [q.data]);
  const set = useMemo(() => new Set(list), [list]);

  // One-time move of the old blob list to the server.
  const legacy = useQuery({
    queryKey: tvKeys.settings(),
    queryFn: () => apiFetch<SettingsResponse>('/users/@me/settings', { retries: 0 }),
    staleTime: 60_000,
    enabled: q.isSuccess && list.length === 0,
  });
  const moved = useRef(false);
  useEffect(() => {
    if (moved.current || !q.isSuccess || list.length > 0 || !legacy.data) return;
    const old = favouritesFrom(legacy.data.settings);
    if (old.length === 0) return;
    moved.current = true;
    void Promise.allSettled(
      old.map((id) => apiFetch<void>(`/tv/favorites/${enc(id)}`, { method: 'PUT', retries: 0 })),
    ).then(() => qc.invalidateQueries({ queryKey: tvKeys.favourites() }));
  }, [q.isSuccess, list.length, legacy.data, qc]);

  const write = useMutation({
    mutationFn: ({ id, on }: { id: string; on: boolean }) =>
      apiFetch<void>(`/tv/favorites/${enc(id)}`, { method: on ? 'PUT' : 'DELETE', retries: 0 }),
    onMutate: async ({ id, on }) => {
      await qc.cancelQueries({ queryKey: tvKeys.favourites() });
      const prev = qc.getQueryData<string[]>(tvKeys.favourites());
      const cur = prev ?? [];
      qc.setQueryData<string[]>(
        tvKeys.favourites(),
        on ? (cur.includes(id) ? cur : [...cur, id]) : cur.filter((x) => x !== id),
      );
      return { prev };
    },
    onError: (_e, _v, ctx) => {
      if (ctx) qc.setQueryData(tvKeys.favourites(), ctx.prev);
    },
  });

  return {
    favourites: set,
    loaded: q.isSuccess,
    failed: write.isError,
    toggle: (channelId: string) => write.mutate({ id: channelId, on: !set.has(channelId) }),
  };
}
