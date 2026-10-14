// Watch progress (CIN-PLY-007 client side): the one place the player talks to the progress endpoints.
//
// Two implementations sit behind one interface, exactly like library/progress.ts:
//   - `httpProgress`: the server routes (assumed shapes below), so a position follows the account across devices;
//   - `localProgress`: localStorage, used by tests and as the fallback when a save fails.
// Nothing else in the UI imports either one.
//
// Routes assumed (docs/specs/cinema-media.md is the contract; adjust only this file if they differ):
//   PUT /cinema/progress/{mediaId}  {title_id, episode_id?, position_s, duration_s, completed}  -> 204
//   GET /cinema/progress/{mediaId}  -> WatchProgress (404 when none)
import { apiFetch } from '../../api/client';
import type { WatchProgress } from './types';

export const USE_SERVER_PROGRESS = true;

export interface WatchProgressInput {
  mediaId: string;
  titleId: string;
  episodeId: string | null;
  positionS: number;
  durationS: number;
  completed: boolean;
  /** Display meta so "continue watching" works from the local store alone. */
  title?: string;
  episodeLabel?: string | null;
  posterUrl?: string | null;
}

export interface LocalWatchItem extends WatchProgressInput {
  updatedAtMs: number;
}

export interface WatchProgressApi {
  /** `keepalive` lets the request outlive the page (pagehide / visibilitychange). */
  save(p: WatchProgressInput, opts?: { keepalive?: boolean }): Promise<void>;
  get(mediaId: string): Promise<WatchProgress | null>;
}

export const httpProgress: WatchProgressApi = {
  async save(p, opts) {
    await apiFetch(`/cinema/progress/${encodeURIComponent(p.mediaId)}`, {
      method: 'PUT',
      json: {
        title_id: p.titleId,
        episode_id: p.episodeId ?? undefined,
        position_s: Math.floor(p.positionS),
        duration_s: Math.floor(p.durationS),
        completed: p.completed,
      },
      retries: 0,
      keepalive: opts?.keepalive,
    });
  },
  async get(mediaId) {
    try {
      return await apiFetch<WatchProgress>(`/cinema/progress/${encodeURIComponent(mediaId)}`, {
        retries: 0,
      });
    } catch {
      return null;
    }
  },
};

const PROGRESS_KEY = 'sr.cinema.progress.v1';

function readAll(): Record<string, LocalWatchItem> {
  try {
    const raw = localStorage.getItem(PROGRESS_KEY);
    return raw ? (JSON.parse(raw) as Record<string, LocalWatchItem>) : {};
  } catch {
    return {};
  }
}
function writeAll(all: Record<string, LocalWatchItem>) {
  try {
    localStorage.setItem(PROGRESS_KEY, JSON.stringify(all));
  } catch {
    /* private mode or full: progress is best effort */
  }
}

export const localProgress: WatchProgressApi & { recent(limit: number): LocalWatchItem[] } = {
  save(p) {
    const all = readAll();
    all[p.mediaId] = { ...p, updatedAtMs: Date.now() };
    writeAll(all);
    return Promise.resolve();
  },
  get(mediaId) {
    const it = readAll()[mediaId];
    return Promise.resolve(
      it
        ? {
            position_s: it.positionS,
            duration_s: it.durationS,
            completed: it.completed,
            updated_at_ms: it.updatedAtMs,
          }
        : null,
    );
  },
  recent(limit) {
    return Object.values(readAll())
      .filter((i) => !i.completed)
      .sort((a, b) => b.updatedAtMs - a.updatedAtMs)
      .slice(0, limit);
  },
};

let current: WatchProgressApi = USE_SERVER_PROGRESS ? httpProgress : localProgress;

/**
 * The implementation the UI uses. A failed server save is kept in the local store so the next visit can still
 * offer to resume; `get` prefers the server, then the local copy.
 */
export const progressApi: WatchProgressApi = {
  async save(p, opts) {
    try {
      await current.save(p, opts);
    } catch (e) {
      if (current !== localProgress) await localProgress.save(p);
      throw e;
    }
  },
  async get(mediaId) {
    return (
      (await current.get(mediaId)) ??
      (current === localProgress ? null : localProgress.get(mediaId))
    );
  },
};

/** Tests swap it with `setProgressApi`; `null` restores the default. */
export function setProgressApi(api: WatchProgressApi | null) {
  current = api ?? (USE_SERVER_PROGRESS ? httpProgress : localProgress);
}
