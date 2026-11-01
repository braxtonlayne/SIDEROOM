// Export one page as a PNG. The page is painted on an off-screen canvas at its full 1600 x 900, with a white
// background, once every image on it has loaded.

import { PAGE_H, PAGE_W, type DrawObject } from './log';
import { drawPage, type ImageCache } from './render';

export async function renderPagePng(
  objects: DrawObject[],
  pageId: string,
  images: ImageCache,
): Promise<Blob> {
  const canvas = document.createElement('canvas');
  canvas.width = PAGE_W;
  canvas.height = PAGE_H;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('canvas');
  // Let pending images settle (bounded), repainting each pass.
  for (let pass = 0; pass < 20; pass++) {
    drawPage(ctx, objects, pageId, images, () => undefined);
    const pending = objects.some(
      (o) => o.pg === pageId && o.k === 'image' && images.get(o.src) === 'loading',
    );
    if (!pending) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  return new Promise((resolve, reject) => {
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('png'))), 'image/png');
  });
}

/**
 * How a sandboxed Activity saves a file: the host page asks the person and downloads it (the frame has no
 * download or popup right). Set by main.ts once the SDK is ready.
 */
let hostSave: ((name: string, bytes: Uint8Array) => Promise<unknown>) | null = null;
export function setHostSave(fn: (name: string, bytes: Uint8Array) => Promise<unknown>): void {
  hostSave = fn;
}

/** Hand the PNG to the person: through the host when sandboxed, a download when top-level. */
export function deliverPng(blob: Blob, name: string): void {
  if (hostSave) {
    const save = hostSave;
    void blob.arrayBuffer().then((b) => save(name, new Uint8Array(b)));
    return;
  }
  const url = URL.createObjectURL(blob);
  if (window.parent === window) {
    const a = document.createElement('a');
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    a.remove();
  } else {
    window.open(url, '_blank', 'noopener');
  }
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
