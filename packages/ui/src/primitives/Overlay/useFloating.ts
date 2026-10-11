import { useCallback, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { computePosition, type Box, type PositionOptions, type PositionResult } from "../../util/position";

/** Anything we can measure: a DOM element or a point (context menus). */
export type FloatingAnchor = HTMLElement | { x: number; y: number } | null;

function anchorBox(a: NonNullable<FloatingAnchor>): Box {
  if (a instanceof HTMLElement) {
    const r = a.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }
  return { x: a.x, y: a.y, width: 0, height: 0 };
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
export function useFloating({ open, anchor, placement, offset, padding, flip, shift }: UseFloatingOptions): UseFloatingResult {
  const elRef = useRef<HTMLElement | null>(null);
  const [el, setEl] = useState<HTMLElement | null>(null);
  const [position, setPosition] = useState<PositionResult | null>(null);

  const ref = useCallback((node: HTMLElement | null) => {
    elRef.current = node;
    setEl(node);
  }, []);

  const update = useCallback(() => {
    const node = elRef.current;
    if (!node || !anchor) return;
    const next = computePosition(
      anchorBox(anchor),
      { width: node.offsetWidth, height: node.offsetHeight },
      { width: document.documentElement.clientWidth || window.innerWidth, height: document.documentElement.clientHeight || window.innerHeight },
      { placement, offset, padding, flip, shift },
    );
    setPosition((prev) =>
      prev && prev.x === next.x && prev.y === next.y && prev.placement === next.placement ? prev : next,
    );
  }, [anchor, placement, offset, padding, flip, shift]);

  useLayoutEffect(() => {
    if (!open || !el) return;
    update();
    const on = () => update();
    window.addEventListener("resize", on);
    window.addEventListener("scroll", on, true);
    const RO = typeof ResizeObserver === "undefined" ? undefined : ResizeObserver;
    const ro = RO ? new RO(on) : undefined;
    ro?.observe(el);
    return () => {
      window.removeEventListener("resize", on);
      window.removeEventListener("scroll", on, true);
      ro?.disconnect();
    };
  }, [open, el, update]);

  const style: CSSProperties = {
    position: "fixed",
    left: 0,
    top: 0,
    transform: position ? `translate(${Math.round(position.x)}px, ${Math.round(position.y)}px)` : undefined,
    visibility: position ? undefined : "hidden",
  };
  return { ref, style, position, update };
}
