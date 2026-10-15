// Pure reader logic: variant choice, preload window, spread pairing, step navigation. No React, no DOM.
import type { PageEntry } from '../types';

/**
 * The URL to show for a page: the smallest sized variant at least as wide as the needed pixels
 * (`cssWidth * dpr`), else the full-size original. Preloading uses the same call so the cache is hit.
 */
export function pickVariant(page: PageEntry, cssWidth: number, dpr: number): string {
  const need = Math.max(1, Math.ceil(cssWidth * Math.max(1, dpr)));
  const sorted = [...page.variants].sort((a, b) => a.width - b.width);
  // Never upscale a variant above the original, and prefer the original when it is smaller than the need.
  for (const v of sorted) if (v.width >= need && v.width < page.width) return v.url;
  return page.url;
}

/** Indexes to warm after `index`: the next `ahead` pages, clamped to the chapter. */
export function preloadWindow(index: number, count: number, ahead = 3): number[] {
  const out: number[] = [];
  for (let i = index + 1; i <= index + ahead && i < count; i++) out.push(i);
  return out;
}

/** A page group shown together: one page, or two in double-page mode. Indexes are in reading order. */
export type Spread = number[];

/**
 * Pairs pages for double-page mode. Wide pages (`is_double_spread`) always stand alone. With `coverAlone`
 * the first page is shown by itself so later pairs line up as in print.
 */
export function buildSpreads(pages: readonly PageEntry[], coverAlone: boolean): Spread[] {
  const out: Spread[] = [];
  let i = 0;
  if (coverAlone && pages.length > 0) {
    out.push([0]);
    i = 1;
  }
  while (i < pages.length) {
    const a = pages[i]!;
    const b = pages[i + 1];
    if (a.is_double_spread || !b || b.is_double_spread) {
      out.push([i]);
      i += 1;
    } else {
      out.push([i, i + 1]);
      i += 2;
    }
  }
  return out;
}

/** Index of the spread holding page `index`. */
export function spreadIndexOf(spreads: readonly Spread[], index: number): number {
  const at = spreads.findIndex((s) => s.includes(index));
  return at < 0 ? 0 : at;
}

/**
 * Where an arrow key goes. In RTL the right arrow goes back, in LTR the left arrow does; the returned value
 * is the step in reading order (+1 forward, -1 back).
 */
export function arrowStep(key: 'ArrowLeft' | 'ArrowRight', direction: 'rtl' | 'ltr'): 1 | -1 {
  const rightIsForward = direction === 'ltr';
  return (key === 'ArrowRight') === rightIsForward ? 1 : -1;
}

/** Tap zone to step: the left third and right third turn pages (direction aware); the middle is neutral. */
export function tapStep(x: number, width: number, direction: 'rtl' | 'ltr'): 1 | -1 | 0 {
  if (width <= 0) return 0;
  const r = x / width;
  if (r < 1 / 3) return direction === 'rtl' ? 1 : -1;
  if (r > 2 / 3) return direction === 'rtl' ? -1 : 1;
  return 0;
}

/** Reading progress in percent for 0-based page `index` of `count` pages. */
export const percentAt = (index: number, count: number): number =>
  count <= 0 ? 0 : Math.round(((Math.min(index, count - 1) + 1) / count) * 100);

export const clamp = (n: number, lo: number, hi: number) => Math.min(Math.max(n, lo), hi);

/** Parses the 1-based page segment of `/library/r/:chapterId/:page` into a 0-based index. */
export function parsePageParam(raw: string | undefined, count: number): number | null {
  if (!raw) return null;
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n) || n < 1) return null;
  return clamp(n - 1, 0, Math.max(0, count - 1));
}

/** Borders the reader may cut off a page (pixels of the full-size image), from the ingest crop data. */
export interface CropBox {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * How to show a page without its borders: the cropped aspect ratio, the image size relative to the box
 * (percent) and the shift that puts the kept region in view (percent of the image). `null` when there is
 * nothing to cut or the box is unusable.
 */
export function cropView(
  page: Pick<PageEntry, 'width' | 'height'>,
  crop: CropBox | null | undefined,
): { ratio: string; widthPct: number; shiftX: number; shiftY: number } | null {
  if (!crop) return null;
  const { width: w, height: h } = page;
  const { left, top, right, bottom } = crop;
  const kw = w - left - right;
  const kh = h - top - bottom;
  const sane = [left, top, right, bottom].every((n) => Number.isFinite(n) && n >= 0);
  if (!sane || kw < w * 0.4 || kh < h * 0.4 || (left === 0 && top === 0 && right === 0 && bottom === 0))
    return null;
  return {
    ratio: `${kw} / ${kh}`,
    widthPct: (w / kw) * 100,
    shiftX: (left / w) * 100,
    shiftY: (top / h) * 100,
  };
}
