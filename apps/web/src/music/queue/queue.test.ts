import { describe, expect, it } from 'vitest';
import {
  albumOrder,
  byAlbum,
  byArtist,
  byGenre,
  dedupe,
  moreLikeThis,
  moveTrack,
  nextCycle,
  seededRng,
  shuffleUpcoming,
  shuffled,
  similarity,
  totalSeconds,
  type QueueTrack,
} from './queue';

const t = (id: string, o: Partial<QueueTrack> = {}): QueueTrack => ({ id, title: `T${id}`, ...o });

const lib: QueueTrack[] = [
  t('a3', { album: 'Alpha', artist: 'Ann', trackNo: 3, genres: ['Rock'], year: 1999 }),
  t('a1', { album: 'Alpha', artist: 'Ann', trackNo: 1, genres: ['Rock'], year: 1999 }),
  t('a2', { album: 'Alpha', artist: 'Ann', trackNo: 2, genres: ['rock', 'Indie'], year: 1999 }),
  t('b1', { album: 'Beta', artist: 'Ann', trackNo: 1, genres: ['Pop'], year: 2004 }),
  t('c1', { album: 'Alpha', artist: 'Cy', trackNo: 1, genres: ['Jazz'], year: 1960 }),
  t('d1', { album: 'Delta', artist: 'Di', albumArtist: 'Ann', trackNo: 1, genres: ['Pop'] }),
  t('e1', { artist: 'Ed', genres: ['Rock'], tags: ['night', 'drive'], year: 2001 }),
  t('f1', { artist: 'Flo', genres: ['Pop'], tags: ['Night'], year: 2003 }),
];
const by = (id: string) => lib.find((x) => x.id === id)!;

describe('album, artist and genre queues', () => {
  it('orders an album by disc then track, starting at the seed', () => {
    const r = byAlbum(lib, by('a2'));
    expect(r.tracks.map((x) => x.id)).toEqual(['a1', 'a2', 'a3']);
    expect(r.startAt).toBe(1);
  });

  it('does not mix two artists who share an album name', () => {
    const r = byAlbum(lib, by('c1'));
    expect(r.tracks.map((x) => x.id)).toEqual(['c1']);
  });

  it('falls back to the seed alone when it has no album', () => {
    expect(byAlbum(lib, by('e1'))).toEqual({ tracks: [by('e1')], startAt: 0 });
  });

  it('puts tracks without numbers after numbered ones and sorts discs first', () => {
    const x = [
      t('z', { title: 'Z' }),
      t('d2', { discNo: 2, trackNo: 1 }),
      t('d1', { discNo: 1, trackNo: 5 }),
    ];
    expect(x.sort(albumOrder).map((y) => y.id)).toEqual(['d1', 'z', 'd2']);
  });

  it('takes everything by the artist, oldest album first, album artist counted', () => {
    const r = byArtist(lib, by('a1'));
    expect(r.tracks.map((x) => x.id)).toEqual(['a1', 'a2', 'a3', 'b1', 'd1']);
    expect(r.tracks[r.startAt]!.id).toBe('a1');
  });

  it('takes a genre case-insensitively, any shared genre', () => {
    const r = byGenre(lib, by('a2'));
    expect(r.tracks.map((x) => x.id).sort()).toEqual(['a1', 'a2', 'a3', 'e1']);
    expect(byGenre(lib, t('x', { genres: [] })).tracks.map((x) => x.id)).toEqual(['x']);
  });
});

describe('more like this', () => {
  it('scores tags above genre above artist, symmetric, zero for strangers', () => {
    // A shared tag (+ era) beats a shared genre (+ era) alone.
    expect(similarity(by('e1'), by('f1'))).toBeGreaterThan(similarity(by('e1'), by('a1')));
    expect(similarity(by('e1'), by('f1'))).toBe(similarity(by('f1'), by('e1')));
    expect(similarity(by('c1'), by('f1'))).toBe(0);
  });

  it('starts with the seed, ranks by likeness, drops strangers and keeps tie order', () => {
    const r = moreLikeThis(lib, by('e1'));
    expect(r[0]!.id).toBe('e1');
    // f1 shares the "night" tag (case-insensitively); a-tracks share the Rock genre.
    expect(r.map((x) => x.id)).toEqual(['e1', 'f1', 'a3', 'a1', 'a2']);
    expect(r.some((x) => x.id === 'c1')).toBe(false);
  });

  it('honours the limit and never repeats the seed', () => {
    const r = moreLikeThis(lib, by('a1'), 3);
    expect(r).toHaveLength(3);
    expect(r.filter((x) => x.id === 'a1')).toHaveLength(1);
  });
});

describe('shuffle with no repeat', () => {
  const ids = (l: QueueTrack[]) => l.map((x) => x.id);

  it('is a permutation: every track exactly once', () => {
    for (let seed = 1; seed <= 20; seed++) {
      const out = shuffled(lib, seededRng(seed));
      expect(ids(out).sort()).toEqual(ids(lib).sort());
    }
  });

  it('is reproducible with a seeded generator and does not touch its input', () => {
    const before = ids(lib);
    expect(ids(shuffled(lib, seededRng(7)))).toEqual(ids(shuffled(lib, seededRng(7))));
    expect(ids(lib)).toEqual(before);
  });

  it('puts the chosen first track first', () => {
    for (let seed = 1; seed <= 10; seed++) {
      expect(shuffled(lib, seededRng(seed), { first: 'b1' })[0]!.id).toBe('b1');
    }
  });

  it('keeps played tracks and the current one in place when shuffling the rest', () => {
    const q = { tracks: lib.slice(0, 6), index: 2 };
    const out = shuffleUpcoming(q, seededRng(3));
    expect(ids(out.tracks.slice(0, 3))).toEqual(ids(q.tracks.slice(0, 3)));
    expect(out.index).toBe(2);
    expect(ids(out.tracks).sort()).toEqual(ids(q.tracks).sort());
  });

  it('opens the next cycle with a different track than the one that just played', () => {
    for (let seed = 1; seed <= 60; seed++) {
      const out = nextCycle(lib, 'a1', seededRng(seed));
      expect(out[0]!.id).not.toBe('a1');
      expect(ids(out).sort()).toEqual(ids(lib).sort());
    }
  });

  it('handles one track and none', () => {
    expect(shuffled([], seededRng(1))).toEqual([]);
    expect(nextCycle([by('a1')], 'a1', seededRng(1)).map((x) => x.id)).toEqual(['a1']);
  });
});

describe('queue edits', () => {
  const q = { tracks: ['a', 'b', 'c', 'd', 'e'], index: 2 };

  it('the current index follows the playing track', () => {
    expect(moveTrack(q, 2, 4)).toEqual({ tracks: ['a', 'b', 'd', 'e', 'c'], index: 4 });
    expect(moveTrack(q, 0, 3)).toEqual({ tracks: ['b', 'c', 'd', 'a', 'e'], index: 1 });
    expect(moveTrack(q, 4, 0)).toEqual({ tracks: ['e', 'a', 'b', 'c', 'd'], index: 3 });
    expect(moveTrack(q, 0, 1)).toEqual({ tracks: ['b', 'a', 'c', 'd', 'e'], index: 2 });
  });

  it('ignores moves outside the queue', () => {
    expect(moveTrack(q, -1, 2)).toBe(q);
    expect(moveTrack(q, 1, 9)).toBe(q);
    expect(moveTrack(q, 3, 3)).toBe(q);
  });

  it('dedupes by id keeping the first and sums known durations', () => {
    expect(dedupe([by('a1'), by('a2'), by('a1')]).map((x) => x.id)).toEqual(['a1', 'a2']);
    expect(totalSeconds([{ durationMs: 1500 }, {}, { durationMs: 500 }])).toBe(2);
  });
});
