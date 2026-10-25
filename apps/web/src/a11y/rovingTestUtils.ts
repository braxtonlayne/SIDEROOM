import { vi } from 'vitest';

/**
 * jsdom has no layout, so `offsetTop` is always 0 and a grid looks like one row. This gives the items matching
 * `selector` a row position (`index / columns`) in DOM order, which is what `columnsOf` reads. Returns the restore
 * function; call it in `afterEach`.
 */
export function mockGridRows(selector: string, columns: number): () => void {
  const spy = vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockImplementation(function (
    this: HTMLElement,
  ) {
    const all = Array.from(document.querySelectorAll<HTMLElement>(selector));
    const i = all.indexOf(this);
    return i < 0 ? 0 : Math.floor(i / columns) * 100;
  });
  return () => spy.mockRestore();
}

/** `tabIndex` of each element, for asserting "one Tab stop". */
export const tabStops = (els: HTMLElement[]) => els.map((e) => e.tabIndex);
