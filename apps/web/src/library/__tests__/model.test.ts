import { decodeBlurhash } from '../reader/blurhash';
import {
  arrowStep,
  buildSpreads,
  parsePageParam,
  percentAt,
  pickVariant,
  preloadWindow,
  spreadIndexOf,
  tapStep,
} from '../reader/model';
import { pickContinue, readMark } from '../continue';
import type { Chapter, PageEntry } from '../types';

const page = (i: number, o: Partial<PageEntry> = {}): PageEntry => ({
  index: i,
  width: 2000,
  height: 3000,
  is_double_spread: false,
  url: `full-${i}`,
  variants: [
    { width: 720, url: `v720-${i}` },
    { width: 1080, url: `v1080-${i}` },
    { width: 1600, url: `v1600-${i}` },
  ],
  ...o,
});

describe('pickVariant', () => {
  it('chooses the smallest variant that covers viewport x dpr', () => {
    expect(pickVariant(page(0), 700, 1)).toBe('v720-0');
    expect(pickVariant(page(0), 700, 2)).toBe('v1600-0');
    expect(pickVariant(page(0), 400, 2)).toBe('v1080-0'.replace('1080', '1080'));
  });
  it('falls back to the original when nothing is wide enough or the original is smaller', () => {
    expect(pickVariant(page(0), 1200, 2)).toBe('full-0');
    expect(pickVariant(page(0, { width: 700 }), 300, 1)).toBe('full-0');
  });
});

describe('preloadWindow', () => {
  it('lists the next three pages, clamped', () => {
    expect(preloadWindow(0, 10)).toEqual([1, 2, 3]);
    expect(preloadWindow(8, 10)).toEqual([9]);
    expect(preloadWindow(9, 10)).toEqual([]);
  });
});

describe('buildSpreads', () => {
  const pages = [page(0), page(1), page(2), page(3, { is_double_spread: true }), page(4), page(5)];
  it('keeps the cover alone and pairs the rest, leaving wide pages single', () => {
    expect(buildSpreads(pages, true)).toEqual([[0], [1, 2], [3], [4, 5]]);
  });
  it('pairs from the first page without the cover offset', () => {
    expect(buildSpreads(pages, false)).toEqual([[0, 1], [2], [3], [4, 5]]);
  });
  it('finds the spread of a page', () => {
    const s = buildSpreads(pages, true);
    expect(spreadIndexOf(s, 2)).toBe(1);
    expect(spreadIndexOf(s, 5)).toBe(3);
  });
});

describe('direction maths', () => {
  it('arrow keys follow the reading direction', () => {
    expect(arrowStep('ArrowLeft', 'rtl')).toBe(1);
    expect(arrowStep('ArrowRight', 'rtl')).toBe(-1);
    expect(arrowStep('ArrowRight', 'ltr')).toBe(1);
    expect(arrowStep('ArrowLeft', 'ltr')).toBe(-1);
  });
  it('tap zones are inverted for RTL and neutral in the middle', () => {
    expect(tapStep(10, 300, 'rtl')).toBe(1);
    expect(tapStep(290, 300, 'rtl')).toBe(-1);
    expect(tapStep(10, 300, 'ltr')).toBe(-1);
    expect(tapStep(290, 300, 'ltr')).toBe(1);
    expect(tapStep(150, 300, 'ltr')).toBe(0);
  });
  it('percent and page param', () => {
    expect(percentAt(0, 4)).toBe(25);
    expect(percentAt(3, 4)).toBe(100);
    expect(parsePageParam('2', 5)).toBe(1);
    expect(parsePageParam('9999', 5)).toBe(4);
    expect(parsePageParam('x', 5)).toBeNull();
    expect(parsePageParam(undefined, 5)).toBeNull();
  });
});

describe('blurhash', () => {
  it('decodes a valid hash to RGBA pixels and rejects junk', () => {
    const px = decodeBlurhash('LEHV6nWB2yk8pyo0adR*.7kCMdnj', 8, 8);
    expect(px).toHaveLength(8 * 8 * 4);
    expect(px![3]).toBe(255);
    expect(decodeBlurhash('short', 8, 8)).toBeNull();
    expect(decodeBlurhash('LEHV6nWB2yk8pyo0adR*.7kCMdn!', 8, 8)).toBeNull();
  });
});

const ch = (n: number, progress?: Chapter['progress']): Chapter => ({
  id: `c${n}`,
  series_id: 's',
  edition_id: 'e',
  number: n,
  page_count: 10,
  format: 'images',
  published: true,
  progress,
});

describe('pickContinue', () => {
  it('starts at the first chapter with no progress', () => {
    const t = pickContinue([ch(2), ch(1)], {});
    expect(t).toMatchObject({ mode: 'start', pageIndex: null });
    expect(t!.chapter.id).toBe('c1');
  });
  it('resumes a started chapter at its page', () => {
    const t = pickContinue(
      [
        ch(1, { page_index: 9, percent: 100, completed: true }),
        ch(2, { page_index: 3, percent: 40, completed: false }),
      ],
      {},
    );
    expect(t).toMatchObject({ mode: 'continue', pageIndex: 3 });
    expect(t!.chapter.id).toBe('c2');
  });
  it('moves on to the chapter after the last finished one', () => {
    const t = pickContinue([ch(1), ch(2), ch(3)], {
      c2: { page_index: 9, percent: 100, completed: true },
    });
    expect(t!.chapter.id).toBe('c3');
  });
  it('uses local progress for read marks', () => {
    expect(readMark(ch(1), {})).toBe('Unread');
    expect(readMark(ch(1), { c1: { page_index: 2, percent: 30, completed: false } })).toBe(
      'Page 3 of 10',
    );
    expect(readMark(ch(1, { page_index: 9, percent: 100, completed: true }), {})).toBe('Read');
  });
});
