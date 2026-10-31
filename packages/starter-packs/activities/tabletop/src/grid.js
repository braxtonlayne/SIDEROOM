// Map geometry: square and hex grids over an image, cell <-> pixel, neighbours and measuring. Pure functions.
// A map is { kind: 'square' | 'hex', size, ox, oy, width, height, unit, unitName, diagonal } where `size` is the
// cell edge (square) or the width across flats (hex, pointy-top, odd rows shifted right), `ox, oy` the grid
// origin in image pixels and `width, height` the image size in pixels.

const SQRT3 = Math.sqrt(3);

export const MAX_CELLS = 40_000;

export function hexHeight(size) {
  return (size * 2) / SQRT3;
}

/** Number of columns and rows that cover the image from the grid origin. */
export function gridDims(map) {
  const usableW = Math.max(map.width - map.ox, 0);
  const usableH = Math.max(map.height - map.oy, 0);
  if (map.kind === 'hex') {
    const h = hexHeight(map.size);
    const rows = usableH <= h ? 1 : Math.ceil((usableH - h) / (h * 0.75)) + 1;
    // Odd rows are shifted by half a cell, so the last column may stick out by that much.
    const cols = Math.max(Math.ceil((usableW - map.size / 2) / map.size), 1);
    return { cols: Math.max(cols, 1), rows };
  }
  return {
    cols: Math.max(Math.ceil(usableW / map.size), 1),
    rows: Math.max(Math.ceil(usableH / map.size), 1),
  };
}

/** True when a map is usable: a positive size and a cell count the fog and the state can carry. */
export function validMap(map) {
  if (!map || (map.kind !== 'square' && map.kind !== 'hex')) return false;
  const nums = [map.size, map.ox, map.oy, map.width, map.height];
  if (!nums.every((n) => Number.isFinite(n))) return false;
  if (map.size < 8 || map.width < 1 || map.height < 1 || map.ox < 0 || map.oy < 0) return false;
  const { cols, rows } = gridDims(map);
  return cols * rows <= MAX_CELLS;
}

export const inBounds = (map, c, r) => {
  const { cols, rows } = gridDims(map);
  return Number.isInteger(c) && Number.isInteger(r) && c >= 0 && r >= 0 && c < cols && r < rows;
};

/** Centre of a cell in image pixels. */
export function cellCenter(map, c, r) {
  if (map.kind === 'hex') {
    const h = hexHeight(map.size);
    return {
      x: map.ox + map.size * (c + 0.5 + (r & 1) * 0.5),
      y: map.oy + h / 2 + r * h * 0.75,
    };
  }
  return { x: map.ox + (c + 0.5) * map.size, y: map.oy + (r + 0.5) * map.size };
}

/** The cell containing an image pixel (may be out of bounds; check with `inBounds`). */
export function pixelToCell(map, x, y) {
  if (map.kind === 'hex') {
    const h = hexHeight(map.size);
    // Pixel -> fractional axial (pointy-top), then cube rounding.
    const px = x - map.ox - map.size / 2;
    const py = y - map.oy - h / 2;
    const q = (px * SQRT3 / 3 - py / 3) / (h / 2);
    const rr = ((py * 2) / 3) / (h / 2);
    const [aq, ar] = axialRound(q, rr);
    // axial -> odd-r offset
    const row = ar;
    const col = aq + (row - (row & 1)) / 2;
    return { c: col, r: row };
  }
  return {
    c: Math.floor((x - map.ox) / map.size),
    r: Math.floor((y - map.oy) / map.size),
  };
}

function axialRound(q, r) {
  const s = -q - r;
  let rq = Math.round(q);
  let rr = Math.round(r);
  const rs = Math.round(s);
  const dq = Math.abs(rq - q);
  const dr = Math.abs(rr - r);
  const ds = Math.abs(rs - s);
  if (dq > dr && dq > ds) rq = -rr - rs;
  else if (dr > ds) rr = -rq - rs;
  return [rq, rr];
}

/** Odd-r offset to axial. */
export const toAxial = (c, r) => ({ q: c - (r - (r & 1)) / 2, r });

/** The corners of a cell (6 for a hex, 4 for a square) in image pixels, for drawing. */
export function cellCorners(map, c, r) {
  const { x, y } = cellCenter(map, c, r);
  if (map.kind === 'hex') {
    const rad = hexHeight(map.size) / 2;
    return Array.from({ length: 6 }, (_, i) => {
      const a = (Math.PI / 180) * (60 * i - 30);
      return { x: x + rad * Math.cos(a), y: y + rad * Math.sin(a) };
    });
  }
  const half = map.size / 2;
  return [
    { x: x - half, y: y - half },
    { x: x + half, y: y - half },
    { x: x + half, y: y + half },
    { x: x - half, y: y + half },
  ];
}

/**
 * Distance between two cells in cells. Square grids follow `map.diagonal`: `'chebyshev'` (a diagonal costs
 * one), `'alternate'` (the 5-10-5 rule: every second diagonal costs two) or `'euclid'` (straight-line, may be
 * fractional). Hex grids count steps.
 */
export function cellDistance(map, a, b) {
  if (map.kind === 'hex') {
    const p = toAxial(a.c, a.r);
    const q = toAxial(b.c, b.r);
    const dq = p.q - q.q;
    const dr = p.r - q.r;
    return (Math.abs(dq) + Math.abs(dr) + Math.abs(dq + dr)) / 2;
  }
  const dx = Math.abs(a.c - b.c);
  const dy = Math.abs(a.r - b.r);
  switch (map.diagonal) {
    case 'alternate':
      return Math.max(dx, dy) + Math.floor(Math.min(dx, dy) / 2);
    case 'euclid':
      return Math.hypot(dx, dy);
    default:
      return Math.max(dx, dy);
  }
}

/** A measurement between two cells: cells and the map's units (feet, metres, squares...). */
export function measure(map, a, b) {
  const cells = cellDistance(map, a, b);
  const unit = Number.isFinite(map.unit) && map.unit > 0 ? map.unit : 1;
  const value = cells * unit;
  return { cells, value: Math.round(value * 100) / 100, unitName: map.unitName ?? '' };
}

/** All cells within `radius` cells (square: Chebyshev box clipped to a disc; hex: hex range) of a centre. */
export function cellsInRadius(map, c0, r0, radius) {
  const { cols, rows } = gridDims(map);
  const out = [];
  const R = Math.max(Math.floor(radius), 0);
  for (let r = Math.max(r0 - R - 1, 0); r <= Math.min(r0 + R + 1, rows - 1); r++) {
    for (let c = Math.max(c0 - R - 1, 0); c <= Math.min(c0 + R + 1, cols - 1); c++) {
      const d =
        map.kind === 'hex'
          ? cellDistance(map, { c: c0, r: r0 }, { c, r })
          : Math.hypot(c - c0, r - r0);
      if (d <= R + (map.kind === 'hex' ? 0 : 0.5)) out.push({ c, r });
    }
  }
  return out;
}
