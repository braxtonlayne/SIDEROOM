// Reading progress and bookmarks: the one place the Library UI talks to those (LIB-RDR-012/013).
//
// Two implementations sit behind one interface:
//   - `httpProgress`: the server routes (docs/specs/library-reader.md section 6), used by default so progress follows
//     the account across devices;
//   - `localProgress`: localStorage, kept for tests and as a fallback.
// Nothing else in the UI imports either one.
//
// Routes used (the OpenAPI document is the contract):
//   PUT    /library/chapters/{id}/progress   {page_index, percent, completed}   -> {progress, applied}
//   GET    /library/chapters/{id}/progress   -> Progress (404 when none)
//   GET    /library/me/progress?limit=       -> {items: [{series, chapter}], next_before}
//   GET    /library/chapters/{id}/bookmarks  -> {bookmarks: [Bookmark]}
//   PUT    /library/chapters/{id}/bookmarks  {page_index, note?} -> Bookmark (one per page)
//   DELETE /library/bookmarks/{id}
//
// The server also supports a conflict prompt (base_updated_at_ms / applied: false). The reader does not use it yet,
// so a save always applies.
import { apiFetch } from '../api/client';
import type { Chapter, ChapterProgress, Series } from './types';

export const USE_SERVER_PROGRESS = true;

/** What the reader reports for a chapter. The meta fields feed "Pick up where you left off" in local mode. */
export interface ProgressInput {
  chapterId: string;
  seriesId: string;
  seriesTitle: string;
  coverUrl?: string | null;
  chapterNumber: number;
  pageCount: number;
  pageIndex: number;
  /** 0..100 */
  percent: number;
  completed: boolean;
}

export interface RecentItem extends ProgressInput {
  updatedAtMs: number;
}

export interface Bookmark {
  id: string;
  chapterId: string;
  pageIndex: number;
  note: string;
  createdAtMs: number;
}

export interface ProgressApi {
  save(p: ProgressInput): Promise<void>;
  recent(limit: number): Promise<RecentItem[]>;
  /** Per-chapter progress of one series, for read marks. Empty for the server (the chapter list carries it). */
  forSeries(seriesId: string): Promise<Record<string, ChapterProgress>>;
  /** Saved position of one chapter, if any. */
  get(chapterId: string): Promise<ChapterProgress | null>;
  bookmarks(chapterId: string): Promise<Bookmark[]>;
  addBookmark(chapterId: string, pageIndex: number, note?: string): Promise<Bookmark>;
  removeBookmark(chapterId: string, id: string): Promise<void>;
}

/* ------------------------------------------------------------------ http */

interface WireBookmark {
  id: string;
  chapter_id: string;
  page_index: number;
  note?: string | null;
  created_at_ms: number;
}
const fromWire = (chapterId: string, b: WireBookmark): Bookmark => ({
  id: b.id,
  chapterId,
  pageIndex: b.page_index,
  note: b.note ?? '',
  createdAtMs: b.created_at_ms,
});

export const httpProgress: ProgressApi = {
  async save(p) {
    await apiFetch(`/library/chapters/${encodeURIComponent(p.chapterId)}/progress`, {
      method: 'PUT',
      json: { page_index: p.pageIndex, percent: p.percent, completed: p.completed },
      retries: 0,
    });
  },
  async recent(limit) {
    const r = await apiFetch<{ items: { series: Series; chapter: Chapter }[] }>(
      `/library/me/progress?limit=${limit}`,
    );
    return r.items.flatMap(({ series, chapter }) =>
      chapter.progress
        ? [
            {
              chapterId: chapter.id,
              seriesId: series.id,
              seriesTitle: series.title,
              coverUrl: series.cover_url ?? null,
              chapterNumber: chapter.number,
              pageCount: chapter.page_count,
              pageIndex: chapter.progress.page_index,
              percent: chapter.progress.percent,
              completed: chapter.progress.completed,
              updatedAtMs: chapter.progress.updated_at_ms ?? 0,
            },
          ]
        : [],
    );
  },
  forSeries: () => Promise.resolve({}),
  async get(chapterId) {
    try {
      return await apiFetch<ChapterProgress>(
        `/library/chapters/${encodeURIComponent(chapterId)}/progress`,
        { retries: 0 },
      );
    } catch {
      return null;
    }
  },
  async bookmarks(chapterId) {
    const r = await apiFetch<{ bookmarks: WireBookmark[] }>(
      `/library/chapters/${encodeURIComponent(chapterId)}/bookmarks`,
    );
    return r.bookmarks.map((b) => fromWire(chapterId, b));
  },
  async addBookmark(chapterId, pageIndex, note) {
    const b = await apiFetch<WireBookmark>(
      `/library/chapters/${encodeURIComponent(chapterId)}/bookmarks`,
      { method: 'PUT', json: { page_index: pageIndex, note: note || undefined }, retries: 0 },
    );
    return fromWire(chapterId, b);
  },
  async removeBookmark(_chapterId, id) {
    await apiFetch(`/library/bookmarks/${encodeURIComponent(id)}`, {
      method: 'DELETE',
      retries: 0,
    });
  },
};

/* ----------------------------------------------------------------- local */

const PROGRESS_KEY = 'sr.library.progress.v1';
const BOOKMARK_KEY = 'sr.library.bookmarks.v1';

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}
function writeJson(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* private mode or full: progress is best effort */
  }
}

export const localProgress: ProgressApi = {
  save(p) {
    const all = readJson<Record<string, RecentItem>>(PROGRESS_KEY, {});
    all[p.chapterId] = { ...p, updatedAtMs: Date.now() };
    writeJson(PROGRESS_KEY, all);
    return Promise.resolve();
  },
  recent(limit) {
    const all = Object.values(readJson<Record<string, RecentItem>>(PROGRESS_KEY, {}));
    // One entry per series: the most recently read chapter, unless it is finished.
    const bySeries = new Map<string, RecentItem>();
    for (const it of all.sort((a, b) => b.updatedAtMs - a.updatedAtMs))
      if (!bySeries.has(it.seriesId)) bySeries.set(it.seriesId, it);
    return Promise.resolve([...bySeries.values()].filter((i) => !i.completed).slice(0, limit));
  },
  forSeries(seriesId) {
    const all = readJson<Record<string, RecentItem>>(PROGRESS_KEY, {});
    const out: Record<string, ChapterProgress> = {};
    for (const it of Object.values(all))
      if (it.seriesId === seriesId)
        out[it.chapterId] = {
          page_index: it.pageIndex,
          percent: it.percent,
          completed: it.completed,
        };
    return Promise.resolve(out);
  },
  get(chapterId) {
    const it = readJson<Record<string, RecentItem>>(PROGRESS_KEY, {})[chapterId];
    return Promise.resolve(
      it ? { page_index: it.pageIndex, percent: it.percent, completed: it.completed } : null,
    );
  },
  bookmarks(chapterId) {
    const all = readJson<Bookmark[]>(BOOKMARK_KEY, []);
    return Promise.resolve(all.filter((b) => b.chapterId === chapterId));
  },
  addBookmark(chapterId, pageIndex, note = '') {
    const all = readJson<Bookmark[]>(BOOKMARK_KEY, []);
    const b: Bookmark = {
      id: `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
      chapterId,
      pageIndex,
      note,
      createdAtMs: Date.now(),
    };
    writeJson(BOOKMARK_KEY, [...all, b]);
    return Promise.resolve(b);
  },
  removeBookmark(_chapterId, id) {
    writeJson(
      BOOKMARK_KEY,
      readJson<Bookmark[]>(BOOKMARK_KEY, []).filter((b) => b.id !== id),
    );
    return Promise.resolve();
  },
};

/* --------------------------------------------------------------- switch */

let current: ProgressApi = USE_SERVER_PROGRESS ? httpProgress : localProgress;

/** The implementation the UI uses. Tests swap it with `setProgressApi`. */
export const progressApi: ProgressApi = {
  save: (p) => current.save(p),
  recent: (n) => current.recent(n),
  forSeries: (s) => current.forSeries(s),
  get: (c) => current.get(c),
  bookmarks: (c) => current.bookmarks(c),
  addBookmark: (c, i, n) => current.addBookmark(c, i, n),
  removeBookmark: (c, i) => current.removeBookmark(c, i),
};

export function setProgressApi(api: ProgressApi | null) {
  current = api ?? (USE_SERVER_PROGRESS ? httpProgress : localProgress);
}

/** "Chapter 36 · Page 2 of 6" */
export const progressLine = (i: Pick<ProgressInput, 'chapterNumber' | 'pageIndex' | 'pageCount'>) =>
  `Chapter ${i.chapterNumber} · Page ${i.pageIndex + 1} of ${i.pageCount}`;
