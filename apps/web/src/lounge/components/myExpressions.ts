/**
 * The expressions of every server the viewer belongs to (`GET /users/@me/expressions`, D-375, D-376):
 * one request fills the emoji, sticker and sound pickers. The list is fetched once per session and
 * dropped when something that changes it arrives on the gateway (an expression list, a role, the
 * viewer's own member object, joining or leaving a server).
 */
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { onDispatch } from '../../store/events';
import { useStore } from '../../store';

export interface ExternalEmoji {
  id: string;
  name: string;
  animated: boolean;
  available: boolean;
  roles: string[];
}

export interface ExternalSticker {
  id: string;
  name: string;
  tags: string;
  description?: string | null;
  /** 1 PNG, 2 APNG, 3 Lottie, 4 GIF. */
  format_type: number;
  available: boolean;
}

export interface ServerExpressions {
  guild: { id: string; name: string; icon: string | null };
  /** The viewer passes the source gate in this server (USE_EXPRESSIONS_EXTERNALLY). */
  usable_externally: boolean;
  emojis: ExternalEmoji[];
  stickers: ExternalSticker[];
  /** Discord soundboard sound objects. */
  sounds: unknown[];
}

let cache: Promise<ServerExpressions[]> | null = null;
let stopWatching: (() => void) | null = null;
const listeners = new Set<() => void>();

const INVALIDATING = new Set([
  'GUILD_EMOJIS_UPDATE',
  'GUILD_STICKERS_UPDATE',
  'GUILD_SOUNDBOARD_SOUNDS_UPDATE',
  'SOUNDBOARD_SOUNDS',
  'GUILD_ROLE_CREATE',
  'GUILD_ROLE_UPDATE',
  'GUILD_ROLE_DELETE',
  'GUILD_CREATE',
  'GUILD_DELETE',
]);

/** Drop the cached list; mounted pickers fetch again. */
export function invalidateMyExpressions(): void {
  cache = null;
  for (const l of [...listeners]) l();
}

function watch(): void {
  if (stopWatching) return;
  stopWatching = onDispatch((event, data) => {
    if (INVALIDATING.has(event)) return invalidateMyExpressions();
    if (event === 'GUILD_MEMBER_UPDATE' || event === 'GUILD_MEMBER_REMOVE') {
      const uid = (data as { user?: { id?: string } } | null)?.user?.id;
      if (uid && uid === useStore.getState().currentUserId) invalidateMyExpressions();
    }
  });
}

/** The list, fetched once per session. A failed fetch is retried by the next caller. */
export function loadMyExpressions(): Promise<ServerExpressions[]> {
  watch();
  if (!cache) {
    const p = apiFetch<ServerExpressions[]>('/users/@me/expressions', { retries: 0 }).catch(
      (e: unknown) => {
        if (cache === p) cache = null;
        throw e;
      },
    );
    cache = p;
  }
  return cache;
}

/** Test hook. */
export function resetMyExpressions(): void {
  cache = null;
  stopWatching?.();
  stopWatching = null;
}

/**
 * The viewer's servers with their usable expressions, `null` until the first answer (and `[]` when
 * the list cannot be read: the pickers keep working for the current server). `enabled` false does
 * not fetch.
 */
export function useMyExpressions(enabled = true): ServerExpressions[] | null {
  const [list, setList] = useState<ServerExpressions[] | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    if (!enabled) return;
    const l = () => setTick((n) => n + 1);
    listeners.add(l);
    return () => {
      listeners.delete(l);
    };
  }, [enabled]);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    loadMyExpressions()
      .then((r) => live && setList(r))
      .catch(() => live && setList((prev) => prev ?? []));
    return () => {
      live = false;
    };
  }, [enabled, tick]);
  return list;
}
