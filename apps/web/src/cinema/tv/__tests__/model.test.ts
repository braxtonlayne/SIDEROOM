import { describe, expect, it } from 'vitest';
import {
  FILTERS,
  HOUR,
  MIN,
  atLiveEdge,
  behindLive,
  categoryOf,
  cellBox,
  filterChannels,
  formatDuration,
  hasDvr,
  kindYear,
  lineupName,
  neighbourCell,
  nowAndNext,
  programProgress,
  stepChannel,
} from '../model';
import { favouritesFrom } from '../favourites';
import { normaliseGuide } from '../api';
import { CHANNELS, SLOT0, block, guide } from './fixtures';

describe('guide geometry and time', () => {
  it('sizes cells by duration and clips at the track start', () => {
    const t0 = SLOT0;
    expect(cellBox({ start_ms: t0, end_ms: t0 + 60 * MIN }, t0, 100)).toEqual({
      left: 0,
      width: 200,
    });
    expect(cellBox({ start_ms: t0 - 30 * MIN, end_ms: t0 + 30 * MIN }, t0, 100)).toEqual({
      left: 0,
      width: 100,
    });
    expect(cellBox({ start_ms: t0 + 90 * MIN, end_ms: t0 + 120 * MIN }, t0, 100)).toEqual({
      left: 300,
      width: 100,
    });
  });

  it('finds now and next', () => {
    const g = guide();
    const { now, next } = nowAndNext(g.blocks.c101, g.now_ms);
    expect(now?.id).toBe('bebop');
    expect(next?.id).toBe('champloo');
    expect(nowAndNext([], 0)).toEqual({ now: null, next: null });
  });

  it('names the lineup by time of day', () => {
    expect(lineupName(new Date(2026, 2, 10, 20).getTime())).toBe('Evening lineup');
    expect(lineupName(new Date(2026, 2, 10, 8).getTime())).toBe('Morning lineup');
    expect(lineupName(new Date(2026, 2, 10, 14).getTime())).toBe('Afternoon lineup');
    expect(lineupName(new Date(2026, 2, 10, 23).getTime())).toBe('Late-night lineup');
  });

  it('formats duration and kind lines', () => {
    expect(formatDuration(30)).toBe('30m');
    expect(formatDuration(120)).toBe('2h');
    expect(formatDuration(95)).toBe('1h 35m');
    expect(kindYear({ kind: 'anime', year: 1998 })).toBe('Anime · 1998');
    expect(kindYear({ kind: 'movie', year: null })).toBe('Movie');
  });
});

describe('filters', () => {
  it('uses the server category, else the dominant programme kind', () => {
    const g = guide();
    expect(categoryOf(CHANNELS[0]!)).toBe('anime');
    const bare = { ...CHANNELS[0]!, category: null };
    expect(categoryOf(bare, g.blocks.c101)).toBe('anime');
    expect(categoryOf(bare, [])).toBe('custom');
    expect(categoryOf(bare, [block('x', 0, 30, { kind: 'movie' })])).toBe('movies');
  });

  it('filters and sorts by channel number', () => {
    const g = guide();
    const fav = new Set(['c105']);
    expect(
      filterChannels([...g.channels].reverse(), g.blocks, 'all', fav).map((c) => c.number),
    ).toEqual([101, 102, 105]);
    expect(filterChannels(g.channels, g.blocks, 'movies', fav).map((c) => c.id)).toEqual(['c105']);
    expect(filterChannels(g.channels, g.blocks, 'favorites', fav).map((c) => c.id)).toEqual([
      'c105',
    ]);
    expect(FILTERS.map((f) => f.label)).toEqual([
      'All',
      'Anime & animation',
      'Movies',
      'Series',
      'Custom',
      'Favorites',
    ]);
  });
});

describe('keyboard navigation', () => {
  const g = guide();
  const rows = [g.blocks.c101!, g.blocks.c102!, g.blocks.c105!];
  it('walks a row and stops at the ends', () => {
    expect(neighbourCell(rows, { row: 0, id: 'bebop' }, 'ArrowRight')).toEqual({
      row: 0,
      id: 'champloo',
    });
    expect(neighbourCell(rows, { row: 0, id: 'bebop' }, 'ArrowLeft')?.id).toBe(rows[0]![0]!.id);
    expect(neighbourCell(rows, { row: 0, id: rows[0]![0]!.id }, 'ArrowLeft')).toBeNull();
    expect(neighbourCell(rows, { row: 0, id: 'bebop' }, 'End')?.id).toBe('bebop2');
    expect(neighbourCell(rows, { row: 0, id: 'bebop' }, 'Home')?.id).toBe(rows[0]![0]!.id);
  });
  it('moves vertically to the block covering the same start time', () => {
    expect(neighbourCell(rows, { row: 0, id: 'bebop' }, 'ArrowDown')).toEqual({
      row: 1,
      id: 'dexter',
    });
    expect(neighbourCell(rows, { row: 0, id: 'champloo' }, 'ArrowDown')).toEqual({
      row: 1,
      id: 'dexter',
    });
    expect(neighbourCell(rows, { row: 1, id: 'batman' }, 'ArrowDown')).toEqual({
      row: 2,
      id: 'alien',
    });
    expect(neighbourCell(rows, { row: 2, id: 'alien' }, 'ArrowDown')).toBeNull();
    expect(neighbourCell(rows, { row: 0, id: 'bebop' }, 'ArrowUp')).toBeNull();
  });
  it('skips empty rows and picks the closest block when none covers', () => {
    const sparse = [[block('a', 0, 30, { id: 'a1' })], [], [block('c', 120, 30, { id: 'c1' })]];
    expect(neighbourCell(sparse, { row: 0, id: 'a1' }, 'ArrowDown')).toEqual({ row: 2, id: 'c1' });
  });
});

describe('live helpers', () => {
  it('steps channels with wrap-around in number order', () => {
    expect(stepChannel(CHANNELS, 'c101', 1)?.id).toBe('c102');
    expect(stepChannel(CHANNELS, 'c105', 1)?.id).toBe('c101');
    expect(stepChannel(CHANNELS, 'c101', -1)?.id).toBe('c105');
    expect(stepChannel(CHANNELS, 'gone', 1)?.id).toBe('c101');
    expect(stepChannel([], 'x', 1)).toBeNull();
  });
  it('computes programme progress', () => {
    const b = { start_ms: 0, end_ms: 30 * MIN };
    expect(programProgress(b, 15 * MIN)).toBe(0.5);
    expect(programProgress(b, -5)).toBe(0);
    expect(programProgress(b, HOUR)).toBe(1);
    expect(programProgress(null, 5)).toBe(0);
  });
  it('offers a DVR window only when the playlist keeps one', () => {
    expect(hasDvr(null)).toBe(false);
    expect(hasDvr({ start: 100, end: 136 })).toBe(false);
    expect(hasDvr({ start: 100, end: 700 })).toBe(true);
    expect(behindLive({ start: 0, end: 100 }, 70)).toBe(30);
    expect(atLiveEdge({ start: 0, end: 100 }, 95)).toBe(true);
    expect(atLiveEdge({ start: 0, end: 100 }, 70)).toBe(false);
    expect(atLiveEdge(null, 0)).toBe(true);
  });
});

describe('data layer', () => {
  it('reads favourites from the nested or flat settings key and ignores junk', () => {
    expect(favouritesFrom({ cinema: { tv: { favourites: ['a', 3, 'b'] } } })).toEqual(['a', 'b']);
    expect(favouritesFrom({ 'cinema.tv.favourites': ['x'] })).toEqual(['x']);
    expect(favouritesFrom({ cinema: 'nope' })).toEqual([]);
    expect(favouritesFrom(undefined)).toEqual([]);
  });
  it('accepts blocks grouped per channel or flat and sorts them', () => {
    const a = block('c1', 30, 30, { id: 'a' });
    const b = block('c1', 0, 30, { id: 'b' });
    const flat = normaliseGuide({ channels: [], blocks: [a, b] }, 5);
    expect(flat.blocks.c1!.map((x) => x.id)).toEqual(['b', 'a']);
    expect(flat.now_ms).toBe(5);
    const grouped = normaliseGuide({ channels: [], blocks: { c1: [a, b] }, now_ms: 9 });
    expect(grouped.blocks.c1!.map((x) => x.id)).toEqual(['b', 'a']);
    expect(grouped.now_ms).toBe(9);
  });
});
