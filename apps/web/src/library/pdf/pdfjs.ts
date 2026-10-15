/**
 * pdf.js integration (G-59, P11-LIB-2, D-190 context): PDFs are read and their cover rendered in the browser,
 * the server never rasterizes (docs/open-question-answers.md P11-LIB-2).
 *
 * `pdfjs-dist` is an allowed component (doc 03 section 3) but is NOT installed in this tree yet (adding a
 * dependency means a download, which needs the owner). The library is therefore loaded through
 * `import.meta.glob`, which matches nothing while the package is absent and so builds cleanly; every caller
 * gets a `PdfNotInstalledError` and shows a "not installed" state. Once `pdfjs-dist` is added to
 * `apps/web/package.json` and the lockfile, the glob matches and nothing else changes.
 */

/** The slice of the pdf.js API used here, so tests can supply a stand-in. */
export interface PdfViewportLike {
  width: number;
  height: number;
}
export interface PdfPageLike {
  getViewport(o: { scale: number }): PdfViewportLike;
  render(o: { canvasContext: CanvasRenderingContext2D; viewport: PdfViewportLike }): {
    promise: Promise<void>;
  };
}
export interface PdfDocumentLike {
  numPages: number;
  getPage(n: number): Promise<PdfPageLike>;
  destroy?(): Promise<void> | void;
}
export interface PdfjsLike {
  getDocument(src: { url?: string; data?: ArrayBuffer }): { promise: Promise<PdfDocumentLike> };
  GlobalWorkerOptions?: { workerSrc: string };
}

export class PdfNotInstalledError extends Error {
  constructor() {
    super('pdfjs-dist is not installed');
    this.name = 'PdfNotInstalledError';
  }
}

const libs = import.meta.glob('/node_modules/pdfjs-dist/build/pdf.min.mjs') as Record<
  string,
  () => Promise<unknown>
>;
const workers = import.meta.glob('/node_modules/pdfjs-dist/build/pdf.worker.min.mjs', {
  query: '?url',
  import: 'default',
}) as Record<string, () => Promise<string>>;

let override: (() => Promise<PdfjsLike>) | null = null;
let cached: Promise<PdfjsLike> | null = null;

/** Test hook: supply a stand-in for pdf.js (or null to restore the real loader). */
export function __setPdfjsForTests(loader: (() => Promise<PdfjsLike>) | null) {
  override = loader;
  cached = null;
}

/** Whether the build found `pdfjs-dist` (no network, no import). */
export const pdfjsInstalled = () => override !== null || Object.keys(libs).length > 0;

export function loadPdfjs(): Promise<PdfjsLike> {
  if (override) return override();
  const load = Object.values(libs)[0];
  if (!load) return Promise.reject(new PdfNotInstalledError());
  cached ??= (async () => {
    const lib = (await load()) as PdfjsLike;
    const worker = Object.values(workers)[0];
    if (worker && lib.GlobalWorkerOptions) lib.GlobalWorkerOptions.workerSrc = await worker();
    return lib;
  })();
  return cached;
}

export async function openPdf(
  src: { url: string } | { data: ArrayBuffer },
): Promise<PdfDocumentLike> {
  const lib = await loadPdfjs();
  return lib.getDocument(src).promise;
}

/** Draws page `n` (1-based) of `doc` into `canvas`, scaled so it is `width` CSS pixels wide. */
export async function renderPdfPage(
  doc: PdfDocumentLike,
  n: number,
  canvas: HTMLCanvasElement,
  width: number,
): Promise<void> {
  const page = await doc.getPage(n);
  const base = page.getViewport({ scale: 1 });
  const scale = base.width > 0 ? width / base.width : 1;
  const viewport = page.getViewport({ scale });
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('canvas 2d unavailable');
  canvas.width = Math.ceil(viewport.width);
  canvas.height = Math.ceil(viewport.height);
  await page.render({ canvasContext: ctx, viewport }).promise;
}

/**
 * Page 1 of a PDF as a JPEG for the cover (about `width` pixels wide). Used at upload time; the file never
 * leaves the browser for this. Rejects with PdfNotInstalledError while pdf.js is absent.
 */
function readBytes(file: Blob): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === 'function') return file.arrayBuffer();
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as ArrayBuffer);
    r.onerror = () => reject(r.error ?? new Error('read failed'));
    r.readAsArrayBuffer(file);
  });
}

export async function renderPdfCover(file: Blob, width = 600): Promise<Blob> {
  await loadPdfjs(); // fail fast (PdfNotInstalledError) before reading the file
  const doc = await openPdf({ data: await readBytes(file) });
  try {
    const canvas = document.createElement('canvas');
    await renderPdfPage(doc, 1, canvas, width);
    return await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (b) => (b ? resolve(b) : reject(new Error('cover encode failed'))),
        'image/jpeg',
        0.85,
      ),
    );
  } finally {
    await doc.destroy?.();
  }
}
