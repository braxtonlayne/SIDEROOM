/**
 * Scrolls a sideways-scrolling bar so that `link` is inside it, with `pad` px to spare (D-817, D-821).
 * Only the bar moves: unlike `scrollIntoView`, the page and any other scrolling ancestor stay where they are.
 * Works the same in a right-to-left bar, because it moves by the difference of the two boxes.
 */
export function scrollBarTo(bar: HTMLElement, link: HTMLElement, pad = 16): void {
  if (bar.scrollWidth <= bar.clientWidth) return;
  const b = bar.getBoundingClientRect();
  const r = link.getBoundingClientRect();
  if (r.left < b.left + pad) bar.scrollLeft += r.left - b.left - pad;
  else if (r.right > b.right - pad) bar.scrollLeft += r.right - b.right + pad;
}
