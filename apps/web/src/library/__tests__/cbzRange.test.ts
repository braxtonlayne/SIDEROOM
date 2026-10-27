import type { ZipProbe } from '@sideroom/wasm';
import { CbzError, openCbz, type PageLister } from '../reader/cbzRange';

const enc = new TextEncoder();

/** A ZIP with the given entries (stored, or deflated when `deflated` bytes are supplied) and the probes for them. */
function buildZip(entries: { name: string; data: Uint8Array; packed?: Uint8Array }[]): {
  bytes: Uint8Array;
  probes: ZipProbe[];
} {
  const parts: Uint8Array[] = [];
  const probes: ZipProbe[] = [];
  let at = 0;
  for (const e of entries) {
    const name = enc.encode(e.name);
    const body = e.packed ?? e.data;
    const header = new Uint8Array(30 + name.length);
    const dv = new DataView(header.buffer);
    dv.setUint32(0, 0x04034b50, true);
    dv.setUint16(8, e.packed ? 8 : 0, true);
    dv.setUint32(18, body.length, true);
    dv.setUint32(22, e.data.length, true);
    dv.setUint16(26, name.length, true);
    header.set(name, 30);
    probes.push({
      name: e.name,
      local_offset: at,
      compressed: body.length,
      size: e.data.length,
      method: e.packed ? 8 : 0,
    });
    parts.push(header, body);
    at += header.length + body.length;
  }
  // A stand-in central directory: the lister is faked, only its length matters.
  parts.push(new Uint8Array(300).fill(7));
  const bytes = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    bytes.set(p, o);
    o += p.length;
  }
  return { bytes, probes };
}

/** `fetch` over a byte array that understands `Range: bytes=a-b` and `bytes=-n`. */
function serve(bytes: Uint8Array) {
  const ranges: string[] = [];
  const f = async (_url: string, init?: RequestInit) => {
    const spec = (init?.headers as Record<string, string> | undefined)?.Range ?? '';
    ranges.push(spec);
    const m = /^bytes=(\d*)-(\d*)$/.exec(spec);
    if (!m) return new Response(bytes as BlobPart, { status: 200 });
    const [a, b] = [m[1]!, m[2]!];
    const start = a === '' ? Math.max(0, bytes.length - Number(b)) : Number(a);
    const end = a === '' ? bytes.length - 1 : Math.min(bytes.length - 1, Number(b));
    return new Response(bytes.slice(start, end + 1) as BlobPart, {
      status: 206,
      headers: { 'Content-Range': `bytes ${start}-${end}/${bytes.length}` },
    });
  };
  return { f, ranges };
}

const text = (b: Blob) =>
  new Promise<string>((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error);
    r.readAsText(b);
  });

describe('CBZ by range (BYOS-CAT-002)', () => {
  it('reads the directory from the tail with one request and each page with one more', async () => {
    const { bytes, probes } = buildZip([
      { name: '1.jpg', data: enc.encode('page-one') },
      { name: '2.jpg', data: enc.encode('page-two!') },
      { name: '3.png', data: enc.encode('page-three') },
    ]);
    const { f, ranges } = serve(bytes);
    const lister: PageLister = {
      firstTail: 128,
      listPages: (tail, len) => {
        expect(tail.length).toBe(128);
        expect(len).toBe(bytes.length);
        return { ok: true, pages: probes };
      },
    };
    const book = await openCbz('https://b/x.cbz', lister, f);
    expect(book.pages.map((p) => p.name)).toEqual(['1.jpg', '2.jpg', '3.png']);
    expect(ranges).toEqual(['bytes=-128']);
    const p2 = await book.page(1);
    expect(await text(p2)).toBe('page-two!');
    expect(p2.type).toBe('image/jpeg');
    expect((await book.page(2)).type).toBe('image/png');
    expect(ranges).toHaveLength(3);
    expect(ranges[1]).toMatch(/^bytes=\d+-\d+$/);
    // A page asked for twice is fetched once.
    await book.page(1);
    expect(ranges).toHaveLength(3);
  });

  it('reads more of the tail when the directory is longer than the first guess', async () => {
    const { bytes, probes } = buildZip([{ name: 'a.jpg', data: enc.encode('x') }]);
    const { f, ranges } = serve(bytes);
    let calls = 0;
    const lister: PageLister = {
      firstTail: 50,
      listPages: () => (++calls === 1 ? { need_tail: 200 } : { ok: true, pages: probes }),
    };
    const book = await openCbz('https://b/x.cbz', lister, f);
    expect(book.pages).toHaveLength(1);
    expect(ranges).toEqual(['bytes=-50', 'bytes=-200']);
  });

  it('refuses what the lister refuses and a server that does not do ranges', async () => {
    const { bytes } = buildZip([{ name: 'a.jpg', data: enc.encode('x') }]);
    const { f } = serve(bytes);
    await expect(
      openCbz(
        'https://b/x.cbz',
        { firstTail: 10, listPages: () => ({ ok: false, message: 'no pages' }) },
        f,
      ),
    ).rejects.toThrow(CbzError);
    const forbidden = async () => new Response('no', { status: 403 });
    await expect(
      openCbz('https://b/x.cbz', { firstTail: 10, listPages: () => ({ need_tail: 5 }) }, forbidden),
    ).rejects.toThrow(/range 403/);
  });

  it('does not trust a page whose local header is not one', async () => {
    const { bytes, probes } = buildZip([{ name: 'a.jpg', data: enc.encode('hello') }]);
    const { f } = serve(bytes);
    const lie: ZipProbe = { ...probes[0]!, local_offset: 3 };
    const book = await openCbz(
      'https://b/x.cbz',
      { firstTail: 10, listPages: () => ({ ok: true, pages: [lie] }) },
      f,
    );
    await expect(book.page(0)).rejects.toThrow(/bad local header/);
    await expect(book.page(5)).rejects.toThrow(/no such page/);
  });

  it('inflates a deflated page when the browser can', async () => {
    if (typeof CompressionStream === 'undefined' || typeof DecompressionStream === 'undefined')
      return;
    const raw = enc.encode('deflate me '.repeat(50));
    const packed = new Uint8Array(
      await new Response(
        new Response(raw as BodyInit).body!.pipeThrough(new CompressionStream('deflate-raw')),
      ).arrayBuffer(),
    );
    const { bytes, probes } = buildZip([{ name: 'a.webp', data: raw, packed }]);
    const { f } = serve(bytes);
    const book = await openCbz(
      'https://b/x.cbz',
      { firstTail: 10, listPages: () => ({ ok: true, pages: probes }) },
      f,
    );
    const page = await book.page(0);
    expect(page.type).toBe('image/webp');
    expect(page.size).toBe(raw.length);
  });
});
