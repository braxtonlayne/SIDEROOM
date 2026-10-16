// Client-side pre-checks before a chapter upload (LIB-ING-009). The archive's table of contents is read in
// WebAssembly (`sr-library-wasm`, judged by the same rules as the server's archive reader), and the first and
// last page are inflated and decoded by the browser to catch a broken image early. Nothing here replaces the
// server's checks; it only saves a long upload that was going to be refused.
import type { ArchivePrecheck, LibraryApi, ZipProbe } from '@sideroom/wasm';
import { t } from '../i18n';

/** The server decodes up to this many pixels per page (`PageLimits`). */
export const MAX_PAGE_PIXELS = 60_000_000;
/** Pages bigger than this are not inflated in the browser to be probed. */
const MAX_PROBE_BYTES = 64 * 1024 * 1024;
const MAX_TAIL_ROUNDS = 3;

export type Precheck =
  { ok: true; pages?: number; ignored?: number } | { ok: false; message: string };

let loaded: Promise<LibraryApi> | null = null;
let injected: LibraryApi | null = null;

/** Tests inject a stand-in for the WebAssembly module. */
export function setLibraryChecker(api: LibraryApi | null) {
  injected = api;
}

async function lib(): Promise<LibraryApi | null> {
  if (injected) return injected;
  loaded ??= import('@sideroom/wasm').then((m) => m.loadLibrary());
  try {
    return await loaded;
  } catch {
    loaded = null;
    return null; // no pre-check, the server still checks everything
  }
}

/** The bytes of a Blob; older engines (and the test DOM) lack `Blob.arrayBuffer`. */
const buffer = (blob: Blob): Promise<ArrayBuffer> =>
  typeof blob.arrayBuffer === 'function'
    ? blob.arrayBuffer()
    : new Promise((resolve, reject) => {
        const r = new FileReader();
        r.onload = () => resolve(r.result as ArrayBuffer);
        r.onerror = () => reject(r.error);
        r.readAsArrayBuffer(blob);
      });
const bytes = async (blob: Blob) => new Uint8Array(await buffer(blob));

function refusal(r: Extract<ArchivePrecheck, { ok: false }>): string {
  const files = r.error.files.join(', ');
  switch (r.error.kind) {
    case 'avif':
      return t('library.precheck.avif', { files });
    case 'unsafe_path':
      return t('library.precheck.unsafePath', { files });
    case 'encrypted':
      return t('library.precheck.encrypted', { files });
    case 'entry_too_large':
      return t('library.precheck.entryTooLarge', { files });
    case 'suspicious_ratio':
      return t('library.precheck.suspiciousRatio', { files });
    case 'too_many_entries':
      return t('library.precheck.tooManyEntries');
    case 'too_large':
      return t('library.precheck.tooLarge');
    case 'no_pages':
      return t('library.precheck.noPages');
    case 'not_an_archive':
      return t('library.precheck.notAnArchive');
    default:
      return t('library.precheck.unreadable');
  }
}

/** Inflate one stored or deflated ZIP entry; `null` when it cannot be probed here. */
export async function readZipEntry(file: Blob, p: ZipProbe): Promise<Uint8Array | null> {
  if (p.size > MAX_PROBE_BYTES || (p.method !== 0 && p.method !== 8)) return null;
  const local = new DataView(await buffer(file.slice(p.local_offset, p.local_offset + 30)));
  if (local.byteLength < 30 || local.getUint32(0, true) !== 0x04034b50) return null;
  const start = p.local_offset + 30 + local.getUint16(26, true) + local.getUint16(28, true);
  const raw = file.slice(start, start + p.compressed);
  if (p.method === 0) return bytes(raw);
  if (typeof DecompressionStream === 'undefined') return null;
  const stream = raw.stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Decode a page to learn that it is an image and how big. `null` when the browser cannot tell. */
async function decodes(
  data: Uint8Array,
): Promise<{ width: number; height: number } | null | false> {
  if (typeof createImageBitmap === 'undefined') return null;
  try {
    const bmp = await createImageBitmap(new Blob([data as BlobPart]));
    const size = { width: bmp.width, height: bmp.height };
    bmp.close?.();
    return size;
  } catch {
    return false;
  }
}

/**
 * Check a CBZ/ZIP (other containers pass through). `maxBytes` is the operator's upload limit when known.
 * Never throws: a check that cannot run lets the file through.
 */
export async function precheckArchive(file: File, maxBytes = 0): Promise<Precheck> {
  const api = await lib();
  if (!api) return { ok: true };
  try {
    const head = await bytes(file.slice(0, 64));
    let want = Math.min(file.size, api.firstTail);
    for (let round = 0; round < MAX_TAIL_ROUNDS; round++) {
      const tail = await bytes(file.slice(file.size - want));
      const r = api.precheck(head, tail, file.size, maxBytes);
      if ('need_tail' in r) {
        want = Math.min(file.size, Math.max(r.need_tail, want + 1));
        continue;
      }
      if (!r.ok) return { ok: false, message: refusal(r) };
      if (!r.listed) return { ok: true };
      // The first and last pages are the likeliest to be a cover or a stray file: try to decode them.
      for (const probe of [r.first, r.last]) {
        const data = await readZipEntry(file, probe).catch(() => null);
        if (!data) continue;
        const d = await decodes(data);
        if (d === false)
          return { ok: false, message: t('library.precheck.badImage', { name: probe.name }) };
        if (d && d.width * d.height > MAX_PAGE_PIXELS)
          return { ok: false, message: t('library.precheck.hugeImage', { name: probe.name }) };
        if (probe.name === r.last.name && r.first.name === r.last.name) break;
      }
      return { ok: true, pages: r.pages, ignored: r.ignored };
    }
    return { ok: true };
  } catch {
    return { ok: true };
  }
}

/** Whether a file name looks like a ZIP/CBZ, the only container the browser can list. */
export const isZipName = (name: string) => /\.(cbz|zip)$/i.test(name);
