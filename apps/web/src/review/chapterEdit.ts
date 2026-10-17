// Pure helpers of the submission editor (REV-ED-001, REV-ED-004): reordering chapters by renumbering and the
// checks shown in the validation summary. No React, no network.
import type { DraftChapter } from './types';

/** The body of `PUT .../library/chapters/{id}`: what a rename or renumber sends. */
export interface ChapterUpdateBody {
  number: number;
  title: string | null;
  format: string;
  release_group: string | null;
  volume_id: string | null;
}

/**
 * A chapter update that keeps everything the person did not change (K-07): the chapter's current volume,
 * format and release group go back with the new number and title instead of a bare `{number, title}`. The
 * source file is the upload's and the server keeps it.
 */
export function chapterUpdateBody(
  c: DraftChapter,
  patch: { number?: number; title?: string | null; volume_id?: string | null } = {},
): ChapterUpdateBody {
  return {
    number: patch.number ?? c.number,
    title: patch.title === undefined ? c.title : patch.title,
    format: c.format,
    release_group: c.release_group,
    volume_id: patch.volume_id === undefined ? (c.volume_id ?? null) : patch.volume_id,
  };
}

export interface Renumber {
  id: string;
  number: number;
}

/**
 * Move the chapter at `from` to position `to` in number order. The chapters keep the same set of numbers: the
 * moved chapter takes the number of the slot it lands in and the ones in between shift by one slot. Returns only
 * the chapters whose number changes (empty when nothing moves). Input must be sorted by number.
 */
export function reorderNumbers(sorted: DraftChapter[], from: number, to: number): Renumber[] {
  if (from === to || from < 0 || to < 0 || from >= sorted.length || to >= sorted.length) return [];
  const slots = sorted.map((c) => c.number);
  const order = sorted.slice();
  const [moved] = order.splice(from, 1);
  order.splice(to, 0, moved!);
  const out: Renumber[] = [];
  order.forEach((c, i) => {
    if (c.number !== slots[i]) out.push({ id: c.id, number: slots[i]! });
  });
  return out;
}

/** A number no chapter of the list uses, for the first step of a swap. */
export const spareNumber = (chapters: DraftChapter[]): number =>
  Math.floor(Math.max(0, ...chapters.map((c) => c.number))) + 1;

export type IssueLevel = 'error' | 'warning';

export interface Issue {
  level: IssueLevel;
  /** Catalog key (see `review.validation.*`) and its variables. */
  key: 'noTitle' | 'noChapters' | 'duplicateNumber' | 'noFile' | 'gap' | 'noSynopsis';
  vars?: Record<string, string | number>;
}

export interface CheckInput {
  title: string;
  /** Series kinds describe the series; a chapters-only submission leaves that to the series. */
  describesSeries: boolean;
  synopsis?: string | null;
  chapters: DraftChapter[];
}

/** The problems a person can fix before sending. Errors would be refused or are surely a mistake; warnings are hints. */
export function checkSubmission(input: CheckInput): Issue[] {
  const out: Issue[] = [];
  if (!input.title.trim()) out.push({ level: 'error', key: 'noTitle' });
  if (input.chapters.length === 0) out.push({ level: 'error', key: 'noChapters' });
  const seen = new Map<number, number>();
  for (const c of input.chapters) seen.set(c.number, (seen.get(c.number) ?? 0) + 1);
  for (const [n, count] of seen)
    if (count > 1) out.push({ level: 'error', key: 'duplicateNumber', vars: { n } });
  const missing = input.chapters.filter((c) => c.page_count === 0).length;
  if (missing > 0) out.push({ level: 'warning', key: 'noFile', vars: { count: missing } });
  // Whole-number gaps only: decimals (4.5) are side chapters, not holes.
  const whole = [...new Set(input.chapters.map((c) => c.number).filter(Number.isInteger))].sort(
    (a, b) => a - b,
  );
  const holes: number[] = [];
  for (let i = 1; i < whole.length; i++)
    for (let n = whole[i - 1]! + 1; n < whole[i]! && holes.length < 20; n++) holes.push(n);
  if (holes.length > 0)
    out.push({
      level: 'warning',
      key: 'gap',
      vars: { list: holes.slice(0, 5).join(', ') + (holes.length > 5 ? ', …' : '') },
    });
  if (input.describesSeries && !(input.synopsis ?? '').trim())
    out.push({ level: 'warning', key: 'noSynopsis' });
  return out;
}
