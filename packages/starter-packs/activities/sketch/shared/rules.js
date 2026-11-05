// @ts-check
// The rules of Sketch & Slander that the host and every page agree on (ACT-SKS-001): the canvas, the palette,
// the ink format and its limits, scoring and names. Pure functions only; host/engine.js runs the game with them.

/** The canvas every page draws in, in logical units (4:3). Points are integers in this space. */
export const CANVAS_W = 1000;
export const CANVAS_H = 750;

/** The palette (index = colour id). Ink of colour ERASER is drawn in the paper colour. */
export const PALETTE = Object.freeze([
  '#1f1f24', '#ffffff', '#8a8f98', '#e5383b', '#f48c06', '#ffd60a',
  '#38b000', '#0fa3b1', '#2667ff', '#7b2cbf', '#ff70a6', '#7f5539',
]);
export const ERASER = -1;
export const PAPER = '#ffffff';
/** Brush widths in logical units (index = size id). */
export const SIZES = Object.freeze([5, 12, 28]);

export const LIMITS = Object.freeze({
  /** Points (x, y pairs) in one ink op: the drawer sends a batch about every 50 ms. */
  pointsPerOp: 128,
  /** Ops and points in one turn's drawing. */
  opsPerTurn: 6000,
  pointsPerTurn: 40000,
  /** Characters of a guess and of a nickname. */
  guessChars: 48,
  nameChars: 20,
  /** Seats in one game (members and guests). */
  players: 16,
  /** Lines of the shared feed the host keeps. */
  feed: 24,
  /** A guess at most this often per player. */
  guessGapMs: 400,
});

export const DEFAULT_SETTINGS = Object.freeze({ rounds: 3, drawSecs: 80, pickSecs: 15, revealSecs: 6, builtIn: true });
export const ROUND_CHOICES = Object.freeze([1, 2, 3, 4, 5]);
export const DRAW_CHOICES = Object.freeze([45, 60, 80, 100, 120]);

/**
 * @typedef {{ k: 'p', id: number, c: number, w: number, pts: number[], q?: number }
 *   | { k: 'u', q?: number } | { k: 'c', q?: number }} InkOp
 *   p: points of stroke `id` (new or continued) in colour `c` and size `w`; u: undo the newest stroke; c: clear.
 *   q: the drawer's own sequence number, so its page knows which of its ops the host has taken.
 */

/**
 * Check and normalise one ink op from the drawer. Null when it is not a valid op.
 * @param {any} op @returns {InkOp | null}
 */
export function cleanInkOp(op) {
  if (!op || typeof op !== 'object') return null;
  const q = Number.isSafeInteger(op.q) && op.q > 0 ? op.q : undefined;
  if (op.k === 'u' || op.k === 'c') return q === undefined ? { k: op.k } : { k: op.k, q };
  if (op.k !== 'p') return null;
  if (!Number.isSafeInteger(op.id) || op.id < 0 || op.id > 1e9) return null;
  if (!Number.isInteger(op.c) || op.c < ERASER || op.c >= PALETTE.length) return null;
  if (!Number.isInteger(op.w) || op.w < 0 || op.w >= SIZES.length) return null;
  if (!Array.isArray(op.pts) || op.pts.length < 2 || op.pts.length % 2 !== 0 || op.pts.length > LIMITS.pointsPerOp * 2) return null;
  /** @type {number[]} */
  const pts = [];
  for (let i = 0; i < op.pts.length; i += 2) {
    const x = op.pts[i];
    const y = op.pts[i + 1];
    if (typeof x !== 'number' || typeof y !== 'number' || !Number.isFinite(x) || !Number.isFinite(y)) return null;
    pts.push(Math.max(0, Math.min(CANVAS_W, Math.round(x))), Math.max(0, Math.min(CANVAS_H, Math.round(y))));
  }
  /** @type {InkOp} */
  const out = { k: 'p', id: op.id, c: op.c, w: op.w, pts };
  if (q !== undefined) out.q = q;
  return out;
}

/**
 * The strokes a list of ops draws, in order (undo removes the newest stroke, clear removes all).
 * @param {InkOp[]} ops @returns {{ id: number, c: number, w: number, pts: number[] }[]}
 */
export function strokesOf(ops) {
  /** @type {{ id: number, c: number, w: number, pts: number[] }[]} */
  let strokes = [];
  /** @type {Map<number, { id: number, c: number, w: number, pts: number[] }>} */
  let byId = new Map();
  for (const op of ops) {
    if (op.k === 'c') {
      strokes = [];
      byId = new Map();
    } else if (op.k === 'u') {
      const last = strokes.pop();
      if (last) byId.delete(last.id);
    } else if (op.k === 'p') {
      const s = byId.get(op.id);
      if (s) s.pts.push(...op.pts);
      else {
        const n = { id: op.id, c: op.c, w: op.w, pts: [...op.pts] };
        strokes.push(n);
        byId.set(op.id, n);
      }
    }
  }
  return strokes;
}

/**
 * Points for a correct guess: more the sooner it comes, plus a bonus for the first two.
 * @param {number} leftMs time left in the turn @param {number} totalMs the turn's length @param {number} place 0 for the first guesser
 */
export function guessPoints(leftMs, totalMs, place) {
  const share = totalMs > 0 ? Math.max(0, Math.min(1, leftMs / totalMs)) : 0;
  const bonus = place === 0 ? 50 : place === 1 ? 25 : 0;
  return Math.round((100 + 200 * share) / 5) * 5 + bonus;
}

/** Points the drawer gets for each player who guessed. */
export const DRAWER_POINTS = 50;

/** A nickname as players see it: no control characters or angle brackets, at most nameChars. @param {unknown} s */
export function cleanName(s) {
  const n = String(s ?? '')
    .replace(/[\p{C}<>]/gu, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, LIMITS.nameChars)
    .trim();
  return n || 'Player';
}

/** A guess as the host keeps it. @param {unknown} s */
export function cleanGuess(s) {
  return String(s ?? '')
    .replace(/[\p{C}<>]/gu, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, LIMITS.guessChars);
}

/** A seeded PRNG (mulberry32): the turn order is public and must be the same after a handoff. @param {number} seed */
export function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Shuffle a copy of `list` with `rand`. @template T @param {T[]} list @param {() => number} rand @returns {T[]} */
export function shuffle(list, rand) {
  const out = [...list];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}
