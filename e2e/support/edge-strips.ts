import type { Locator, Page } from '@playwright/test';

/**
 * Shared edge-strip cutter (wave 101, SEC-AUD-447, D-957).
 *
 * A spec that reads pixels at the two inline edges of a box (a scroll shade, a cover, a border) cuts two thin strips out
 * of a screenshot. Wave 98's cue spec and wave 100's rtl spec took `locator.boundingBox()` and clipped from `y = box.y`
 * without bringing the box into view: once other specs' invoices had made the Invoices table taller than the viewport, the
 * clip lay partly or wholly off screen ("empty clip", or strips of the wrong pixels).
 *
 * What this assumes: the box is a real element with a layout box; the page may be scrolled (it is scrolled here, with
 * `scrollIntoView({ block: 'nearest', inline: 'nearest' })`, no smooth scroll, then one animation frame, and not scrolled
 * back); the box's own content is not scrolled (the spec sets `scrollLeft` itself, before the call); the part of the box
 * that is inside the viewport is what is cut. If less than 40 px of its height, or less than twice the strip width of its
 * width, is inside the viewport, it throws `Error('edge-strips: <selector> has only <n> px in view')`: a spec error, never
 * a finding. No image library: the strips are PNG buffers from `page.screenshot({ clip })`, compared with `Buffer.equals`.
 *
 * Use (three lines in a spec):
 *   const { l, r } = await edgeStrips(page, '.adm-table-wrap:has(> .inv-table)', { maxHeight: 200 });
 *   // ... switch the rule, call it again ...
 *   expect(on.l.equals(off.l)).toBe(false); // a shade at the left
 * Both strips of a pair, and the "off" and "on" pair, are cut at the same page scroll when the box does not move between
 * the calls: the second call's scroll is a no-op.
 *
 * Plain Playwright and DOM; nothing is imported from a spec.
 */

export interface StripRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface EdgeStrips {
  /** The strip at the box's left edge. */
  l: Buffer;
  /** The strip at the box's right edge. */
  r: Buffer;
  /** The box's rect after scrolling (viewport coordinates, may run past the viewport). */
  box: StripRect;
  /** The rect of the left strip as cut; `clip.width` is the strip width. The right strip is at `x + visible width - width`. */
  clip: StripRect & { viewportHeight: number; viewportWidth: number; visible: StripRect };
}

export interface EdgeStripsOptions {
  /** Strip width in px (default 16). */
  width?: number;
  /** Cap on the strip height in px (default: the whole visible part). */
  maxHeight?: number;
}

const MIN_HEIGHT = 40;

export async function edgeStrips(
  page: Page,
  target: string | Locator,
  opts: EdgeStripsOptions = {},
): Promise<EdgeStrips> {
  const width = opts.width ?? 16;
  const loc = (typeof target === 'string' ? page.locator(target) : target).first();
  const name = typeof target === 'string' ? target : String(target);
  const m = await loc.evaluate(async (el) => {
    el.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'instant' });
    await new Promise<void>((done) => requestAnimationFrame(() => done()));
    const b = el.getBoundingClientRect();
    return {
      box: { x: b.x, y: b.y, width: b.width, height: b.height },
      vw: document.documentElement.clientWidth,
      vh: window.innerHeight,
    };
  });
  const left = Math.max(m.box.x, 0);
  const right = Math.min(m.box.x + m.box.width, m.vw);
  const top = Math.max(m.box.y, 0);
  const bottom = Math.min(m.box.y + m.box.height, m.vh);
  const visW = right - left;
  const visH = bottom - top;
  if (visH < MIN_HEIGHT || visW < 2 * width) {
    const px = visH < MIN_HEIGHT ? Math.max(visH, 0) : Math.max(visW, 0);
    throw new Error(`edge-strips: ${name} has only ${Math.round(px)} px in view`);
  }
  const height = Math.min(visH, opts.maxHeight ?? visH);
  const clipAt = (x: number): StripRect => ({ x, y: top, width, height });
  const l = await page.screenshot({ clip: clipAt(left) });
  const r = await page.screenshot({ clip: clipAt(right - width) });
  return {
    l,
    r,
    box: m.box,
    clip: {
      ...clipAt(left),
      viewportHeight: m.vh,
      viewportWidth: m.vw,
      visible: { x: left, y: top, width: visW, height: visH },
    },
  };
}
