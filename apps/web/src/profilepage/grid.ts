// Pure grid arithmetic of the profile editor (EXT-PRF-005): where blocks may sit on the 12 column grid, how a
// drag or a key press moves or resizes one, and where a new block goes. The server checks the same rules
// (no overlap, inside the grid) when the document is saved.
import { DEFAULT_SIZE, GRID_COLUMNS, MAX_ROWS, type Block, type BlockKind } from './types';

type Rect = Pick<Block, 'x' | 'y' | 'w' | 'h'>;

export const overlaps = (a: Rect, b: Rect): boolean =>
  !(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y);

export const inside = (r: Rect): boolean =>
  r.w >= 1 &&
  r.h >= 1 &&
  r.w <= GRID_COLUMNS &&
  r.h <= 24 &&
  r.x >= 0 &&
  r.y >= 0 &&
  r.x + r.w <= GRID_COLUMNS &&
  r.y + r.h <= MAX_ROWS;

/** May `r` replace block `id` (or be added when `id` is null)? */
export function fits(blocks: readonly Block[], r: Rect, id: string | null): boolean {
  return inside(r) && !blocks.some((b) => b.id !== id && overlaps(b, r));
}

/** The block `id` moved to column `x`, row `y`, or the same blocks when that does not fit. */
export function moveBlock(blocks: readonly Block[], id: string, x: number, y: number): Block[] {
  const b = blocks.find((k) => k.id === id);
  if (!b) return [...blocks];
  const nx = Math.min(Math.max(0, x), GRID_COLUMNS - b.w);
  const ny = Math.max(0, y);
  return fits(blocks, { x: nx, y: ny, w: b.w, h: b.h }, id)
    ? blocks.map((k) => (k.id === id ? { ...k, x: nx, y: ny } : k))
    : [...blocks];
}

/** The block `id` resized to `w` by `h`, or the same blocks when that does not fit. */
export function resizeBlock(blocks: readonly Block[], id: string, w: number, h: number): Block[] {
  const b = blocks.find((k) => k.id === id);
  if (!b) return [...blocks];
  const r = { x: b.x, y: b.y, w: Math.max(1, w), h: Math.max(1, h) };
  return fits(blocks, r, id) ? blocks.map((k) => (k.id === id ? { ...k, ...r } : k)) : [...blocks];
}

/** First free place for a `w` by `h` block, scanning rows then columns; null when the grid is full. */
export function findSpot(
  blocks: readonly Block[],
  w: number,
  h: number,
): { x: number; y: number } | null {
  for (let y = 0; y + h <= MAX_ROWS; y++) {
    for (let x = 0; x + w <= GRID_COLUMNS; x++) {
      if (fits(blocks, { x, y, w, h }, null)) return { x, y };
    }
  }
  return null;
}

/** An unused block id of the form `kind-N`. */
export function newId(blocks: readonly Block[], kind: BlockKind): string {
  const base = kind.replace(/_/g, '-');
  const used = new Set(blocks.map((b) => b.id));
  for (let n = 1; ; n++) {
    const id = `${base}-${n}`;
    if (!used.has(id)) return id;
  }
}

/** Add a block of `kind` in the first free spot; null when it does not fit. */
export function addBlock(blocks: readonly Block[], kind: BlockKind): Block[] | null {
  const [w, h] = DEFAULT_SIZE[kind];
  const spot = findSpot(blocks, w, h) ?? findSpot(blocks, Math.min(w, 6), Math.min(h, 3));
  if (!spot) return null;
  const size = findSpot(blocks, w, h) ? { w, h } : { w: Math.min(w, 6), h: Math.min(h, 3) };
  return [...blocks, { id: newId(blocks, kind), type: kind, ...spot, ...size, config: {} }];
}

/** The cell under a point of the canvas: `left`/`top` are pixels from its corner. */
export function cellAt(left: number, top: number, cellW: number, cellH: number) {
  return {
    x: Math.max(0, Math.min(GRID_COLUMNS - 1, Math.floor(left / cellW))),
    y: Math.max(0, Math.floor(top / cellH)),
  };
}

/** How many rows the canvas needs to show every block, and a few spare ones. */
export const rowsFor = (blocks: readonly Block[], spare = 3): number =>
  Math.min(MAX_ROWS, Math.max(8, ...blocks.map((b) => b.y + b.h)) + spare);
