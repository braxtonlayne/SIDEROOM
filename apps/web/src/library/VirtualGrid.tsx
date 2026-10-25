// A windowed grid for Browse (LIB-BRW-001): only the rows near the viewport are mounted, so a catalog of
// thousands of series keeps a few dozen cards in the DOM. Spacers above and below keep the scroll length.
// The scroller is the nearest scrollable ancestor (the page body or the app shell). Where nothing can be
// measured (width 0: tests, print) or the list is short, every item renders in a plain grid.
import { useRovingTabindex } from '../a11y/useRovingTabindex';
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';

const MIN_COL = 150;
const GAP = 20;
const OVERSCAN_ROWS = 3;
/** Below this many items the whole list is rendered. */
export const VIRTUAL_MIN_ITEMS = 60;
const ROW_ESTIMATE = 330;

function scrollParent(el: HTMLElement | null): HTMLElement | Window {
  for (let n = el?.parentElement ?? null; n; n = n.parentElement) {
    const o = getComputedStyle(n).overflowY;
    if ((o === 'auto' || o === 'scroll') && n.scrollHeight > n.clientHeight) return n;
  }
  return window;
}

/** How many columns of at least `min` pixels fit `width`. */
export const columnsFor = (width: number, min = MIN_COL, gap = GAP) =>
  Math.max(1, Math.floor((width + gap) / (min + gap)));

/** The row range [first, last) to mount for a viewport, with overscan; clamped to `rows`. */
export function visibleRows(
  offset: number,
  viewport: number,
  stride: number,
  rows: number,
  overscan = OVERSCAN_ROWS,
) {
  const first = Math.max(0, Math.floor(offset / stride) - overscan);
  const last = Math.min(rows, Math.ceil((offset + viewport) / stride) + overscan);
  return { first: Math.min(first, rows), last: Math.max(last, Math.min(first, rows)) };
}

export function VirtualGrid<T>({
  items,
  keyOf,
  render,
}: {
  items: T[];
  keyOf: (item: T) => string;
  render: (item: T) => ReactNode;
}) {
  const host = useRef<HTMLDivElement>(null);
  // Cards are one Tab stop with arrow keys; only mounted rows take part, focus scrolls the next ones in.
  useRovingTabindex(host, { selector: '[data-poster-link]', orientation: 'grid' });
  const firstRow = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [rowH, setRowH] = useState(ROW_ESTIMATE);
  const [view, setView] = useState({ offset: 0, viewport: 0 });

  useLayoutEffect(() => {
    const el = host.current;
    if (!el) return;
    setWidth(el.clientWidth);
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const measure = useCallback(() => {
    const el = host.current;
    if (!el) return;
    const sp = scrollParent(el);
    const top = el.getBoundingClientRect().top;
    if (sp === window) setView({ offset: Math.max(0, -top), viewport: window.innerHeight });
    else {
      const r = (sp as HTMLElement).getBoundingClientRect();
      setView({ offset: Math.max(0, r.top - top), viewport: r.height });
    }
  }, []);
  useEffect(() => {
    measure();
    const sp = scrollParent(host.current);
    sp.addEventListener('scroll', measure, { passive: true });
    window.addEventListener('resize', measure);
    return () => {
      sp.removeEventListener('scroll', measure);
      window.removeEventListener('resize', measure);
    };
  }, [measure, items.length]);

  const virtual = width > 0 && items.length >= VIRTUAL_MIN_ITEMS;
  const cols = virtual ? columnsFor(width) : 0;
  const rows = virtual ? Math.ceil(items.length / cols) : 0;
  const stride = rowH + GAP;
  const { first, last } = virtual
    ? visibleRows(view.offset, view.viewport, stride, rows)
    : { first: 0, last: 0 };

  // Learn the real row height from the first mounted row.
  useLayoutEffect(() => {
    const h = firstRow.current?.offsetHeight ?? 0;
    if (virtual && h > 0 && Math.abs(h - rowH) > 1) setRowH(h);
  }, [virtual, rowH, first, cols]);

  if (!virtual)
    return (
      <div ref={host} className="lib-grid" data-testid="browse-grid">
        {items.map((it) => (
          <div key={keyOf(it)}>{render(it)}</div>
        ))}
      </div>
    );

  const shown = items.slice(first * cols, last * cols);
  return (
    <div ref={host} className="lib-vgrid">
      <div
        className="lib-grid lib-vgrid-window"
        data-testid="browse-grid"
        data-virtual="true"
        data-rows={rows}
        style={{
          gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
          paddingBlockStart: first * stride,
          paddingBlockEnd: Math.max(0, rows - last) * stride,
        }}
      >
        {shown.map((it, i) => (
          <div key={keyOf(it)} ref={i === 0 ? firstRow : undefined}>
            {render(it)}
          </div>
        ))}
      </div>
    </div>
  );
}
