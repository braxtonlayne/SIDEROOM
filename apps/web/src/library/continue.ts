// Which chapter "Start reading" / "Continue reading" opens, and how a chapter row's read mark reads.
import type { Chapter, ChapterProgress } from './types';

export type ProgressMap = Record<string, ChapterProgress | undefined>;

/** A chapter's progress: the server's (on the chapter) wins over the local one. */
export const progressOf = (c: Chapter, local: ProgressMap): ChapterProgress | null =>
  c.progress ?? local[c.id] ?? null;

export interface ContinueTarget {
  chapter: Chapter;
  /** 0-based page to resume on; null starts at the top. */
  pageIndex: number | null;
  mode: 'start' | 'continue';
}

/**
 * Resume the furthest chapter that is started but not finished; else the chapter after the furthest finished
 * one; else the first chapter. `null` when the edition has no chapters.
 */
export function pickContinue(
  chapters: readonly Chapter[],
  local: ProgressMap,
): ContinueTarget | null {
  const sorted = [...chapters].sort((a, b) => a.number - b.number);
  if (sorted.length === 0) return null;
  let lastDone = -1;
  let partial: { i: number; p: ChapterProgress } | null = null;
  sorted.forEach((c, i) => {
    const p = progressOf(c, local);
    if (!p) return;
    if (p.completed) lastDone = i;
    else if (!partial || i > partial.i) partial = { i, p };
  });
  const part = partial as { i: number; p: ChapterProgress } | null;
  if (part && part.i > lastDone)
    return { chapter: sorted[part.i]!, pageIndex: part.p.page_index, mode: 'continue' };
  if (lastDone >= 0) {
    const next = sorted[lastDone + 1];
    if (next) return { chapter: next, pageIndex: null, mode: 'continue' };
    return { chapter: sorted[lastDone]!, pageIndex: null, mode: 'continue' };
  }
  return { chapter: sorted[0]!, pageIndex: null, mode: 'start' };
}

/** Text of a chapter row's read mark. */
export function readMark(c: Chapter, local: ProgressMap): string {
  const p = progressOf(c, local);
  if (!p) return 'Unread';
  if (p.completed) return 'Read';
  return `Page ${p.page_index + 1} of ${c.page_count}`;
}
