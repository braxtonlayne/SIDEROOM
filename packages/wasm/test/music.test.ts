import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { loadMusic } from '../src/index.js';

/**
 * MUS-WASM-001: the facade over sr-music-wasm (hand-written ID3/FLAC/Ogg/MP4 reader). The Rust unit
 * tests cover the parsers; this checks the JS bridge (JSON + separate cover bytes). Needs `pnpm build`.
 * Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail instead of skipping when the build is missing.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/music/music_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

const syncsafe = (n: number) => [(n >> 21) & 0x7f, (n >> 14) & 0x7f, (n >> 7) & 0x7f, n & 0x7f];
const enc = new TextEncoder();

function frame(id: string, body: number[]): number[] {
  return [...enc.encode(id), ...syncsafe(body.length), 0, 0, ...body];
}

/** An ID3v2.4 tag with a title, a track and a 4-byte "JPEG" cover. */
function mp3(): Uint8Array {
  const jpeg = [0xff, 0xd8, 0xff, 0xe0];
  const frames = [
    ...frame('TIT2', [3, ...enc.encode('Facade Song')]),
    ...frame('TRCK', [0, ...enc.encode('2/9')]),
    ...frame('APIC', [0, ...enc.encode('image/jpeg'), 0, 3, 0, ...jpeg]),
  ];
  return Uint8Array.from([...enc.encode('ID3'), 4, 0, 0, ...syncsafe(frames.length), ...frames]);
}

describe.skipIf(!built)('music facade', () => {
  it('reads tags and the cover bytes', async () => {
    const music = await loadMusic(readFileSync(wasmPath));
    const t = music.readTags(mp3());
    expect(t.title).toBe('Facade Song');
    expect(t.track).toBe(2);
    expect(t.artist).toBeNull();
    expect(t.cover?.mime).toBe('image/jpeg');
    expect(Array.from(t.cover?.bytes ?? [])).toEqual([0xff, 0xd8, 0xff, 0xe0]);
  });

  it('throws on garbage and truncated input', async () => {
    const music = await loadMusic(readFileSync(wasmPath));
    expect(() => music.readTags(enc.encode('definitely not audio'))).toThrow();
    expect(() => music.readTags(new Uint8Array(0))).toThrow();
    expect(() => music.readTags(mp3().slice(0, 5))).toThrow();
    expect(() => music.readTagsTail(new Uint8Array(10))).toThrow();
  });

  it('parses LRC and names sidecars', async () => {
    const music = await loadMusic(readFileSync(wasmPath));
    expect(music.parseLrc('[00:02.5]b\n[00:01]a')).toEqual([
      { ms: 1000, line: 'a' },
      { ms: 2500, line: 'b' },
    ]);
    expect(music.lrcSidecarNames('m/x.flac')).toEqual(['m/x.lrc', 'm/x.LRC', 'm/x.flac.lrc']);
  });
});

describe.skipIf(!built)('music facade: M4A with moov past the head', () => {
  const box = (type: string, body: number[]) => {
    const size = body.length + 8;
    return [
      (size >>> 24) & 255,
      (size >> 16) & 255,
      (size >> 8) & 255,
      size & 255,
      ...enc.encode(type),
      ...body,
    ];
  };
  const ftyp = box('ftyp', [...enc.encode('M4A '), 0, 0, 0, 0, ...enc.encode('M4A mp42')]);

  it('throws NEED_MOOV with the next box offset', async () => {
    const music = await loadMusic(readFileSync(wasmPath));
    // mdat declares 1000 bytes but the head carries only 8 of them.
    const head = Uint8Array.from([...ftyp, 0, 0, 3, 232, ...enc.encode('mdat'), 1, 2, 3, 4]);
    let err: { code?: string; offset?: number } | undefined;
    try {
      music.readTags(head);
    } catch (e) {
      err = e as { code?: string; offset?: number };
    }
    expect(err?.code).toBe('NEED_MOOV');
    expect(err?.offset).toBe(ftyp.length + 1000);
  });

  it('reads ftyp + moov fetched by range', async () => {
    const music = await loadMusic(readFileSync(wasmPath));
    const data = box('data', [0, 0, 0, 1, 0, 0, 0, 0, ...enc.encode('Late Moov')]);
    // The title atom type is the 4 bytes A9 'n' 'a' 'm'.
    const nam = [0, 0, 0, data.length + 8, 0xa9, 0x6e, 0x61, 0x6d, ...data];
    const meta = box('meta', [0, 0, 0, 0, ...box('ilst', nam)]);
    const moov = box('moov', box('udta', meta));
    const t = music.readTags(Uint8Array.from([...ftyp, ...moov]));
    expect(t.title).toBe('Late Moov');
  });
});
