import { describe, expect, it, vi } from 'vitest';
import { HEAD_BYTES, MAX_MOOV_BYTES, readTrackTags } from '../readTrack';
import { locateMoov, parseBox } from '../ranges';
import type { MusicTags, TagReader } from '../types';

const tags = (over: Partial<MusicTags> = {}): MusicTags => ({
  title: 'Song',
  artist: null,
  album: null,
  albumArtist: null,
  track: null,
  disc: null,
  year: null,
  genre: null,
  durationHint: null,
  cover: null,
  ...over,
});

function box(type: string, payload: number, fill = 0): Uint8Array {
  const b = new Uint8Array(8 + payload).fill(fill);
  new DataView(b.buffer).setUint32(0, b.length);
  for (let i = 0; i < 4; i++) b[4 + i] = type.charCodeAt(i);
  return b;
}
const cat = (...p: Uint8Array[]) => {
  const o = new Uint8Array(p.reduce((n, x) => n + x.length, 0));
  let at = 0;
  for (const x of p) {
    o.set(x, at);
    at += x.length;
  }
  return o;
};

/** A fake presigned object: answers `Range: bytes=a-b` with 206 and records every request. */
function fakeBucket(file: Uint8Array) {
  const log: Array<{ start: number; length: number }> = [];
  const fetchFn = vi.fn(async (_url: string, init?: RequestInit) => {
    const m = /bytes=(\d+)-(\d+)/.exec(
      (init?.headers as Record<string, string> | undefined)?.Range ?? '',
    );
    if (!m) return new Response(null, { status: 400 });
    const start = Number(m[1]);
    const end = Math.min(Number(m[2]), file.length - 1);
    log.push({ start, length: end - start + 1 });
    return new Response(file.slice(start, end + 1), { status: 206 });
  });
  return { fetchFn, log };
}

const readerOver = (b: ReturnType<typeof fakeBucket>) => async (s: number, n: number) =>
  new Uint8Array(
    await (await b.fetchFn('u', { headers: { Range: `bytes=${s}-${s + n - 1}` } })).arrayBuffer(),
  );

describe('box walk', () => {
  it('reads sizes, the 64-bit form and refuses nonsense', () => {
    expect(parseBox(box('moov', 10), 100)).toEqual({ size: 18, type: 'moov' });
    const big = new Uint8Array(16);
    new DataView(big.buffer).setUint32(0, 1);
    new DataView(big.buffer).setBigUint64(8, 5_000_000_000n);
    big.set([0x6d, 0x64, 0x61, 0x74], 4);
    expect(parseBox(big, 9e9)).toEqual({ size: 5_000_000_000, type: 'mdat' });
    expect(parseBox(new Uint8Array(4), 100)).toBeNull();
    const tiny = box('free', 0);
    new DataView(tiny.buffer).setUint32(0, 3);
    expect(parseBox(tiny, 100)).toBeNull();
  });

  it('finds moov behind several boxes and stops at the step cap', async () => {
    const file = cat(box('ftyp', 12), box('free', 40), box('mdat', 5000), box('moov', 90));
    const b = fakeBucket(file);
    const at = await locateMoov(readerOver(b), 0, file.length);
    expect(at).toEqual({ offset: 20 + 48 + 5008, size: 98 });
    expect(b.log.every((r) => r.length <= 16)).toBe(true);
    expect(b.log).toHaveLength(4);
    // 40 tiny boxes and no moov: gives up after 32 reads.
    const many = cat(...Array.from({ length: 40 }, () => box('free', 0)));
    const b2 = fakeBucket(many);
    expect(await locateMoov(readerOver(b2), 0, many.length)).toBeNull();
    expect(b2.log).toHaveLength(32);
  });
});

describe('an M4A with moov after mdat, beyond the first 1 MiB', () => {
  const ftyp = box('ftyp', 12, 7);
  const mdat = box('mdat', HEAD_BYTES + 4096);
  const moov = box('moov', 300, 9);
  const file = cat(ftyp, mdat, moov);
  const moovAt = ftyp.length + mdat.length;

  const reader = (calls: Uint8Array[]): TagReader => ({
    readTags(head) {
      calls.push(head);
      if (head.length === HEAD_BYTES)
        throw Object.assign(new Error('moov not in head'), {
          code: 'NEED_MOOV',
          offset: ftyp.length,
        });
      return tags({ title: 'Late moov' });
    },
    readTagsTail() {
      throw new Error('no tail for m4a');
    },
  });

  it('walks to moov with 16-byte reads, fetches moov alone and passes ftyp + moov to readTags', async () => {
    const { fetchFn, log } = fakeBucket(file);
    const calls: Uint8Array[] = [];
    const warn = vi.fn();
    const out = await readTrackTags('https://bucket/x.m4a?sig', file.length, 'm4a', {
      reader: reader(calls),
      fetchFn,
      warnOnce: warn,
    });
    expect(out).toEqual({ kind: 'tags', tags: tags({ title: 'Late moov' }) });
    // head, a 16-byte read at mdat (the offset NEED_MOOV named), one at moov, then moov itself.
    expect(log[0]).toEqual({ start: 0, length: HEAD_BYTES });
    const walk = log.slice(1, -1);
    expect(walk.map((r) => r.start)).toEqual([ftyp.length, moovAt]);
    expect(walk.every((r) => r.length === 16)).toBe(true);
    expect(log.at(-1)).toEqual({ start: moovAt, length: moov.length });
    // The second readTags call got ftyp followed by moov, nothing else.
    expect(calls).toHaveLength(2);
    expect(calls[1]).toEqual(cat(ftyp, moov));
    expect(warn).not.toHaveBeenCalled();
  });

  it('counts a moov bigger than 8 MiB as untagged and says so', async () => {
    const hugeMoov = new Uint8Array(16); // only the header matters: the size field claims over 8 MiB
    new DataView(hugeMoov.buffer).setUint32(0, MAX_MOOV_BYTES + 1024);
    hugeMoov.set([0x6d, 0x6f, 0x6f, 0x76], 4);
    const big = cat(ftyp, mdat, hugeMoov);
    const declared = ftyp.length + mdat.length + MAX_MOOV_BYTES + 1024;
    const { fetchFn, log } = fakeBucket(big);
    const warn = vi.fn();
    const out = await readTrackTags('u', declared, 'm4a', {
      reader: reader([]),
      fetchFn,
      warnOnce: warn,
    });
    expect(out).toEqual({ kind: 'none', reason: 'moov-too-large' });
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls[0]?.[0]).toBe('moov-too-large');
    // No attempt was made to fetch it.
    expect(log.some((r) => r.length > 16 && r.start > 0)).toBe(false);
  });

  it('gives up quietly when no moov turns up within 32 boxes', async () => {
    const frees = cat(...Array.from({ length: 40 }, () => box('free', 8)));
    const f = cat(ftyp, frees);
    const { fetchFn, log } = fakeBucket(f);
    const warn = vi.fn();
    const r: TagReader = {
      readTags: () => {
        throw Object.assign(new Error('x'), { code: 'NEED_MOOV', offset: ftyp.length });
      },
      readTagsTail: () => {
        throw new Error('x');
      },
    };
    const out = await readTrackTags('u', f.length, 'm4a', { reader: r, fetchFn, warnOnce: warn });
    expect(out).toEqual({ kind: 'none', reason: 'moov-not-found' });
    expect(log.filter((x) => x.length <= 16)).toHaveLength(32);
  });
});

describe('the other containers', () => {
  it('falls back to the ID3v1 tail for an MP3 without a head tag, and not for FLAC', async () => {
    const file = new Uint8Array(5000);
    const { fetchFn, log } = fakeBucket(file);
    const r: TagReader = {
      readTags: () => {
        throw new Error('no id3v2');
      },
      readTagsTail: (t) => {
        expect(t.length).toBe(128);
        return tags({ title: 'v1' });
      },
    };
    const warnOnce = vi.fn();
    expect(await readTrackTags('u', file.length, 'mp3', { reader: r, fetchFn, warnOnce })).toEqual({
      kind: 'tags',
      tags: tags({ title: 'v1' }),
    });
    expect(log.at(-1)).toEqual({ start: 5000 - 128, length: 128 });
    expect(await readTrackTags('u', file.length, 'flac', { reader: r, fetchFn, warnOnce })).toEqual(
      { kind: 'none', reason: 'no-tags' },
    );
  });

  it('lets a network failure propagate so the caller can retry it', async () => {
    const fetchFn = vi.fn(async () => new Response(null, { status: 503 }));
    await expect(
      readTrackTags('u', 10, 'mp3', {
        reader: { readTags: () => tags(), readTagsTail: () => tags() },
        fetchFn,
        warnOnce: vi.fn(),
      }),
    ).rejects.toMatchObject({ status: 503 });
  });
});
