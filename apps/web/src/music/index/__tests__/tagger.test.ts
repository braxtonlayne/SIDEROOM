import { describe, expect, it, vi } from 'vitest';
import { HttpError } from '../backoff';
import { patchFrom, Tagger } from '../tagger';
import type { MusicTags, TagPatch, TaggerApi, TrackRow } from '../types';

const tags = (over: Partial<MusicTags> = {}): MusicTags => ({
  title: 'T',
  artist: 'A',
  album: 'Al',
  albumArtist: null,
  track: 3,
  disc: null,
  year: 1999,
  genre: 'Rock; Pop',
  durationHint: 1234.4,
  cover: null,
  ...over,
});

const row = (n: number): TrackRow => ({
  id: String(n),
  mount_id: '10',
  key: `m/${n}.mp3`,
  size: 2000,
  container: 'mp3',
  tags_state: 'pending',
});

/** A fake server: tracks leave the pending list once patched with read or none. */
function fakeApi(rows: TrackRow[]) {
  const patches: Array<{ id: string; body: TagPatch }> = [];
  const uploads: string[] = [];
  const state = new Map<string, string>(rows.map((r) => [r.id, r.tags_state]));
  const api: TaggerApi = {
    async listPending(_m, limit, offset) {
      const pending = rows.filter((r) => ['pending', 'failed'].includes(state.get(r.id)!));
      return { items: pending.slice(offset, offset + limit), total: pending.length };
    },
    async presign(id) {
      return { url: `https://bucket/${id}` };
    },
    async patch(id, body) {
      patches.push({ id, body });
      state.set(id, body.tags_state);
    },
    rescan: vi.fn(async () => {
      for (const r of rows) state.set(r.id, 'pending');
      return { queued: rows.length };
    }),
    async uploadCover(_m, key) {
      uploads.push(key);
    },
  };
  return { api, patches, uploads, state };
}

const okFetch = async () => new Response(new Uint8Array(100), { status: 206 });
const instant = { sleep: async () => undefined, random: () => 1 };
const plainReader = async () => ({ readTags: () => tags(), readTagsTail: () => tags() });

describe('patchFrom', () => {
  it('sends only what the file has, splits genres and rounds the duration', () => {
    expect(patchFrom(tags(), 'covers/aa.jpg')).toEqual({
      source: 'file',
      tags_state: 'read',
      title: 'T',
      artist: 'A',
      album: 'Al',
      track_no: 3,
      year: 1999,
      duration_ms: 1234,
      genres: ['Rock', 'Pop'],
      cover_key: 'covers/aa.jpg',
    });
    expect(
      patchFrom(
        tags({
          title: null,
          artist: null,
          album: null,
          track: null,
          year: 99999,
          genre: null,
          durationHint: null,
        }),
        null,
      ),
    ).toEqual({ source: 'file', tags_state: 'read' });
  });
});

describe('Tagger', () => {
  it('tags 10 tracks, 4 at a time, and ends with nothing pending', async () => {
    const s = fakeApi(Array.from({ length: 10 }, (_, i) => row(i + 1)));
    let inflight = 0;
    let peak = 0;
    const gates: Array<() => void> = [];
    const tagger = new Tagger({
      api: s.api,
      reader: plainReader,
      fetchFn: async () => {
        inflight++;
        peak = Math.max(peak, inflight);
        await new Promise<void>((r) => gates.push(r));
        inflight--;
        return okFetch();
      },
      backoff: instant,
    });
    const run = tagger.start('10');
    for (let i = 0; i < 400 && s.patches.length < 10; i++) {
      await new Promise((r) => setTimeout(r, 0));
      while (gates.length) gates.shift()!();
    }
    const done = await run;
    expect(peak).toBe(4);
    expect(done).toMatchObject({ total: 10, done: 10, tagged: 10, failed: 0, running: false });
    expect(s.patches).toHaveLength(10);
    expect(s.patches.every((p) => p.body.source === 'file' && p.body.tags_state === 'read')).toBe(
      true,
    );
  });

  it('retries a transient patch failure with backoff and marks a permanent failure failed', async () => {
    const s = fakeApi([row(1), row(2)]);
    const sleeps: number[] = [];
    let failedOnce = 0;
    const realPatch = s.api.patch;
    s.api.patch = async (id, body) => {
      if (id === '1' && body.tags_state === 'read' && failedOnce++ < 2) throw new HttpError(503);
      return realPatch(id, body);
    };
    const tagger = new Tagger({
      api: s.api,
      reader: plainReader,
      fetchFn: async (u) => (u.endsWith('/2') ? new Response(null, { status: 404 }) : okFetch()),
      backoff: { sleep: async (ms) => void sleeps.push(ms), random: () => 1, baseMs: 100 },
      warn: () => undefined,
    });
    const done = await tagger.start();
    expect(sleeps).toEqual([100, 200]);
    expect(done).toMatchObject({ total: 2, done: 2, tagged: 1, failed: 1 });
    expect(s.state.get('1')).toBe('read');
    expect(s.state.get('2')).toBe('failed');
  });

  it('does not loop on tracks that stay failed', async () => {
    const s = fakeApi([row(1), row(2), row(3)]);
    const fetchFn = vi.fn(async () => new Response(null, { status: 404 }));
    const tagger = new Tagger({
      api: s.api,
      reader: plainReader,
      fetchFn,
      backoff: instant,
      warn: () => undefined,
    });
    const done = await tagger.start();
    expect(done).toMatchObject({ total: 3, failed: 3, tagged: 0 });
    expect(fetchFn).toHaveBeenCalledTimes(3);
  });

  it('uploads a cover once per hash, as covers/<hash>.jpg, and points the tracks at it', async () => {
    const s = fakeApi([row(1), row(2), row(3)]);
    const cover = { mime: 'image/png', bytes: new Uint8Array([1, 2, 3]) };
    const tagger = new Tagger({
      api: s.api,
      reader: async () => ({ readTags: () => tags({ cover }), readTagsTail: () => tags() }),
      fetchFn: okFetch,
      backoff: instant,
      toJpeg: async (c) => c.bytes,
      hash: async () => '0123456789abcdef0123456789abcdef',
    });
    await tagger.start();
    expect(s.uploads).toEqual(['covers/0123456789abcdef0123456789abcdef.jpg']);
    expect(s.patches.every((p) => p.body.cover_key === s.uploads[0])).toBe(true);
  });

  it('still tags the track when the cover cannot be stored', async () => {
    const s = fakeApi([row(1)]);
    s.api.uploadCover = async () => {
      throw new Error('bucket said no');
    };
    const tagger = new Tagger({
      api: s.api,
      reader: async () => ({
        readTags: () => tags({ cover: { mime: 'image/jpeg', bytes: new Uint8Array([9]) } }),
        readTagsTail: () => tags(),
      }),
      fetchFn: okFetch,
      backoff: instant,
      hash: async () => 'abcdef0123456789',
      warn: () => undefined,
    });
    const done = await tagger.start();
    expect(done).toMatchObject({ tagged: 1, failed: 0 });
    expect(s.patches[0]?.body.cover_key).toBeUndefined();
  });

  it('marks a file without tags as none, and "Rescan tags" queues the mount then reads it again', async () => {
    const s = fakeApi([row(1), row(2)]);
    const unsupported = () => {
      throw new Error('unsupported');
    };
    const tagger = new Tagger({
      api: s.api,
      reader: async () => ({ readTags: unsupported, readTagsTail: unsupported }),
      fetchFn: okFetch,
      backoff: instant,
    });
    await tagger.start();
    expect(s.patches.map((p) => p.body.tags_state)).toEqual(['none', 'none']);
    expect(s.state.get('1')).toBe('none');
    const again = await tagger.rescan('10');
    expect(s.api.rescan).toHaveBeenCalledWith('10');
    expect(again).toMatchObject({ total: 2, tagged: 2 });
    expect(s.patches).toHaveLength(4);
  });

  it('joins a run that is already going instead of starting a second one', async () => {
    const s = fakeApi([row(1)]);
    const tagger = new Tagger({
      api: s.api,
      reader: plainReader,
      fetchFn: okFetch,
      backoff: instant,
    });
    const a = tagger.start();
    const b = tagger.start();
    expect(a).toBe(b);
    await a;
    expect(s.patches).toHaveLength(1);
  });
});
