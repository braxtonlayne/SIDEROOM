import { useCallback, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import {
  computePosition,
  type Box,
  type PositionOptions,
  type PositionResult,
} from '../../util/position';

/** Anything we can measure: a DOM element or a point (context menus). */
export type FloatingAnchor = HTMLElement | { x: number; y: number } | null;

/** How many animation frames an anchor with no box is waited for before the element is left as it is. */
export const UNMEASURED_ANCHOR_FRAMES = 12;

/**
 * D-870: an element anchor that is not in the document or measures 0 x 0 (a display: none ancestor, a wrapper not laid
 * out yet) has no box to place against: placing against it puts the element in the viewport's corner. The 0 x 0 test
 * runs only where the browser can tell a laid-out element from one that is not (`checkVisibility`): jsdom measures
 * every element as 0 x 0.
 */
function hasBox(a: HTMLElement): boolean {
  if (!a.isConnected) return false;
  if (typeof a.checkVisibility !== 'function') return true;
  const r = a.getBoundingClientRect();
  return r.width > 0 || r.height > 0;
}

function anchorBox(a: NonNullable<FloatingAnchor>): Box {
  if (a instanceof HTMLElement) {
    const r = a.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }
  return { x: a.x, y: a.y, width: 0, height: 0 };
}

/**
 * The CSS `zoom` in effect on an element (its own times its ancestors'; 1 where the browser has no `zoom` or does
 * not report it). Pointer and `getBoundingClientRect` values are viewport pixels, but a zoomed element's own
 * lengths (its `translate`, its `offsetWidth`) are in its zoomed pixels, so the two are converted with this.
 */
export function effectiveZoom(el: Element | null): number {
  const z = (el as { currentCSSZoom?: unknown } | null)?.currentCSSZoom;
  return typeof z === 'number' && Number.isFinite(z) && z > 0 ? z : 1;
}

export interface UseFloatingOptions extends PositionOptions {
  open: boolean;
  anchor: FloatingAnchor;
}

export interface UseFloatingResult {
  /** Attach to the floating element. */
  ref: (el: HTMLElement | null) => void;
  /** position: fixed styles; hidden until the first measurement to avoid a flash at 0,0. */
  style: CSSProperties;
  position: PositionResult | null;
  update: () => void;
}

/**
 * Positions a floating element relative to an anchor with edge flipping and shifting.
 * Re-measures on window resize and on any scroll (capture), while open.
 */
export function useFloating({
  open,
  anchor,
  placement,
  offset,
  padding,
  flip,
  shift,
}: UseFloatingOptions): UseFloatingResult {
  const elRef = useRef<HTMLElement | null>(null);
  const [el, setEl] = useState<HTMLElement | null>(null);
  const [position, setPosition] = useState<PositionResult | null>(null);
  // The floating element may sit inside a zoomed box (the member's zoom, D-540): measure in viewport pixels,
  // place in its own.
  const [zoom, setZoom] = useState(1);

  const frames = useRef(0);
  const raf = useRef(0);
  const latest = useRef<() => void>(() => {});

  const ref = useCallback((node: HTMLElement | null) => {
    elRef.current = node;
    setEl(node);
  }, []);

  const update = useCallback(() => {
    const node = elRef.current;
    if (!node || !anchor) return;
    if (anchor instanceof HTMLElement && !hasBox(anchor)) {
      // Not used: the place already set is kept; with none yet the element stays hidden and the anchor is measured
      // again on the next frame, a bounded number of times.
      if (
        frames.current < UNMEASURED_ANCHOR_FRAMES &&
        typeof requestAnimationFrame === 'function'
      ) {
        frames.current += 1;
        cancelAnimationFrame(raf.current);
        raf.current = requestAnimationFrame(() => latest.current());
      }
      return;
    }
    frames.current = 0;
    const z = effectiveZoom(node);
    setZoom(z);
    const next = computePosition(
      anchorBox(anchor),
      { width: node.offsetWidth * z, height: node.offsetHeight * z },
      {
        width: document.documentElement.clientWidth || window.innerWidth,
        height: document.documentElement.clientHeight || window.innerHeight,
      },
      { placement, offset, padding, flip, shift },
    );
    setPosition((prev) =>
      prev && prev.x === next.x && prev.y === next.y && prev.placement === next.placement
        ? prev
        : next,
    );
  }, [anchor, placement, offset, padding, flip, shift]);
  latest.current = update;

  useLayoutEffect(() => {
    if (!open || !el) return;
    frames.current = 0;
    update();
    const on = () => update();
    window.addEventListener('resize', on);
    window.addEventListener('scroll', on, true);
    const RO = typeof ResizeObserver === 'undefined' ? undefined : ResizeObserver;
    const ro = RO ? new RO(on) : undefined;
    ro?.observe(el);
    return () => {
      cancelAnimationFrame(raf.current);
      window.removeEventListener('resize', on);
      window.removeEventListener('scroll', on, true);
      ro?.disconnect();
    };
  }, [open, el, update]);

  const style: CSSProperties = {
    position: 'fixed',
    left: 0,
    top: 0,
    transform: position
      ? `translate(${Math.round(position.x / zoom)}px, ${Math.round(position.y / zoom)}px)`
      : undefined,
    visibility: position ? undefined : 'hidden',
  };
  return { ref, style, position, update };
}
