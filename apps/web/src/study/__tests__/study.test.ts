import type { MusicTags, ZipProbe } from '@sideroom/wasm';
import { describe, expect, it } from 'vitest';
import type { PageLister } from '../../library/reader/cbzRange';
import {
  archiveCover,
  audioCover,
  captureFrame,
  CoverQueue,
  coverSource,
  extractCover,
  pickCoverPage,
} from '../covers';
import { countByType, filterByType, titleOf, typeOf } from '../fileTypes';
import { coverChoices, displayTitle, noteFromForm, noteMap, withNote } from '../notes';
import {
  continueRails,
  isFinished,
  recentlyAdded,
  recordOpened,
  togglePin,
  type Opened,
} from '../rails';
import {
  allEntries,
  buildSidebar,
  entryOfMount,
  legacyVisible,
  provenanceOf,
  sortEntries,
} from '../sources';

// ---- fixtures generated here: a ZIP with stored entries, an MP3 with an ID3v2.3 APIC frame ----
const enc = new TextEncoder();

function buildZip(entries: { name: string; data: Uint8Array }[]) {
  const parts: Uint8Array[] = [];
  const probes: ZipProbe[] = [];
  let at = 0;
  for (const e of entries) {
    const name = enc.encode(e.name);
    const header = new Uint8Array(30 + name.length);
    const dv = new DataView(header.buffer);
    dv.setUint32(0, 0x04034b50, true);
    dv.setUint32(18, e.data.length, true);
    dv.setUint32(22, e.data.length, true);
    dv.setUint16(26, name.length, true);
    header.set(name, 30);
    probes.push({
      name: e.name,
      local_offset: at,
      compressed: e.data.length,
      size: e.data.length,
      method: 0,
    });
    parts.push(header, e.data);
    at += header.length + e.data.length;
  }
  parts.push(new Uint8Array(300).fill(7)); // stand-in central directory; the lister is faked
  const bytes = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    bytes.set(p, o);
    o += p.length;
  }
  return { bytes, probes };
}

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4]);
const JPG = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 9, 9, 9, 9]);

/** ID3v2.3 header + one APIC frame holding `pic`. */
function mp3WithCover(pic: Uint8Array, mime = 'image/png'): Uint8Array {
  const body = new Uint8Array([
    0,
    ...enc.encode(mime),
    0,
    3,
    0, // text encoding, mime, NUL, picture type (front), empty description
    ...pic,
  ]);
  const frame = new Uint8Array(10 + body.length);
  frame.set(enc.encode('APIC'));
  new DataView(frame.buffer).setUint32(4, body.length);
  frame.set(body, 10);
  const head = new Uint8Array(10);
  head.set(enc.encode('ID3'));
  head[3] = 3;
  const size = frame.length;
  head.set([(size >> 21) & 127, (size >> 14) & 127, (size >> 7) & 127, size & 127], 6);
  const out = new Uint8Array(10 + frame.length + 64);
  out.set(head);
  out.set(frame, 10);
  return out;
}

/** The facade's `readTags`, reduced to the one frame these fixtures carry. */
const fakeTags = {
  readTags(head: Uint8Array): MusicTags {
    const t: MusicTags = {
      title: null,
      artist: null,
      album: null,
      albumArtist: null,
      track: null,
      disc: null,
      year: null,
      genre: null,
      durationHint: null,
      cover: null,
    };
    if (String.fromCharCode(...head.subarray(0, 3)) !== 'ID3') throw new Error('no tags');
    const dv = new DataView(head.buffer, head.byteOffset);
    const len = dv.getUint32(14);
    const frame = head.subarray(20, 20 + len);
    let i = 1;
    let mime = '';
    while (frame[i] !== 0) mime += String.fromCharCode(frame[i++]!);
    i += 3; // NUL, picture type, empty description NUL
    t.cover = { mime, bytes: frame.slice(i) };
    return t;
  },
  readTagsTail(): MusicTags {
    throw new Error('no tail');
  },
};

/** A fetch over bytes honouring `Range: bytes=a-b` and `bytes=-n`. */
function rangeFetch(bytes: Uint8Array) {
  return (async (_url: string, init?: RequestInit) => {
    const h = (init?.headers ?? {}) as Record<string, string>;
    const m = /bytes=(\d*)-(\d*)/.exec(h.Range ?? '');
    let a = 0;
    let b = bytes.length - 1;
    if (m) {
      if (m[1] === '') a = Math.max(0, bytes.length - Number(m[2]));
      else {
        a = Number(m[1]);
        b = Math.min(bytes.length - 1, Number(m[2] || bytes.length - 1));
      }
    }
    const slice = bytes.slice(a, b + 1);
    return new Response(slice, {
      status: 206,
      headers: { 'content-range': `bytes ${a}-${b}/${bytes.length}` },
    });
  }) as unknown as typeof fetch;
}

const lister = (probes: ZipProbe[]): PageLister => ({
  firstTail: 200,
  listPages: () => ({ ok: true, pages: probes }),
});

const bytesOf = (b: Blob | null) =>
  new Promise<Uint8Array>((res) => {
    const r = new FileReader();
    r.onload = () => res(new Uint8Array(r.result as ArrayBuffer));
    r.readAsArrayBuffer(b!);
  });

describe('type filters', () => {
  it('sorts files into the six filters by extension and folder', () => {
    expect(typeOf('Films/a.MKV')).toBe('video');
    expect(typeOf('Music/a.flac')).toBe('audio');
    expect(typeOf('Books/a.epub')).toBe('reading');
    expect(typeOf('Books/a.cbz')).toBe('reading');
    expect(typeOf('Books/a.pdf')).toBe('reading');
    expect(typeOf('Pics/a.webp')).toBe('images');
    expect(typeOf('emu/roms/anything.bin')).toBe('games');
    expect(typeOf('mods/quake.pk3')).toBe('games');
    expect(typeOf('notes.txt')).toBe('other');
    expect(typeOf('archive.zip')).toBe('other');
    expect(typeOf('noext')).toBe('other');
  });
  it('filters and counts', () => {
    const files = ['a.mp4', 'b.mp3', 'c.epub', 'd.png', 'e.txt', 'f.mkv'].map((key) => ({ key }));
    expect(filterByType(files, 'video').map((f) => f.key)).toEqual(['a.mp4', 'f.mkv']);
    expect(filterByType(files, null)).toHaveLength(6);
    expect(countByType(files)).toEqual({
      video: 2,
      audio: 1,
      reading: 1,
      games: 0,
      images: 1,
      other: 1,
    });
  });
  it('turns a file name into a title', () => {
    expect(titleOf('Films/The_Long.Take.2019.mkv')).toBe('The Long Take 2019');
    expect(titleOf('x/README')).toBe('README');
  });
});

describe('sidebar by source', () => {
  const inputs = {
    own: [
      { id: '1', label: 'Home', status: 'active' },
      { id: '9', label: 'Old', status: 'disabled' },
    ],
    incoming: [
      { mount_id: '2', owner_id: '20', label: 'Alex films' },
      { mount_id: '3', owner_id: '20', label: 'Alex books' },
      { mount_id: '4', owner_id: '21', label: 'Sam' },
    ],
    shelf: [
      { mount_id: '1', label: 'Home', scope: 'own' as const, owner_id: '10' },
      { mount_id: '2', label: 'Alex films', scope: 'friend' as const, owner_id: '20' },
      { mount_id: '5', label: 'Club', scope: 'guild' as const, guild_id: '70' },
      { mount_id: '6', label: 'Staff', scope: 'staff' as const },
    ],
    roms: [
      { mount_id: '5', mount_label: 'Club', scope: 'guild', guild_id: '70' },
      { mount_id: '7', mount_label: 'Arcade', scope: 'guild', guild_id: '71' },
      { mount_id: '1', mount_label: 'Home', scope: 'own' },
    ],
  };

  it('groups mounts: My Files, one entry per friend, one per guild, each mount once', () => {
    const s = buildSidebar(legacyVisible(inputs));
    expect(s.mine?.mounts.map((m) => m.mountId)).toEqual(['1']);
    expect(s.friends.map((e) => [e.ownerId, e.mounts.map((m) => m.mountId)])).toEqual([
      ['20', ['2', '3']],
      ['21', ['4']],
    ]);
    expect(s.guilds.map((e) => [e.guildId, e.mounts.map((m) => m.mountId)])).toEqual([
      ['70', ['5']],
      ['71', ['7']],
    ]);
    expect(allEntries(s).map((e) => e.id)).toEqual([
      'mine',
      'friend:20',
      'friend:21',
      'guild:70',
      'guild:71',
    ]);
  });

  it('never shows staff-only mounts or unusable own mounts, and omits empty groups', () => {
    const s = buildSidebar(legacyVisible(inputs));
    const ids = allEntries(s).flatMap((e) => e.mounts.map((m) => m.mountId));
    expect(ids).not.toContain('6');
    expect(ids).not.toContain('9');
    const empty = buildSidebar([]);
    expect(empty.mine).toBeNull();
    expect(allEntries(empty)).toEqual([]);
  });

  it('accepts a /shelves/visible shaped list directly', () => {
    const s = buildSidebar([
      { shelf_id: '1', kind: 'own', label: 'a' },
      { shelf_id: '2', kind: 'friend', label: 'b', owner_id: '5' },
      { shelf_id: '2', kind: 'friend', label: 'b', owner_id: '5' },
      { shelf_id: '3', kind: 'guild', label: 'c', guild_id: '8' },
    ]);
    expect(s.friends[0]!.mounts).toHaveLength(1);
    expect(entryOfMount(s, '3')?.id).toBe('guild:8');
    expect(entryOfMount(s, '99')).toBeNull();
  });

  it('puts friends and guilds in name order', () => {
    const s = buildSidebar(legacyVisible(inputs));
    const names: Record<string, string> = { 'friend:20': 'Zed', 'friend:21': 'Amy' };
    expect(sortEntries(s.friends, (e) => names[e.id]!).map((e) => e.id)).toEqual([
      'friend:21',
      'friend:20',
    ]);
  });

  it('words the provenance of each entry', () => {
    const s = buildSidebar(legacyVisible(inputs));
    const handle = (id: string) => ({ '10': 'me', '20': 'alex' })[id] ?? null;
    const guild = (id: string) => (id === '70' ? 'Club' : null);
    expect(provenanceOf(s.mine!, handle, guild, '10')).toEqual({ kind: 'own', handle: 'me' });
    expect(provenanceOf(s.friends[0]!, handle, guild, '10')).toEqual({
      kind: 'friend',
      handle: 'alex',
    });
    expect(provenanceOf(s.friends[1]!, handle, guild, '10')).toEqual({
      kind: 'friend',
      handle: null,
    });
    expect(provenanceOf(s.guilds[0]!, handle, guild, '10')).toEqual({
      kind: 'guild',
      name: 'Club',
    });
  });
});

describe('cover extraction from the file itself', () => {
  it('picks an extractor by file type', () => {
    expect(coverSource('a.mp3')).toBe('audio');
    expect(coverSource('a.cbz')).toBe('cbz');
    expect(coverSource('a.epub')).toBe('epub');
    expect(coverSource('a.mp4')).toBe('video');
    expect(coverSource('a.png')).toBe('image');
    expect(coverSource('a.pdf')).toBeNull();
    expect(coverSource('a.nes')).toBeNull();
  });

  it('reads ID3 art from a generated MP3 by a range read', async () => {
    const mp3 = mp3WithCover(PNG);
    const blob = await audioCover('https://b/x.mp3', mp3.length, 'mp3', {
      fetchFn: rangeFetch(mp3),
      tags: async () => fakeTags,
    });
    expect(blob?.type).toBe('image/png');
    expect(await bytesOf(blob)).toEqual(PNG);
  });

  it('gives no cover for audio without a picture, or without a reader', async () => {
    const plain = new Uint8Array(200);
    const d = { fetchFn: rangeFetch(plain), tags: async () => fakeTags };
    expect(await extractCover('a.mp3', 'https://b/x', plain.length, d)).toBeNull();
    expect(await audioCover('u', 10, 'mp3', {})).toBeNull();
  });

  it('takes page 1 of a generated CBZ', async () => {
    const zip = buildZip([
      { name: '001.jpg', data: JPG },
      { name: '002.jpg', data: new Uint8Array([5, 5]) },
    ]);
    const blob = await archiveCover('https://b/a.cbz', false, {
      fetchFn: rangeFetch(zip.bytes),
      lister: async () => lister(zip.probes),
    });
    expect(blob?.type).toBe('image/jpeg');
    expect(await bytesOf(blob)).toEqual(JPG);
  });

  it('takes the cover image of a generated EPUB, falling back to the first image', async () => {
    const zip = buildZip([
      { name: 'OEBPS/images/fig1.png', data: new Uint8Array([1, 1]) },
      { name: 'OEBPS/images/cover.jpg', data: JPG },
    ]);
    const d = { fetchFn: rangeFetch(zip.bytes), lister: async () => lister(zip.probes) };
    expect(await bytesOf(await archiveCover('u', true, d))).toEqual(JPG);
    expect(pickCoverPage(['a/1.png', 'a/2.png'], true)).toBe(0);
    expect(pickCoverPage([], true)).toBe(-1);
    expect(pickCoverPage(['x/discover.png', 'x/cover.png'], true)).toBe(1);
  });

  it('routes a video to the frame capture and swallows failures', async () => {
    const frame = new Blob([JPG], { type: 'image/jpeg' });
    const seen: string[] = [];
    const ok = await extractCover('Films/a.mp4', 'https://b/a.mp4', 5, {
      frame: async (u) => {
        seen.push(u);
        return frame;
      },
    });
    expect(ok).toBe(frame);
    expect(seen).toEqual(['https://b/a.mp4']);
    const bad = await extractCover('a.mp4', 'u', 1, {
      frame: async () => {
        throw new Error('decode');
      },
    });
    expect(bad).toBeNull();
    expect(await extractCover('a.pdf', 'u', 1, {})).toBeNull();
  });

  it('captures a frame through a <video> and a canvas', async () => {
    const listeners: Record<string, () => void> = {};
    const video = {
      muted: false,
      preload: '',
      crossOrigin: '',
      duration: 100,
      videoWidth: 960,
      videoHeight: 540,
      currentTime: 0,
      addEventListener: (n: string, f: () => void) => (listeners[n] = f),
      removeAttribute: () => undefined,
      load: () => undefined,
      set src(_v: string) {
        queueMicrotask(() => {
          listeners.loadedmetadata!();
          listeners.seeked!();
        });
      },
    };
    const drawn: number[][] = [];
    const canvas = {
      width: 0,
      height: 0,
      getContext: () => ({
        drawImage: (_v: unknown, _x: number, _y: number, w: number, h: number) =>
          drawn.push([w, h]),
      }),
      toBlob: (cb: (b: Blob) => void) => cb(new Blob([JPG], { type: 'image/jpeg' })),
    };
    const doc = {
      createElement: (t: string) => (t === 'video' ? video : canvas),
    } as unknown as Document;
    const blob = await captureFrame('https://b/a.mp4', doc);
    expect(video.currentTime).toBe(10);
    expect(drawn).toEqual([[480, 270]]);
    expect(blob?.type).toBe('image/jpeg');
  });

  it('extracts each file once and two at a time', async () => {
    let live = 0;
    let peak = 0;
    let calls = 0;
    const q = new CoverQueue(2, () => 'blob:x');
    const run = async () => {
      calls += 1;
      live += 1;
      peak = Math.max(peak, live);
      await new Promise((r) => setTimeout(r, 5));
      live -= 1;
      return new Blob(['x']);
    };
    const urls = await Promise.all(['a', 'b', 'c', 'd', 'a'].map((id) => q.get(id, run)));
    expect(urls).toEqual(['blob:x', 'blob:x', 'blob:x', 'blob:x', 'blob:x']);
    expect(calls).toBe(4);
    expect(peak).toBe(2);
    expect(await new CoverQueue(1).get('z', async () => null)).toBeNull();
  });
});

describe('owner edits', () => {
  it('uses the owner title over the file name, and merges the form', () => {
    const notes = noteMap({
      can_edit: true,
      notes: [{ key: 'Films/a.mkv', title: 'My cut', note: 'seen twice' }],
    });
    expect(displayTitle('Films/a.mkv', notes)).toBe('My cut');
    expect(displayTitle('Films/b.mkv', notes)).toBe('b');
    expect(noteFromForm('k', { title: ' T ', note: '', cover: 'c/x.jpg' })).toEqual({
      key: 'k',
      title: 'T',
      cover_key: 'c/x.jpg',
    });
    expect(noteFromForm('k', { title: ' ', note: '', cover: '' })).toBeNull();
  });
  it('applies a saved edit locally and offers only images as covers', () => {
    const base = noteMap({ can_edit: true, notes: [] });
    const set = withNote(base, 'k', { key: 'k', title: 'X' });
    expect(set.get('k')?.title).toBe('X');
    expect(base.size).toBe(0);
    expect(withNote(set, 'k', null).has('k')).toBe(false);
    expect(coverChoices(['a/1.png', 'a/2.mp4', 'a/3.JPG', 'a/4.txt'])).toEqual([
      'a/1.png',
      'a/3.JPG',
    ]);
  });
});

describe('personal rails', () => {
  const o = (key: string, at: number, of: number, updatedAt: number): Opened => ({
    mountId: '1',
    key,
    title: key,
    at,
    of,
    updatedAt,
  });
  it('keeps the newest open first, once per file', () => {
    const l = recordOpened(recordOpened([], o('a.mp4', 5, 100, 1)), o('b.epub', 1, 0, 2));
    expect(recordOpened(l, o('a.mp4', 50, 100, 3)).map((x) => x.key)).toEqual(['a.mp4', 'b.epub']);
  });
  it('splits continue watching from reading and drops finished files', () => {
    const list = [
      o('a.mp4', 5, 100, 4),
      o('done.mp4', 99, 100, 3),
      o('b.cbz', 3, 0, 2),
      o('c.mp3', 1, 200, 1),
      o('x.txt', 0, 0, 0),
    ];
    const r = continueRails(list);
    expect(r.watching.map((x) => x.key)).toEqual(['a.mp4', 'c.mp3']);
    expect(r.reading.map((x) => x.key)).toEqual(['b.cbz']);
    expect(isFinished(o('d.mp4', 95, 100, 0))).toBe(true);
  });
  it('toggles pinned folders', () => {
    const p = { mountId: '1', prefix: 'Films/', label: 'Films' };
    const on = togglePin([], p);
    expect(on).toEqual([p]);
    expect(togglePin(on, p)).toEqual([]);
  });
  it('lists the newest additions across shelves, skipping untyped files', () => {
    const r = recentlyAdded([
      {
        mountId: '1',
        objects: [
          { key: 'a.mp4', size: 1, last_modified: '2026-10-01T00:00:00Z' },
          { key: 'n.txt', size: 1, last_modified: '2026-12-01T00:00:00Z' },
        ],
      },
      { mountId: '2', objects: [{ key: 'b.cbz', size: 1, last_modified: '2026-10-05T00:00:00Z' }] },
    ]);
    expect(r.map((x) => `${x.mountId}:${x.key}`)).toEqual(['2:b.cbz', '1:a.mp4']);
  });
});
