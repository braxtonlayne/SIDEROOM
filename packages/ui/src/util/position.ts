export type Side = "top" | "bottom" | "left" | "right";
export type Align = "start" | "center" | "end";
/** "top", "bottom-start", "right-end", ... */
export type FloatingPlacement = Side | `${Side}-${Align}`;

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface Size {
  width: number;
  height: number;
}

export interface PositionOptions {
  placement?: FloatingPlacement;
  /** Gap between the anchor and the floating element. */
  offset?: number;
  /** Minimum distance kept from the viewport edge when shifting. */
  padding?: number;
  /** Flip to the opposite side when the preferred side overflows and the other fits better. */
  flip?: boolean;
  /** Slide along the cross axis to stay inside the viewport. */
  shift?: boolean;
}

export interface PositionResult {
  x: number;
  y: number;
  /** The placement actually used (after flipping). */
  placement: FloatingPlacement;
  side: Side;
}

const OPPOSITE: Record<Side, Side> = { top: "bottom", bottom: "top", left: "right", right: "left" };

export function parsePlacement(p: FloatingPlacement): { side: Side; align: Align } {
  const [side, align] = p.split("-") as [Side, Align | undefined];
  return { side, align: align ?? "center" };
}

function coords(anchor: Box, float: Size, side: Side, align: Align, offset: number) {
  let x: number;
  let y: number;
  if (side === "top" || side === "bottom") {
    y = side === "top" ? anchor.y - float.height - offset : anchor.y + anchor.height + offset;
    x =
      align === "start"
        ? anchor.x
        : align === "end"
          ? anchor.x + anchor.width - float.width
          : anchor.x + (anchor.width - float.width) / 2;
  } else {
    x = side === "left" ? anchor.x - float.width - offset : anchor.x + anchor.width + offset;
    y =
      align === "start"
        ? anchor.y
        : align === "end"
          ? anchor.y + anchor.height - float.height
          : anchor.y + (anchor.height - float.height) / 2;
  }
  return { x, y };
}

/** How many pixels the box sticks out of the viewport along the main axis of `side`. */
function mainOverflow(side: Side, c: { x: number; y: number }, float: Size, vp: Size, pad: number): number {
  switch (side) {
    case "top":
      return Math.max(0, pad - c.y);
    case "bottom":
      return Math.max(0, c.y + float.height - (vp.height - pad));
    case "left":
      return Math.max(0, pad - c.x);
    case "right":
      return Math.max(0, c.x + float.width - (vp.width - pad));
  }
}

/**
 * Pure positioning: place `float` next to `anchor` inside a `viewport` (all in viewport coordinates),
 * flipping to the opposite side when the preferred one overflows, then shifting along the cross axis so
 * the element stays on screen.
 */
export function computePosition(
  anchor: Box,
  float: Size,
  viewport: Size,
  { placement = "bottom", offset = 8, padding = 8, flip = true, shift = true }: PositionOptions = {},
): PositionResult {
  const { side: preferred, align } = parsePlacement(placement);
  let side = preferred;
  let c = coords(anchor, float, side, align, offset);
  if (flip) {
    const over = mainOverflow(side, c, float, viewport, padding);
    if (over > 0) {
      const opp = OPPOSITE[side];
      const oc = coords(anchor, float, opp, align, offset);
      if (mainOverflow(opp, oc, float, viewport, padding) < over) {
        side = opp;
        c = oc;
      }
    }
  }
  let { x, y } = c;
  if (shift) {
    const clamp = (v: number, size: number, vpSize: number) =>
      Math.max(padding, Math.min(v, vpSize - size - padding));
    if (side === "top" || side === "bottom") x = clamp(x, float.width, viewport.width);
    else y = clamp(y, float.height, viewport.height);
    // Neither side has room (a tall menu opened mid-screen): pin it inside the viewport rather than
    // letting part of it hang off the edge, even though it then overlaps the anchor.
    if (flip && mainOverflow(side, { x, y }, float, viewport, padding) > 0) {
      if (side === "top" || side === "bottom") y = clamp(y, float.height, viewport.height);
      else x = clamp(x, float.width, viewport.width);
    }
  }
  return { x, y, side, placement: align === "center" ? side : (`${side}-${align}` as FloatingPlacement) };
}
