import { describe, expect, it } from 'vitest';
import {
  Fmp4Demuxer,
  chunked,
  demuxFmp4,
  hevcCodecString,
  opusHeadFromDops,
  parseEsds,
} from './fmp4';

// ---- a tiny box writer, enough to build the files ffmpeg would write ----------

const u8 = (...n: number[]) => new Uint8Array(n);
const be32 = (n: number) => u8((n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255);
const be16 = (n: number) => u8((n >> 8) & 255, n & 255);
const cat = (...parts: Uint8Array[]) => {
  const out = new Uint8Array(parts.reduce((a, p) => a + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
};
const str = (s: string) => u8(...[...s].map((c) => c.charCodeAt(0)));
const box = (type: string, ...body: Uint8Array[]) => {
  const b = cat(...body);
  return cat(be32(8 + b.length), str(type), b);
};
const zeros = (n: number) => new Uint8Array(n);

function trak(id: number, handler: 'vide' | 'soun', timescale: number, entry: Uint8Array) {
  const tkhd = box('tkhd', be32(0), be32(0), be32(0), be32(id), zeros(68));
  const mdhd = box('mdhd', be32(0), be32(0), be32(0), be32(timescale), be32(0), zeros(4));
  const hdlr = box('hdlr', be32(0), be32(0), str(handler), zeros(13));
  const stsd = box('stsd', be32(0), be32(1), entry);
  return box('trak', tkhd, box('mdia', mdhd, hdlr, box('minf', box('stbl', stsd))));
}

const avcC = u8(1, 0x64, 0x00, 0x28, 0xff, 0xe1, 0, 0);
const videoEntry = box('avc1', zeros(24), be16(1280), be16(720), zeros(50), box('avcC', avcC));
const esdsDsi = u8(0x12, 0x10); // AAC-LC 44.1k stereo
const esds = box(
  'esds',
  be32(0),
  u8(0x03, 0x19, 0, 1, 0), // ES_Descr: id, flags
  u8(0x04, 0x11, 0x40, 0x15, 0, 0, 0),
  zeros(8),
  u8(0x05, 0x02),
  esdsDsi,
);
const audioEntry = box('mp4a', zeros(16), be16(2), be16(16), zeros(4), be32(48000 * 65536), esds);

function moov() {
  const trex = (id: number, dur: number) =>
    box('trex', be32(0), be32(id), be32(1), be32(dur), be32(0), be32(0x10000));
  return box(
    'moov',
    box('mvhd', be32(0), be32(0), be32(0), be32(1000), be32(0), zeros(80)),
    trak(1, 'vide', 90000, videoEntry),
    trak(2, 'soun', 48000, audioEntry),
    box('mvex', trex(1, 3000), trex(2, 1024)),
  );
}

interface S {
  size: number;
  dur?: number;
  flags?: number;
  cts?: number;
}

function traf(id: number, dts: number, samples: S[], dataOffset: number, withDur: boolean) {
  const tfhd = box('tfhd', be32(0x020000), be32(id)); // default-base-is-moof
  const tfdt = box('tfdt', be32(0), be32(dts));
  const flags = 0x1 | 0x4 | 0x200 | (withDur ? 0x100 : 0) | 0x800;
  const body: Uint8Array[] = [
    be32(flags),
    be32(samples.length),
    be32(dataOffset),
    be32(0x02000000),
  ];
  for (const s of samples) {
    if (withDur) body.push(be32(s.dur ?? 0));
    body.push(be32(s.size), be32(s.cts ?? 0));
  }
  return box('traf', tfhd, tfdt, box('trun', ...body));
}

/** One fragment: video 2 samples (first key, B-frame style cts), audio 2 samples. */
function fragment(vDts: number, aDts: number) {
  const vs: S[] = [
    { size: 4, dur: 3000, cts: 6000 },
    { size: 3, dur: 3000, cts: 3000 },
  ];
  const as: S[] = [{ size: 2 }, { size: 2 }];
  // moof size does not depend on data offsets (fixed width fields), so build twice
  const build = (vo: number, ao: number) =>
    box(
      'moof',
      box('mfhd', be32(0), be32(1)),
      traf(1, vDts, vs, vo, true),
      traf(2, aDts, as, ao, false),
    );
  const size = build(0, 0).length;
  const moof = build(size + 8, size + 8 + 7);
  const mdat = box('mdat', u8(1, 1, 1, 1), u8(2, 2, 2), u8(3, 3), u8(4, 4));
  return cat(moof, mdat);
}

const file = () =>
  cat(box('ftyp', str('isom'), be32(0)), moov(), fragment(0, 0), fragment(6000, 2048));

describe('fmp4 demuxer', () => {
  it('reads tracks and configs from the moov', () => {
    const d = new Fmp4Demuxer();
    d.push(file());
    expect(d.tracks).toHaveLength(2);
    const [v, a] = d.tracks!;
    expect(v).toMatchObject({
      kind: 'video',
      codec: 'avc1.640028',
      width: 1280,
      height: 720,
      timescale: 90000,
    });
    expect([...v!.description!]).toEqual([...avcC]);
    expect(a).toMatchObject({ kind: 'audio', codec: 'mp4a.40.2', channels: 2, sampleRate: 48000 });
    expect([...a!.description!]).toEqual([...esdsDsi]);
  });

  it('returns packets with pts, durations and key flags', () => {
    const d = new Fmp4Demuxer();
    const pk = d.push(file());
    const video = pk.filter((p) => p.kind === 'video');
    expect(video).toHaveLength(4);
    // first_sample_flags marks the first sample sync; the trex default (0x10000) makes the rest delta
    expect(video.map((p) => p.key)).toEqual([true, false, true, false]);
    expect(video[0]!.timestampUs).toBe(Math.round((6000 * 1e6) / 90000));
    expect(video[0]!.durationUs).toBe(33333);
    expect([...video[0]!.data]).toEqual([1, 1, 1, 1]);
    expect([...video[1]!.data]).toEqual([2, 2, 2]);
    expect(video[1]!.timestampUs).toBe(Math.round((6000 * 1e6) / 90000)); // dts 3000 + cts 3000
    const audio = pk.filter((p) => p.kind === 'audio');
    expect(audio.map((p) => p.timestampUs)).toEqual([0, 21333, 42667, 64000]);
    expect(audio.every((p) => p.key)).toBe(true);
    expect([...audio[0]!.data]).toEqual([3, 3]);
  });

  it('copes with the input arriving in tiny pieces', async () => {
    const whole = new Fmp4Demuxer().push(file());
    const src = await demuxFmp4(chunked(file(), 7), 12);
    const got: unknown[] = [];
    for await (const p of src.packets) got.push(p);
    expect(got).toHaveLength(whole.length);
    expect(src.durationSec).toBe(12);
    expect(src.video?.config.codec).toBe('avc1.640028');
    expect(src.audio?.config.numberOfChannels).toBe(2);
  });

  it('rejects a file with no moov', async () => {
    await expect(demuxFmp4(chunked(box('ftyp', str('isom'), be32(0))))).rejects.toThrow(/no moov/);
  });
});

describe('codec strings', () => {
  it('builds the hvc1 string of ISO 14496-15', () => {
    const hvcc = new Uint8Array(23);
    hvcc[1] = 0x01; // profile idc 1, main tier
    hvcc.set([0x60, 0, 0, 0], 2);
    hvcc.set([0x90, 0, 0, 0, 0, 0], 6);
    hvcc[12] = 120;
    expect(hevcCodecString('hvc1', hvcc)).toBe('hvc1.1.6.L120.90');
  });

  it('builds an OpusHead from dOps', () => {
    const dops = cat(u8(0, 2), be16(312), be32(44100), be16(0), u8(0));
    const head = opusHeadFromDops(dops);
    expect(String.fromCharCode(...head.subarray(0, 8))).toBe('OpusHead');
    expect(head[9]).toBe(2);
    expect(head[10]! | (head[11]! << 8)).toBe(312);
    expect(head[12]! | (head[13]! << 8) | (head[14]! << 16)).toBe(44100);
  });

  it('reads AAC object type from esds', () => {
    const es = parseEsds(esds.subarray(8));
    expect(es?.oti).toBe(0x40);
    expect([...es!.dsi!]).toEqual([...esdsDsi]);
  });
});
