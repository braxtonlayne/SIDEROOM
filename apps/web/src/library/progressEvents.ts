// The reader's other devices (D-690, LIB-RDR-018): `SR_LIBRARY_UPDATE` with `type: 'progress'` says another session of
// this account saved a position (docs/05-api-and-gateway.md section 4: chapter_id, series_id, page_index, percent, cfi,
// completed, updated_at_ms). The frame does not say which session saved, so a reader tells its own echo apart by
// what it sent (see `useProgressSync`).
//
// Two jobs, both only while the Library is mounted (LibraryRoot starts this; the Lounge never loads it):
//   - the cached data behind "continue reading" and the read marks is refreshed, so the Library home shows the
//     phone's progress without a reload;
//   - a reader that has the chapter open hears about it (`onRemoteProgress`).
import type { QueryClient } from '@tanstack/react-query';
import { onDispatch } from '../store/events';
import type { ChapterProgress } from './types';

export interface RemoteProgress extends ChapterProgress {
  chapter_id: string;
  series_id: string;
}

type Listener = (p: RemoteProgress) => void;
const listeners = new Map<string, Set<Listener>>();

/** Hear a position another session saved for `chapterId`. Returns the unsubscribe function. */
export function onRemoteProgress(chapterId: string, fn: Listener): () => void {
  let set = listeners.get(chapterId);
  if (!set) listeners.set(chapterId, (set = new Set()));
  set.add(fn);
  return () => {
    set.delete(fn);
    if (set.size === 0) listeners.delete(chapterId);
  };
}

const str = (v: unknown): string | null =>
  typeof v === 'string' ? v : typeof v === 'number' ? String(v) : null;
const num = (v: unknown, fallback = 0): number => (typeof v === 'number' ? v : fallback);

/** Read a `progress` frame; null when it lacks what a position needs. */
export function parseProgressEvent(data: unknown): RemoteProgress | null {
  const d = data as Record<string, unknown> | null;
  if (!d || d.type !== 'progress') return null;
  const chapter_id = str(d.chapter_id);
  const series_id = str(d.series_id);
  if (!chapter_id || !series_id) return null;
  return {
    chapter_id,
    series_id,
    page_index: num(d.page_index),
    percent: num(d.percent),
    completed: d.completed === true,
    cfi: typeof d.cfi === 'string' ? d.cfi : null,
    updated_at_ms: num(d.updated_at_ms),
  };
}

/** Start listening; returns the stop function. One listener per Library mount. */
export function startProgressSync(qc: QueryClient): () => void {
  return onDispatch((event, data) => {
    if (event !== 'SR_LIBRARY_UPDATE') return;
    const type = (data as { type?: unknown } | null)?.type;
    if (type === 'progress_cleared' || type === 'read_marks') {
      void qc.invalidateQueries({ queryKey: ['library', 'recent'] });
      void qc.invalidateQueries({ queryKey: ['library', 'unread'] });
      const sid = str((data as { series_id?: unknown }).series_id);
      void qc.invalidateQueries({
        queryKey: sid ? ['library', 'chapters', sid] : ['library', 'chapters'],
      });
      void qc.invalidateQueries({
        queryKey: sid ? ['library', 'series-progress', sid] : ['library', 'series-progress'],
      });
      return;
    }
    const p = parseProgressEvent(data);
    if (!p) return;
    void qc.invalidateQueries({ queryKey: ['library', 'recent'] });
    void qc.invalidateQueries({ queryKey: ['library', 'unread'] });
    void qc.invalidateQueries({ queryKey: ['library', 'chapters', p.series_id] });
    void qc.invalidateQueries({ queryKey: ['library', 'series-progress', p.series_id] });
    for (const fn of [...(listeners.get(p.chapter_id) ?? [])]) fn(p);
  });
}
