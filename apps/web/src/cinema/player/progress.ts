// Watch progress (CIN-PLY-007 client side): the one place the player talks to the progress endpoints.
//
// Two implementations sit behind one interface, exactly like library/progress.ts:
//   - `httpProgress`: the social routes (spec cinema-media section 8.1), so a position follows the account
//     across devices;
//   - `localProgress`: localStorage, used by tests and as the fallback when a save fails.
// Nothing else in the UI imports either one.
//
// Routes (a movie is addressed by its title alone; its episode id is "0" in the data):
//   GET|PUT /cinema/titles/{title}/progress                        movies
//   GET|PUT /cinema/titles/{title}/episodes/{episode}/progress     episodes
//   PUT body {position_secs, duration_secs, completed?, playing}   -> {progress, applied, recorded}
//   GET -> Progress {title_id, episode_id, position_secs, duration_secs, percent, completed, updated_at_ms} (404: none)
// `completed` is decided by the server (percent >= 90, sticky); the player sends it only when playback ended.
import { apiFetch } from '../../api/client';
import type { WatchProgress } from './types';

export const USE_SERVER_PROGRESS = true;

export interface WatchProgressInput {
  titleId: string;
  /** null for a movie. */
  episodeId: string | null;
  positionS: number;
  durationS: number;
  /** Set only when playback reached the end. */
  completed?: boolean;
  /** True on the heartbeat while the picture is moving; false on pause, stop and leaving (ends "watching"). */
  playing: boolean;
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
  get(titleId: string, episodeId: string | null): Promise<WatchProgress | null>;
}

const enc = encodeURIComponent;
const path = (titleId: string, episodeId: string | null) =>
  episodeId && episodeId !== '0'
    ? `/cinema/titles/${enc(titleId)}/episodes/${enc(episodeId)}/progress`
    : `/cinema/titles/${enc(titleId)}/progress`;

interface WireProgress {
  position_secs: number;
  duration_secs: number;
  completed: boolean;
  updated_at_ms: number;
}

export const httpProgress: WatchProgressApi = {
  async save(p, opts) {
    await apiFetch(path(p.titleId, p.episodeId), {
      method: 'PUT',
      json: {
        position_secs: Math.floor(p.positionS),
        duration_secs: Math.floor(p.durationS),
        completed: p.completed || undefined,
        playing: p.playing,
      },
      retries: 0,
      keepalive: opts?.keepalive,
    });
  },
  async get(titleId, episodeId) {
    try {
      const w = await apiFetch<WireProgress>(path(titleId, episodeId), { retries: 0 });
      return {
        position_s: w.position_secs,
        duration_s: w.duration_secs,
        completed: w.completed,
        updated_at_ms: w.updated_at_ms,
      };
    } catch {
      return null;
    }
  },
};

const PROGRESS_KEY = 'sr.cinema.progress.v1';
const keyOf = (titleId: string, episodeId: string | null) => `${titleId}:${episodeId ?? '0'}`;

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
    const k = keyOf(p.titleId, p.episodeId);
    all[k] = { ...p, completed: p.completed ?? all[k]?.completed, updatedAtMs: Date.now() };
    writeAll(all);
    return Promise.resolve();
  },
  get(titleId, episodeId) {
    const it = readAll()[keyOf(titleId, episodeId)];
    return Promise.resolve(
      it
        ? {
            position_s: it.positionS,
            duration_s: it.durationS,
            completed: it.completed ?? false,
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
  async get(titleId, episodeId) {
    return (
      (await current.get(titleId, episodeId)) ??
      (current === localProgress ? null : localProgress.get(titleId, episodeId))
    );
  },
};

/** Tests swap it with `setProgressApi`; `null` restores the default. */
export function setProgressApi(api: WatchProgressApi | null) {
  current = api ?? (USE_SERVER_PROGRESS ? httpProgress : localProgress);
}
