import { checkSubmission, reorderNumbers, spareNumber } from '../chapterEdit';
import type { DraftChapter } from '../types';

const ch = (id: string, number: number, extra: Partial<DraftChapter> = {}): DraftChapter => ({
  id,
  series_id: 's',
  edition_id: 'e',
  number,
  title: null,
  page_count: 10,
  format: 'images',
  release_group: null,
  published: false,
  ...extra,
});

describe('reorderNumbers', () => {
  const list = [ch('a', 1), ch('b', 2), ch('c', 3), ch('d', 4)];
  it('moves a chapter down and shifts the ones in between', () => {
    expect(reorderNumbers(list, 0, 2)).toEqual([
      { id: 'b', number: 1 },
      { id: 'c', number: 2 },
      { id: 'a', number: 3 },
    ]);
  });
  it('moves a chapter up', () => {
    expect(reorderNumbers(list, 3, 2)).toEqual([
      { id: 'd', number: 3 },
      { id: 'c', number: 4 },
    ]);
  });
  it('keeps the set of numbers, decimals included', () => {
    const l = [ch('a', 1), ch('b', 1.5), ch('c', 2)];
    const out = reorderNumbers(l, 0, 1);
    expect(out).toEqual([
      { id: 'b', number: 1 },
      { id: 'a', number: 1.5 },
    ]);
  });
  it('does nothing for no move or out-of-range indexes', () => {
    expect(reorderNumbers(list, 1, 1)).toEqual([]);
    expect(reorderNumbers(list, -1, 2)).toEqual([]);
    expect(reorderNumbers(list, 0, 9)).toEqual([]);
  });
  it('finds a spare number above every chapter', () => {
    expect(spareNumber(list)).toBe(5);
    expect(spareNumber([])).toBe(1);
    expect(spareNumber([ch('a', 2.5)])).toBe(3);
  });
});

describe('checkSubmission', () => {
  const base = { title: 'Series', describesSeries: true, synopsis: 'A story.' };
  const keys = (i: ReturnType<typeof checkSubmission>) => i.map((x) => `${x.level}:${x.key}`);

  it('is clean for a complete submission', () => {
    expect(checkSubmission({ ...base, chapters: [ch('a', 1), ch('b', 2)] })).toEqual([]);
  });
  it('errors on a missing title, no chapters and duplicate numbers', () => {
    expect(keys(checkSubmission({ ...base, title: ' ', chapters: [] }))).toEqual([
      'error:noTitle',
      'error:noChapters',
    ]);
    expect(keys(checkSubmission({ ...base, chapters: [ch('a', 1), ch('b', 1)] }))).toEqual([
      'error:duplicateNumber',
    ]);
  });
  it('warns about chapters without pages, whole-number gaps and a missing synopsis', () => {
    const out = checkSubmission({
      ...base,
      synopsis: '',
      chapters: [ch('a', 1), ch('b', 2.5), ch('c', 5, { page_count: 0 })],
    });
    expect(keys(out)).toEqual(['warning:noFile', 'warning:gap', 'warning:noSynopsis']);
    expect(out[0]!.vars).toEqual({ count: 1 });
    // 2.5 is a side chapter: only 2, 3 and 4 are holes between 1 and 5.
    expect(out[1]!.vars).toEqual({ list: '2, 3, 4' });
  });
  it('does not ask a chapters-only submission for a synopsis', () => {
    expect(
      checkSubmission({ ...base, describesSeries: false, synopsis: null, chapters: [ch('a', 1)] }),
    ).toEqual([]);
  });
});
