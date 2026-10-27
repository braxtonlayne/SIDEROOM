// Client-side upload pre-checks (LIB-ING-009): the WASM listing is stubbed (it is tested natively and in
// packages/wasm); this covers the file reading, the retry for a longer tail, the decode probe and the messages.
import type { ArchivePrecheck, LibraryApi } from '@sideroom/wasm';
import { isZipName, precheckArchive, readZipEntry, setLibraryChecker } from '../precheck';

const probe = (name: string, local_offset = 0, size = 4) => ({
  name,
  local_offset,
  compressed: size,
  size,
  method: 0,
});

const listed = (
  over: Partial<Extract<ArchivePrecheck, { listed: true }>> = {},
): ArchivePrecheck => ({
  ok: true,
  container: 'zip',
  listed: true,
  pages: 2,
  total_bytes: 8,
  largest_bytes: 4,
  first_names: ['a.jpg', 'b.jpg'],
  comic_info: false,
  series_json: false,
  ignored: 1,
  first: probe('a.jpg'),
  last: probe('b.jpg', 34),
  ...over,
});

function stub(answers: ArchivePrecheck[]): LibraryApi & { calls: number[] } {
  const calls: number[] = [];
  return {
    calls,
    firstTail: 16,
    listPages: () => ({ ok: false, message: 'not used' }),
    defaults: { maxArchive: 0, maxEntries: 0, maxEntry: 0, maxTotal: 0 },
    precheck: (_h, tail) => {
      calls.push(tail.length);
      return answers[Math.min(calls.length - 1, answers.length - 1)]!;
    },
  };
}

/** A file with two stored entries at the offsets `listed()` reports (30-byte header + name + data). */
function cbz(): File {
  const out = new Uint8Array(200).fill(1);
  const dv = new DataView(out.buffer);
  for (const at of [0, 34]) {
    dv.setUint32(at, 0x04034b50, true);
    dv.setUint16(at + 26, 0, true);
    dv.setUint16(at + 28, 0, true);
  }
  return new File([out], 'one.cbz');
}

afterEach(() => {
  setLibraryChecker(null);
  vi.unstubAllGlobals();
});

describe('name check', () => {
  it('only ZIP-like names can be listed in the browser', () => {
    expect(isZipName('Chapter 1.CBZ')).toBe(true);
    expect(isZipName('a.zip')).toBe(true);
    expect(isZipName('a.cb7')).toBe(false);
    expect(isZipName('a.rar')).toBe(false);
  });
});

describe('precheckArchive', () => {
  it('lets a good archive through and decodes the first and last page', async () => {
    setLibraryChecker(stub([listed()]));
    const decoded: number[] = [];
    vi.stubGlobal('createImageBitmap', async (b: Blob) => {
      decoded.push(b.size);
      return { width: 1000, height: 1500, close() {} };
    });
    expect(await precheckArchive(cbz())).toEqual({ ok: true, pages: 2, ignored: 1 });
    expect(decoded).toEqual([4, 4]);
  });

  it('turns a refusal into a message that names the files', async () => {
    setLibraryChecker(
      stub([
        {
          ok: false,
          error: { kind: 'avif', message: 'x', files: ['a.avif', 'b.avif'] },
        },
      ]),
    );
    const r = await precheckArchive(cbz());
    expect(r).toEqual({
      ok: false,
      message: 'AVIF pages are not supported: a.avif, b.avif. Convert them to JPEG, PNG or WebP.',
    });
  });

  it('asks again with the longer tail the module wants', async () => {
    const api = stub([{ need_tail: 100 }, listed()]);
    setLibraryChecker(api);
    vi.stubGlobal('createImageBitmap', undefined);
    expect((await precheckArchive(cbz())).ok).toBe(true);
    expect(api.calls).toEqual([16, 100]);
  });

  it('refuses a page the browser cannot decode, or one over the pixel limit', async () => {
    setLibraryChecker(stub([listed()]));
    vi.stubGlobal('createImageBitmap', async () => {
      throw new Error('bad');
    });
    expect(await precheckArchive(cbz())).toEqual({
      ok: false,
      message: 'The page a.jpg is not a readable image.',
    });
    vi.stubGlobal('createImageBitmap', async () => ({ width: 10_000, height: 10_000, close() {} }));
    const r = await precheckArchive(cbz());
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.message).toContain('60 megapixel');
  });

  it('passes other containers through and never throws', async () => {
    setLibraryChecker(stub([{ ok: true, container: '7z', listed: false }]));
    expect(await precheckArchive(cbz())).toEqual({ ok: true });
    setLibraryChecker({
      ...stub([]),
      precheck: () => {
        throw new Error('wasm trapped');
      },
    });
    expect(await precheckArchive(cbz())).toEqual({ ok: true });
  });
});

describe('readZipEntry', () => {
  it('reads a stored entry and skips ones it cannot probe', async () => {
    const f = cbz();
    expect((await readZipEntry(f, probe('a.jpg')))?.length).toBe(4);
    expect(await readZipEntry(f, { ...probe('a.jpg'), method: 14 })).toBeNull();
    expect(await readZipEntry(f, { ...probe('a.jpg'), size: 100 * 1024 * 1024 })).toBeNull();
    // Not a local header at that offset.
    expect(await readZipEntry(f, probe('a.jpg', 100))).toBeNull();
  });
});
