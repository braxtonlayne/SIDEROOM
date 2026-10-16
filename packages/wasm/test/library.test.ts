import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadLibrary } from '../src/index.js';

/**
 * LIB-ING-009: the browser reads a CBZ's central directory in WASM and judges it with the same
 * rules as the server (crates/sr-library-rules, tested natively and against the server's archive
 * reader in crates/sr-library-wasm). This checks the bridge. Needs `pnpm build` first.
 * Set SR_WASM_REQUIRED=1 (wasm.yml does) to fail instead of skipping when the build is missing.
 */
const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(here, '../dist/library/library_bg.wasm');
const built = existsSync(wasmPath);

if (!built && process.env.SR_WASM_REQUIRED === '1') {
  throw new Error(`missing ${wasmPath}; run pnpm --filter @sideroom/wasm build first`);
}

/** A ZIP with stored (uncompressed) entries. */
function zip(files: [string, number][]): Uint8Array {
  const parts: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  const enc = new TextEncoder();
  for (const [name, len] of files) {
    const n = enc.encode(name);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true);
    local.setUint32(18, len, true);
    local.setUint32(22, len, true);
    local.setUint16(26, n.length, true);
    const body = new Uint8Array(len).fill(7);
    parts.push(new Uint8Array(local.buffer), n, body);
    const cd = new DataView(new ArrayBuffer(46));
    cd.setUint32(0, 0x02014b50, true);
    cd.setUint16(4, 20, true);
    cd.setUint16(6, 20, true);
    cd.setUint32(20, len, true);
    cd.setUint32(24, len, true);
    cd.setUint16(28, n.length, true);
    cd.setUint32(42, offset, true);
    central.push(new Uint8Array(cd.buffer), n);
    offset += 30 + n.length + len;
  }
  const cdSize = central.reduce((a, c) => a + c.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, files.length, true);
  end.setUint16(10, files.length, true);
  end.setUint32(12, cdSize, true);
  end.setUint32(16, offset, true);
  const all = [...parts, ...central, new Uint8Array(end.buffer)];
  const out = new Uint8Array(all.reduce((a, c) => a + c.length, 0));
  let at = 0;
  for (const p of all) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

describe.runIf(built)('sr-library-wasm pre-checks', () => {
  it('lists pages like the server and reports where to probe', async () => {
    const lib = await loadLibrary(readFileSync(wasmPath));
    const z = zip([
      ['c/10.jpg', 30],
      ['c/2.png', 20],
      ['__MACOSX/._2.png', 4],
      ['c/1.webp', 10],
    ]);
    const r = lib.precheck(z.slice(0, 64), z, z.length);
    expect(r).toMatchObject({ ok: true, container: 'zip', pages: 3, ignored: 1, total_bytes: 60 });
    if ('first_names' in r) expect(r.first_names).toEqual(['c/1.webp', 'c/2.png', 'c/10.jpg']);
    if ('first' in r) expect(r.first.name).toBe('c/1.webp');
  });

  it('asks for more of the tail, then answers', async () => {
    const lib = await loadLibrary(readFileSync(wasmPath));
    const z = zip(
      Array.from({ length: 40 }, (_, i) => [`page-${String(i).padStart(3, '0')}.jpg`, 50]),
    );
    const first = lib.precheck(z.slice(0, 64), z.slice(z.length - 100), z.length);
    expect('need_tail' in first).toBe(true);
    const need = 'need_tail' in first ? first.need_tail : 0;
    const again = lib.precheck(z.slice(0, 64), z.slice(z.length - need), z.length);
    expect(again).toMatchObject({ ok: true, pages: 40 });
  });

  it('refuses what the server refuses, with the offending names', async () => {
    const lib = await loadLibrary(readFileSync(wasmPath));
    const avif = zip([
      ['a.avif', 5],
      ['b.jpg', 5],
    ]);
    const r = lib.precheck(avif.slice(0, 64), avif, avif.length);
    expect(r).toMatchObject({ ok: false, error: { kind: 'avif', files: ['a.avif'] } });
    const evil = zip([['../evil.jpg', 5]]);
    expect(lib.precheck(evil.slice(0, 64), evil, evil.length)).toMatchObject({
      ok: false,
      error: { kind: 'unsafe_path' },
    });
    const text = zip([['notes.txt', 5]]);
    expect(lib.precheck(text.slice(0, 64), text, text.length)).toMatchObject({
      ok: false,
      error: { kind: 'no_pages' },
    });
    expect(lib.precheck(new TextEncoder().encode('%PDF-1.7'), new Uint8Array(8), 8)).toMatchObject({
      ok: false,
      error: { kind: 'not_an_archive' },
    });
    // The operator's upload limit.
    const small = zip([['a.jpg', 5]]);
    expect(lib.precheck(small.slice(0, 64), small, small.length, 10)).toMatchObject({
      ok: false,
      error: { kind: 'too_large' },
    });
    // 7z and RAR are accepted on sight; the server reads them in full.
    expect(
      lib.precheck(
        new Uint8Array([0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c, 0, 0]),
        new Uint8Array(0),
        100,
      ),
    ).toMatchObject({ ok: true, container: '7z', listed: false });
  });
});
