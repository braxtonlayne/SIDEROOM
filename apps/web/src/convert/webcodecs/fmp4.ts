// BYOS-CNV-002: a small fragmented-MP4 demuxer of our own (no library). The
// input is what ffmpeg.wasm writes for `-c copy -f mp4 -movflags
// frag_keyframe+empty_moov+default_base_moof`: one `moov` with empty sample
// tables, then `moof`+`mdat` pairs. Output is encoded packets plus the decoder
// configs WebCodecs needs. It reads only what ffmpeg writes (ISO base media,
// 32-bit sizes plus the 64-bit largesize form, trun/tfhd/tfdt as in ISO 14496-12).

import type { DemuxedSource, Packet } from './types';

interface Box {
  type: string;
  /** Offset of the box header in the buffer. */
  start: number;
  /** Offset of the first byte after the 8 or 16 byte header. */
  body: number;
  end: number;
}

const ascii = (b: Uint8Array, o: number, n = 4) => String.fromCharCode(...b.subarray(o, o + n));
const dv = (b: Uint8Array) => new DataView(b.buffer, b.byteOffset, b.byteLength);
const hex = (n: number, pad = 2) => n.toString(16).padStart(pad, '0');
const u64 = (v: DataView, o: number) => Number(v.getBigUint64(o));

function* boxes(b: Uint8Array, from = 0, to = b.byteLength): Generator<Box> {
  const v = dv(b);
  let o = from;
  while (o + 8 <= to) {
    let size = v.getUint32(o);
    const type = ascii(b, o + 4);
    let body = o + 8;
    if (size === 1) {
      if (o + 16 > to) return;
      size = u64(v, o + 8);
      body = o + 16;
    } else if (size === 0) {
      size = to - o;
    }
    if (size < body - o || o + size > to) return;
    yield { type, start: o, body, end: o + size };
    o += size;
  }
}

function find(b: Uint8Array, parent: Box | null, ...path: string[]): Box | null {
  let cur: Box | null = parent;
  for (const t of path) {
    let hit: Box | null = null;
    const from = cur ? cur.body : 0;
    const to = cur ? cur.end : b.byteLength;
    for (const c of boxes(b, from, to)) {
      if (c.type === t) {
        hit = c;
        break;
      }
    }
    if (!hit) return null;
    cur = hit;
  }
  return cur;
}

export interface Fmp4Track {
  id: number;
  kind: 'video' | 'audio';
  timescale: number;
  codec: string;
  description?: Uint8Array;
  width: number;
  height: number;
  sampleRate: number;
  channels: number;
  defaults: { duration: number; size: number; flags: number };
}

// ---- codec configuration boxes ------------------------------------------------

/** hvcC to the `hvc1.P.C.Lxx.B` string of ISO 14496-15 Annex E. */
export function hevcCodecString(entry: string, hvcc: Uint8Array): string {
  const b1 = hvcc[1]!;
  const space = ['', 'A', 'B', 'C'][b1 >> 6]!;
  const tier = (b1 & 0x20) !== 0 ? 'H' : 'L';
  const idc = b1 & 0x1f;
  let compat = dv(hvcc).getUint32(2);
  let rev = 0;
  for (let i = 0; i < 32; i++) {
    rev = (rev * 2 + (compat & 1)) >>> 0;
    compat >>>= 1;
  }
  const constraints = Array.from(hvcc.subarray(6, 12));
  while (constraints.length > 1 && constraints[constraints.length - 1] === 0) constraints.pop();
  return `${entry}.${space}${idc}.${rev.toString(16)}.${tier}${hvcc[12]}.${constraints.map((c) => c.toString(16)).join('.')}`;
}

function av1CodecString(av1c: Uint8Array): string {
  const profile = av1c[1]! >> 5;
  const level = av1c[1]! & 0x1f;
  const tier = (av1c[2]! & 0x80) !== 0 ? 'H' : 'M';
  const high = (av1c[2]! & 0x40) !== 0;
  const twelve = (av1c[2]! & 0x20) !== 0;
  return `av01.${profile}.${hex(level)}${tier}.${twelve ? '12' : high ? '10' : '08'}`;
}

function readDescriptorLength(b: Uint8Array, o: number): [number, number] {
  let len = 0;
  for (let i = 0; i < 4; i++) {
    const c = b[o++]!;
    len = (len << 7) | (c & 0x7f);
    if (!(c & 0x80)) break;
  }
  return [len, o];
}

/** esds -> { oti, dsi } (object type indication, decoder specific info). */
export function parseEsds(body: Uint8Array): { oti: number; dsi: Uint8Array | null } | null {
  let o = 4; // version + flags
  if (body[o] !== 0x03) return null;
  let len: number;
  [len, o] = readDescriptorLength(body, o + 1);
  const esEnd = Math.min(body.length, o + len);
  const flags = body[o + 2]!;
  o += 3;
  if (flags & 0x80) o += 2;
  if (flags & 0x40) o += 1 + body[o]!;
  if (flags & 0x20) o += 2;
  if (body[o] !== 0x04) return null;
  [len, o] = readDescriptorLength(body, o + 1);
  const oti = body[o]!;
  o += 13; // oti, streamType, bufferSize(3), max(4), avg(4)
  let dsi: Uint8Array | null = null;
  if (o < esEnd && body[o] === 0x05) {
    [len, o] = readDescriptorLength(body, o + 1);
    dsi = body.slice(o, o + len);
  }
  return { oti, dsi };
}

/** dOps (big endian) to an RFC 7845 OpusHead (little endian), the description WebCodecs wants. */
export function opusHeadFromDops(dops: Uint8Array): Uint8Array {
  const v = dv(dops);
  const channels = dops[1]!;
  const family = dops[10]!;
  const extra = family !== 0 ? dops.subarray(11) : new Uint8Array(0);
  const out = new Uint8Array(19 + extra.length);
  out.set([0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64, 1, channels]);
  const o = dv(out);
  o.setUint16(10, v.getUint16(2), true);
  o.setUint32(12, v.getUint32(4), true);
  o.setInt16(16, v.getInt16(8), true);
  out[18] = family;
  out.set(extra, 19);
  return out;
}

const VIDEO_ENTRIES = ['avc1', 'avc3', 'hvc1', 'hev1', 'vp09', 'av01'];
const AUDIO_ENTRIES = ['mp4a', 'Opus', 'ac-3', 'ec-3', 'fLaC'];

function parseSampleEntry(
  b: Uint8Array,
  entry: Box,
  kind: 'video' | 'audio',
): Partial<Fmp4Track> | null {
  const v = dv(b);
  const type = entry.type;
  const base = entry.body;
  const out: Partial<Fmp4Track> = {};
  let kids: [number, number];
  if (kind === 'video') {
    out.width = v.getUint16(base + 24);
    out.height = v.getUint16(base + 26);
    kids = [base + 78, entry.end];
  } else {
    out.channels = v.getUint16(base + 16);
    out.sampleRate = v.getUint32(base + 24) / 65536;
    kids = [base + 28, entry.end];
  }
  const child = (t: string): Uint8Array | null => {
    for (const c of boxes(b, kids[0], kids[1])) if (c.type === t) return b.subarray(c.body, c.end);
    return null;
  };
  switch (type) {
    case 'avc1':
    case 'avc3': {
      const c = child('avcC');
      if (!c) return null;
      out.codec = `${type}.${hex(c[1]!)}${hex(c[2]!)}${hex(c[3]!)}`;
      out.description = c.slice();
      return out;
    }
    case 'hvc1':
    case 'hev1': {
      const c = child('hvcC');
      if (!c) return null;
      out.codec = hevcCodecString(type, c);
      out.description = c.slice();
      return out;
    }
    case 'vp09': {
      const c = child('vpcC');
      if (!c) return null;
      out.codec = `vp09.${hex(c[4]!)}.${hex(c[5]!)}.${hex(c[6]! >> 4)}`;
      return out;
    }
    case 'av01': {
      const c = child('av1C');
      if (!c) return null;
      out.codec = av1CodecString(c);
      return out;
    }
    case 'mp4a': {
      const c = child('esds');
      const es = c ? parseEsds(c) : null;
      if (!es) return null;
      if (es.oti === 0x40 && es.dsi) {
        let aot = es.dsi[0]! >> 3;
        if (aot === 31) aot = 32 + (((es.dsi[0]! & 7) << 3) | (es.dsi[1]! >> 5));
        out.codec = `mp4a.40.${aot}`;
        out.description = es.dsi;
      } else if (es.oti === 0x6b || es.oti === 0x69) {
        out.codec = 'mp3';
      } else {
        return null;
      }
      return out;
    }
    case 'Opus': {
      const c = child('dOps');
      if (!c) return null;
      out.codec = 'opus';
      out.description = opusHeadFromDops(c);
      return out;
    }
    case 'ac-3':
    case 'ec-3':
      out.codec = type;
      return out;
    case 'fLaC': {
      const c = child('dfLa');
      if (!c) return null;
      out.codec = 'flac';
      const d = new Uint8Array(4 + c.length - 4);
      d.set([0x66, 0x4c, 0x61, 0x43]);
      d.set(c.subarray(4), 4);
      out.description = d;
      return out;
    }
    default:
      return null;
  }
}

/** Parses a `moov` box. Tracks with codecs we cannot configure are skipped. */
export function parseMoov(b: Uint8Array, moov: Box): { tracks: Fmp4Track[]; durationSec: number } {
  const v = dv(b);
  const trex = new Map<number, Fmp4Track['defaults']>();
  const mvex = find(b, moov, 'mvex');
  if (mvex) {
    for (const t of boxes(b, mvex.body, mvex.end)) {
      if (t.type !== 'trex') continue;
      trex.set(v.getUint32(t.body + 4), {
        duration: v.getUint32(t.body + 12),
        size: v.getUint32(t.body + 16),
        flags: v.getUint32(t.body + 20),
      });
    }
  }
  let durationSec = 0;
  const mvhd = find(b, moov, 'mvhd');
  if (mvhd) {
    const ver = b[mvhd.body]!;
    const ts = v.getUint32(mvhd.body + (ver === 1 ? 20 : 12));
    const dur = ver === 1 ? u64(v, mvhd.body + 24) : v.getUint32(mvhd.body + 16);
    durationSec = ts ? dur / ts : 0;
  }
  const tracks: Fmp4Track[] = [];
  for (const trak of boxes(b, moov.body, moov.end)) {
    if (trak.type !== 'trak') continue;
    const tkhd = find(b, trak, 'tkhd');
    const mdhd = find(b, trak, 'mdia', 'mdhd');
    const hdlr = find(b, trak, 'mdia', 'hdlr');
    const stsd = find(b, trak, 'mdia', 'minf', 'stbl', 'stsd');
    if (!tkhd || !mdhd || !hdlr || !stsd) continue;
    const handler = ascii(b, hdlr.body + 8);
    const kind = handler === 'vide' ? 'video' : handler === 'soun' ? 'audio' : null;
    if (!kind) continue;
    const id = v.getUint32(tkhd.body + (b[tkhd.body] === 1 ? 20 : 12));
    const timescale = v.getUint32(mdhd.body + (b[mdhd.body] === 1 ? 20 : 12));
    const [entry] = boxes(b, stsd.body + 8, stsd.end);
    if (!entry || !(kind === 'video' ? VIDEO_ENTRIES : AUDIO_ENTRIES).includes(entry.type))
      continue;
    const cfg = parseSampleEntry(b, entry, kind);
    if (!cfg?.codec) continue;
    tracks.push({
      id,
      kind,
      timescale,
      codec: cfg.codec,
      description: cfg.description,
      width: cfg.width ?? 0,
      height: cfg.height ?? 0,
      sampleRate: cfg.sampleRate ?? 0,
      channels: cfg.channels ?? 0,
      defaults: trex.get(id) ?? { duration: 0, size: 0, flags: 0 },
    });
  }
  return { tracks, durationSec };
}

// ---- fragments ----------------------------------------------------------------

const NON_SYNC = 0x10000;

/** Packets of one `moof` + following `mdat` (both in `buf`, moof starting at `moofStart`). */
function parseFragment(
  buf: Uint8Array,
  moof: Box,
  tracks: Map<number, Fmp4Track>,
  nextDts: Map<number, number>,
): Packet[] {
  const v = dv(buf);
  const out: Packet[] = [];
  let cursor = -1;
  for (const traf of boxes(buf, moof.body, moof.end)) {
    if (traf.type !== 'traf') continue;
    const tfhd = find(buf, traf, 'tfhd');
    if (!tfhd) continue;
    const track = tracks.get(v.getUint32(tfhd.body + 4));
    if (!track) continue;
    const tfFlags = v.getUint32(tfhd.body) & 0xffffff;
    let o = tfhd.body + 8;
    let base = moof.start; // default-base-is-moof, and ffmpeg's fragments never use anything else
    if (tfFlags & 0x1) {
      base = moof.start; // an absolute base_data_offset is not used by our producer; treat as moof relative
      o += 8;
    }
    if (tfFlags & 0x2) o += 4;
    let defDur = track.defaults.duration;
    let defSize = track.defaults.size;
    let defFlags = track.defaults.flags;
    if (tfFlags & 0x8) {
      defDur = v.getUint32(o);
      o += 4;
    }
    if (tfFlags & 0x10) {
      defSize = v.getUint32(o);
      o += 4;
    }
    if (tfFlags & 0x20) defFlags = v.getUint32(o);
    const tfdt = find(buf, traf, 'tfdt');
    let dts = nextDts.get(track.id) ?? 0;
    if (tfdt)
      dts = tfdt.body && buf[tfdt.body] === 1 ? u64(v, tfdt.body + 4) : v.getUint32(tfdt.body + 4);
    for (const trun of boxes(buf, traf.body, traf.end)) {
      if (trun.type !== 'trun') continue;
      const ver = buf[trun.body]!;
      const f = v.getUint32(trun.body) & 0xffffff;
      const count = v.getUint32(trun.body + 4);
      let p = trun.body + 8;
      if (f & 0x1) {
        cursor = base + v.getInt32(p);
        p += 4;
      } else if (cursor < 0) {
        cursor = moof.end + 8;
      }
      let firstFlags: number | null = null;
      if (f & 0x4) {
        firstFlags = v.getUint32(p);
        p += 4;
      }
      for (let i = 0; i < count; i++) {
        let dur = defDur;
        let size = defSize;
        let flags = i === 0 && firstFlags !== null ? firstFlags : defFlags;
        let cts = 0;
        if (f & 0x100) {
          dur = v.getUint32(p);
          p += 4;
        }
        if (f & 0x200) {
          size = v.getUint32(p);
          p += 4;
        }
        if (f & 0x400) {
          flags = v.getUint32(p);
          p += 4;
        }
        if (f & 0x800) {
          cts = ver === 0 ? v.getUint32(p) : v.getInt32(p);
          p += 4;
        }
        if (cursor + size > buf.byteLength) throw new Error('fmp4: sample runs past the mdat');
        out.push({
          kind: track.kind,
          timestampUs: Math.round(((dts + cts) * 1e6) / track.timescale),
          durationUs: Math.round((dur * 1e6) / track.timescale),
          key: track.kind === 'audio' || (flags & NON_SYNC) === 0,
          data: buf.slice(cursor, cursor + size),
        });
        cursor += size;
        dts += dur;
      }
    }
    nextDts.set(track.id, dts);
  }
  return out;
}

/** Incremental demuxer: push bytes as they arrive, collect packets. */
export class Fmp4Demuxer {
  tracks: Fmp4Track[] | null = null;
  durationSec = 0;
  private pending = new Uint8Array(0);
  private byId = new Map<number, Fmp4Track>();
  private nextDts = new Map<number, number>();
  private moofBytes: Uint8Array | null = null;

  push(chunk: Uint8Array): Packet[] {
    const merged = new Uint8Array(this.pending.length + chunk.length);
    merged.set(this.pending);
    merged.set(chunk, this.pending.length);
    const out: Packet[] = [];
    let consumed = 0;
    const v = dv(merged);
    for (;;) {
      if (merged.length - consumed < 8) break;
      let size = v.getUint32(consumed);
      if (size === 1) {
        if (merged.length - consumed < 16) break;
        size = u64(v, consumed + 8);
      }
      if (size === 0) break; // "to end of file" boxes are not produced by ffmpeg
      if (merged.length - consumed < size) break;
      const bytes = merged.subarray(consumed, consumed + size);
      const type = ascii(bytes, 4);
      if (type === 'moov') {
        const box = [...boxes(bytes)][0]!;
        const parsed = parseMoov(bytes, box);
        this.tracks = parsed.tracks;
        this.durationSec = parsed.durationSec;
        this.byId = new Map(parsed.tracks.map((t) => [t.id, t]));
      } else if (type === 'moof') {
        this.moofBytes = bytes.slice();
      } else if (type === 'mdat' && this.moofBytes) {
        const frag = new Uint8Array(this.moofBytes.length + bytes.length);
        frag.set(this.moofBytes);
        frag.set(bytes, this.moofBytes.length);
        const moof = [...boxes(frag)][0]!;
        out.push(...parseFragment(frag, moof, this.byId, this.nextDts));
        this.moofBytes = null;
      }
      consumed += size;
    }
    this.pending = merged.slice(consumed);
    return out;
  }
}

/** Slices a buffer into pieces so the demuxer never copies a whole big file at once. */
export async function* chunked(bytes: Uint8Array, size = 1 << 20): AsyncGenerator<Uint8Array> {
  for (let o = 0; o < bytes.length; o += size)
    yield bytes.subarray(o, Math.min(bytes.length, o + size));
}

/**
 * Reads chunks until the `moov` is in, then returns the configs and an async
 * iterable over the packets. `durationHintSec` fills in when the file's own
 * duration is 0 (fragmented files usually say 0).
 */
export async function demuxFmp4(
  chunks: AsyncIterable<Uint8Array>,
  durationHintSec = 0,
): Promise<DemuxedSource> {
  const demux = new Fmp4Demuxer();
  const it = chunks[Symbol.asyncIterator]();
  const first: Packet[] = [];
  while (!demux.tracks) {
    const n = await it.next();
    if (n.done) throw new Error('fmp4: no moov box found');
    first.push(...demux.push(n.value));
  }
  const tracks = demux.tracks;
  const video = tracks.find((t) => t.kind === 'video');
  const audio = tracks.find((t) => t.kind === 'audio');
  async function* packets(): AsyncGenerator<Packet> {
    yield* first;
    for (;;) {
      const n = await it.next();
      if (n.done) return;
      yield* demux.push(n.value);
    }
  }
  return {
    video: video && {
      config: {
        codec: video.codec,
        codedWidth: video.width,
        codedHeight: video.height,
        description: video.description,
      },
      width: video.width,
      height: video.height,
    },
    audio: audio && {
      config: {
        codec: audio.codec,
        sampleRate: audio.sampleRate,
        numberOfChannels: audio.channels,
        description: audio.description,
      },
      sampleRate: audio.sampleRate,
      channels: audio.channels,
    },
    packets: packets(),
    durationSec: demux.durationSec || durationHintSec,
  };
}
