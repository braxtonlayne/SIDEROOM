import { describe, expect, it, vi } from 'vitest';
import { seededRng } from './queue';
import { PAGE, SCAN_LIMIT, buildSmartQueue, scan, type Fetcher, type LibraryQuery } from './smart';
import type { LibraryTrack } from './types';

const trk = (id: string, o: Partial<LibraryTrack> = {}): LibraryTrack => ({
  id,
  mountId: 'm1',
  title: `T${id}`,
  artist: '',
  album: '',
  ...o,
});

const LIB: LibraryTrack[] = [
  trk('1', { album: 'Alpha', artist: 'Ann', trackNo: 2, genres: ['Rock'], tags: ['night'] }),
  trk('2', { album: 'Alpha', artist: 'Ann', trackNo: 1, genres: ['Rock'] }),
  trk('3', { album: 'Beta', artist: 'Ann', trackNo: 1, genres: ['Pop'] }),
  trk('4', { album: 'Gamma', artist: 'Bo', trackNo: 1, genres: ['Rock'], tags: ['night'] }),
  trk('5', { album: 'Delta', artist: 'Cy', trackNo: 1, genres: ['Jazz'] }),
];

/** A fetcher over LIB that honours the server's exact album/artist filters and paging. */
function server(all: LibraryTrack[] = LIB) {
  const calls: LibraryQuery[] = [];
  const fetch: Fetcher = async (q) => {
    calls.push(q);
    let rows = all;
    if (q.album) rows = rows.filter((t) => t.album === q.album);
    if (q.artist) rows = rows.filter((t) => t.artist === q.artist || t.albumArtist === q.artist);
    if (q.mount) rows = rows.filter((t) => t.mountId === q.mount);
    const off = q.offset ?? 0;
    return { items: rows.slice(off, off + (q.limit ?? PAGE)), total: rows.length };
  };
  return { fetch, calls };
}

describe('smart queue builders', () => {
  it('album: asks the server for that album only and orders it', async () => {
    const s = server();
    const r = await buildSmartQueue('album', LIB[0]!, s.fetch);
    expect(r.tracks.map((t) => t.id)).toEqual(['2', '1']);
    expect(r.startAt).toBe(1);
    expect(s.calls).toEqual([
      { album: 'Alpha', artist: 'Ann', mount: 'm1', limit: PAGE, offset: 0 },
    ]);
  });

  it('artist: everything by them, oldest album first, starting at the seed', async () => {
    const r = await buildSmartQueue('artist', LIB[2]!, server().fetch);
    expect(r.tracks.map((t) => t.id)).toEqual(['2', '1', '3']);
    expect(r.tracks[r.startAt]!.id).toBe('3');
  });

  it('genre: scans the library and keeps the genre', async () => {
    const r = await buildSmartQueue('genre', LIB[3]!, server().fetch);
    expect(r.tracks.map((t) => t.id).sort()).toEqual(['1', '2', '4']);
  });

  it('similar: seed first, then likeness order, strangers left out', async () => {
    const r = await buildSmartQueue('similar', LIB[0]!, server().fetch);
    expect(r.tracks[0]!.id).toBe('1');
    // 2 shares genre + artist; 4 shares tag + genre; 3 shares artist only; 5 shares nothing.
    expect(r.tracks.map((t) => t.id)).toEqual(['1', '4', '2', '3']);
  });

  it('shuffle: a permutation of the same tracks that opens with the seed', async () => {
    const r = await buildSmartQueue('artist', LIB[2]!, server().fetch, {
      shuffle: true,
      rng: seededRng(5),
    });
    expect(r.startAt).toBe(0);
    expect(r.tracks[0]!.id).toBe('3');
    expect(r.tracks.map((t) => t.id).sort()).toEqual(['1', '2', '3']);
  });

  it('a seed with nothing to group on plays alone and asks nothing', async () => {
    const s = server();
    const bare = trk('9');
    for (const k of ['album', 'artist', 'genre'] as const) {
      expect(await buildSmartQueue(k, bare, s.fetch)).toEqual({ tracks: [bare], startAt: 0 });
    }
    expect(s.calls).toEqual([]);
  });

  it('keeps the seed even when the server page lacks it', async () => {
    const hole = server(LIB.filter((t) => t.id !== '3'));
    const r = await buildSmartQueue('artist', LIB[2]!, hole.fetch);
    expect(r.tracks.some((t) => t.id === '3')).toBe(true);
  });
});

describe('scan', () => {
  it('pages until the total and stops at the limit', async () => {
    const many = Array.from({ length: 450 }, (_, i) => trk(String(i)));
    const f = vi.fn(server(many).fetch);
    const rows = await scan(f);
    expect(rows).toHaveLength(450);
    expect(f).toHaveBeenCalledTimes(3);
    const huge = Array.from({ length: SCAN_LIMIT + 300 }, (_, i) => trk(String(i)));
    expect(await scan(server(huge).fetch)).toHaveLength(SCAN_LIMIT);
  });

  it('stops on a short page', async () => {
    const f = vi.fn(server(LIB).fetch);
    expect(await scan(f)).toHaveLength(5);
    expect(f).toHaveBeenCalledTimes(1);
  });
});
