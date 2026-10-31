import {
  BIOS_PREFIX,
  BIOS_SIZE,
  biosSourceAllowed,
  checkBios,
  fetchBios,
  KNOWN_PS1_BIOS,
  type BiosEntry,
} from '../bios';
import { md5Hex, sha1Hex } from '../digest';

const bytes = (fill: number, n = BIOS_SIZE) => new Uint8Array(n).fill(fill);

describe('digests', () => {
  it('match the published test vectors', async () => {
    const abc = new TextEncoder().encode('abc');
    expect(md5Hex(abc)).toBe('900150983cd24fb0d6963f7d28e17f72');
    expect(md5Hex(new Uint8Array(0))).toBe('d41d8cd98f00b204e9800998ecf8427e');
    expect(await sha1Hex(abc)).toBe('a9993e364706816aba3e25717850c26c9cd0d89d');
    const fox = new TextEncoder().encode('The quick brown fox jumps over the lazy dog');
    expect(md5Hex(fox)).toBe('9e107d9d372bb6826bd81d3542a419d6');
  });
});

describe('the BIOS list', () => {
  it('holds only well-formed entries and no PS2 images', () => {
    expect(KNOWN_PS1_BIOS.length).toBeGreaterThan(15);
    for (const e of KNOWN_PS1_BIOS) {
      expect(e.md5).toMatch(/^[0-9a-f]{32}$/);
      expect(e.sha1 === undefined || /^[0-9a-f]{40}$/.test(e.sha1)).toBe(true);
      expect(e.file).toMatch(/^scph\d+w?r?\.bin$/);
    }
    expect(new Set(KNOWN_PS1_BIOS.map((e) => e.md5)).size).toBe(KNOWN_PS1_BIOS.length);
  });
});

describe('checkBios', () => {
  const data = bytes(7);
  const own = (): BiosEntry[] => [
    { file: 'scph1001.bin', region: 'NTSC-U', description: 'test image', md5: md5Hex(data) },
  ];
  it('accepts a 512 KiB file whose digest is on the list, by MD5 or by SHA-1', async () => {
    expect((await checkBios(data, own())).ok).toBe(true);
    const sha1 = await sha1Hex(data);
    const entry: BiosEntry = {
      file: 'x.bin',
      region: 'PAL',
      description: 'd',
      md5: '0'.repeat(32),
      sha1,
    };
    expect((await checkBios(data, [entry])).ok).toBe(true);
  });
  it('refuses an unknown file, and tells the member what it is', async () => {
    const v = await checkBios(bytes(9), own());
    expect(v.ok).toBe(false);
    if (!v.ok) {
      expect(v.reason).toBe('unknown');
      expect(v.sha1).toMatch(/^[0-9a-f]{40}$/);
      expect(v.md5).toMatch(/^[0-9a-f]{32}$/);
    }
  });
  it('refuses the wrong size before hashing', async () => {
    expect(await checkBios(bytes(7, 1024), own())).toEqual({ ok: false, reason: 'size' });
    expect(await checkBios(bytes(7, BIOS_SIZE + 1), own())).toEqual({ ok: false, reason: 'size' });
  });
  it('refuses a zero-filled image against the shipped list', async () => {
    expect((await checkBios(bytes(0))).ok).toBe(false);
  });
});

describe('where a BIOS may come from', () => {
  const src = (over = {}) => ({
    mountOwnerKind: 'user',
    mountOwnerId: '42',
    key: `${BIOS_PREFIX}scph1001.bin`,
    ...over,
  });
  it('is emu/bios/ of a mount the member owns', () => {
    expect(biosSourceAllowed(src(), '42')).toBe(true);
  });
  it('is never a server shelf, the instance bucket, someone else, or another folder', () => {
    expect(biosSourceAllowed(src({ mountOwnerKind: 'guild' }), '42')).toBe(false);
    expect(biosSourceAllowed(src({ mountOwnerKind: 'platform', mountOwnerId: null }), '42')).toBe(
      false,
    );
    expect(biosSourceAllowed(src(), '43')).toBe(false);
    expect(biosSourceAllowed(src({ key: 'emu/roms/scph1001.bin' }), '42')).toBe(false);
    expect(biosSourceAllowed(src({ key: 'emu/bios/../roms/x.bin' }), '42')).toBe(false);
    expect(biosSourceAllowed(src({ key: BIOS_PREFIX }), '42')).toBe(false);
  });
});

describe('fetchBios keeps nothing', () => {
  const data = bytes(5);
  const list: BiosEntry[] = [
    { file: 'scph5501.bin', region: 'NTSC-U', description: 'd', md5: md5Hex(data) },
  ];
  const mk = (served: Uint8Array) => {
    const seen: Array<RequestInit | undefined> = [];
    return {
      seen,
      io: {
        presignGet: async () => ({ url: 'https://bucket.example/b?sig=1' }),
        fetch: (async (_u: string, init?: RequestInit) => {
          seen.push(init);
          return new Response(served as BlobPart);
        }) as typeof fetch,
      },
    };
  };
  const own = (key: string) => ({ mountOwnerKind: 'user', mountOwnerId: '42', key });

  it('reads with no-store and no credentials, hands back verified bytes only, and writes no storage', async () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    const { io, seen } = mk(data);
    const r = await fetchBios('7', own('emu/bios/scph5501.bin'), '42', io, list);
    expect(r.verdict.ok).toBe(true);
    expect(r.bytes?.length).toBe(BIOS_SIZE);
    expect(seen[0]).toMatchObject({ cache: 'no-store', credentials: 'omit' });
    expect(setItem).not.toHaveBeenCalled();
    setItem.mockRestore();
  });
  it('drops the bytes of a refused file and never asks the bucket for a forbidden source', async () => {
    const { io, seen } = mk(bytes(1));
    const bad = await fetchBios('7', own('emu/bios/x.bin'), '42', io, list);
    expect(bad.verdict.ok).toBe(false);
    expect(bad.bytes).toBeNull();
    const shelf = { mountOwnerKind: 'guild', mountOwnerId: '5', key: 'emu/bios/x.bin' };
    const refused = await fetchBios('9', shelf, '42', io, list);
    expect(refused.verdict).toEqual({ ok: false, reason: 'source' });
    expect(seen.length).toBe(1);
  });
});
