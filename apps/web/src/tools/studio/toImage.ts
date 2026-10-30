/**
 * UTL-IMG-001: render a canvas or a DOM node to a PNG blob. This is the API for "post as image" (the tier list
 * of utl-collab) and any other tool that wants a picture of itself. Nothing leaves the page: the blob is made
 * locally and the caller decides what to do with it (usually `File` -> the composer's upload path).
 *
 *   import { toPngBlob } from '../studio/toImage';
 *   const blob = await toPngBlob(tierListElement, { scale: 2, background: '#16161a' });
 *   const file = new File([blob], 'tier-list.png', { type: 'image/png' });
 *
 * A DOM node is rasterised through an SVG `foreignObject` with its computed styles inlined, so it needs no
 * library. Limits: images inside the node must be same-origin or `data:` URLs (a cross-origin image taints the
 * canvas and the call rejects with `tainted`), web fonts are used only if already loaded, and `<canvas>`
 * elements inside the node are copied as their current bitmap.
 */

export interface ToPngOptions {
  /** Device pixels per CSS pixel for a DOM node (default 2, capped so the longest edge is 4096). */
  scale?: number;
  /** CSS colour painted behind a DOM node (default transparent). */
  background?: string;
}

type Source = HTMLCanvasElement | OffscreenCanvas | Element;

const MAX_EDGE = 4096;

function blobOf(canvas: HTMLCanvasElement | OffscreenCanvas): Promise<Blob> {
  if ('convertToBlob' in canvas) return canvas.convertToBlob({ type: 'image/png' });
  return new Promise((resolve, reject) => {
    try {
      canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('encode-failed'))), 'image/png');
    } catch {
      reject(new Error('tainted'));
    }
  });
}

/** Inline the computed style of `from` and its descendants onto the matching nodes of `to`. */
function inlineStyles(from: Element, to: Element): void {
  const cs = getComputedStyle(from);
  let css = '';
  for (let i = 0; i < cs.length; i++) {
    const p = cs.item(i);
    css += `${p}:${cs.getPropertyValue(p)};`;
  }
  to.setAttribute('style', css);
  if (from instanceof HTMLCanvasElement && to instanceof HTMLCanvasElement) {
    // A cloned canvas is blank; swap it for an <img> of the current bitmap.
    try {
      const img = document.createElement('img');
      img.src = from.toDataURL('image/png');
      img.setAttribute('style', css);
      to.replaceWith(img);
    } catch {
      /* tainted canvas: leave it blank */
    }
    return;
  }
  if (from instanceof HTMLInputElement && to instanceof HTMLInputElement) {
    to.setAttribute('value', from.value);
  }
  const a = Array.from(from.children);
  const b = Array.from(to.children);
  a.forEach((child, i) => {
    const twin = b[i];
    if (twin) inlineStyles(child, twin);
  });
}

/** The SVG document (as a string) that draws `node` at `width` x `height` CSS pixels. Exported for tests. */
export function nodeToSvg(node: Element, width: number, height: number): string {
  const clone = node.cloneNode(true) as Element;
  inlineStyles(node, clone);
  clone.setAttribute('xmlns', 'http://www.w3.org/1999/xhtml');
  const xhtml = new XMLSerializer().serializeToString(clone);
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" ` +
    `viewBox="0 0 ${width} ${height}"><foreignObject width="100%" height="100%">${xhtml}</foreignObject></svg>`
  );
}

async function renderNode(node: Element, options: ToPngOptions): Promise<Blob> {
  const rect = node.getBoundingClientRect();
  const width = Math.max(1, Math.ceil(rect.width));
  const height = Math.max(1, Math.ceil(rect.height));
  const scale = Math.min(options.scale ?? 2, MAX_EDGE / Math.max(width, height));
  const svg = nodeToSvg(node, width, height);
  const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  const img = await new Promise<HTMLImageElement>((resolve, reject) => {
    const el = new Image();
    el.onload = () => resolve(el);
    el.onerror = () => reject(new Error('render-failed'));
    el.src = url;
  });
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(width * scale);
  canvas.height = Math.round(height * scale);
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('canvas-unavailable');
  if (options.background) {
    ctx.fillStyle = options.background;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
  }
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  return blobOf(canvas);
}

/** A PNG of a canvas (as it is now) or of a DOM node. */
export function toPngBlob(source: Source, options: ToPngOptions = {}): Promise<Blob> {
  if (typeof OffscreenCanvas !== 'undefined' && source instanceof OffscreenCanvas)
    return blobOf(source);
  if (typeof HTMLCanvasElement !== 'undefined' && source instanceof HTMLCanvasElement)
    return blobOf(source);
  return renderNode(source as Element, options);
}
