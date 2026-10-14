// Channel favourites (TV-UI-007): channel ids in the synced user-settings blob under `cinema.tv.favourites`
// (nested `{ cinema: { tv: { favourites: [...] } } }`; the blob is an RFC 7386 merge patch, so the array is sent
// whole). Toggling is optimistic and rolls back when the write fails.
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo } from 'react';
import { apiFetch } from '../../api/client';
import { tvKeys } from './api';

interface SettingsResponse {
  version: number;
  settings: Record<string, unknown>;
}

export const FAVOURITES_KEY = 'cinema.tv.favourites';
const MAX_FAVOURITES = 200;

/** Reads `cinema.tv.favourites` (nested, or the flat dotted key) out of the blob. Never throws. */
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

export function useFavourites() {
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: tvKeys.settings(),
    queryFn: () => apiFetch<SettingsResponse>('/users/@me/settings', { retries: 0 }),
    staleTime: 60_000,
  });
  const list = useMemo(() => favouritesFrom(q.data?.settings), [q.data]);
  const set = useMemo(() => new Set(list), [list]);

  const write = useMutation({
    mutationFn: (next: string[]) =>
      apiFetch<SettingsResponse>('/users/@me/settings', {
        method: 'PATCH',
        json: { settings: { cinema: { tv: { favourites: next } } } },
        retries: 0,
      }),
    onMutate: async (next) => {
      await qc.cancelQueries({ queryKey: tvKeys.settings() });
      const prev = qc.getQueryData<SettingsResponse>(tvKeys.settings());
      qc.setQueryData<SettingsResponse>(tvKeys.settings(), {
        version: prev?.version ?? 0,
        settings: {
          ...(prev?.settings ?? {}),
          cinema: {
            ...((prev?.settings?.cinema as object | undefined) ?? {}),
            tv: {
              ...(((prev?.settings?.cinema as Record<string, unknown> | undefined)?.tv as
                object | undefined) ?? {}),
              favourites: next,
            },
          },
        },
      });
      return { prev };
    },
    onError: (_e, _n, ctx) => {
      if (ctx?.prev) qc.setQueryData(tvKeys.settings(), ctx.prev);
    },
    onSuccess: (res) => qc.setQueryData(tvKeys.settings(), res),
  });

  return {
    favourites: set,
    loaded: q.isSuccess,
    failed: write.isError,
    toggle: (channelId: string) => {
      const next = set.has(channelId)
        ? list.filter((x) => x !== channelId)
        : [...list, channelId].slice(-MAX_FAVOURITES);
      write.mutate(next);
    },
  };
}
