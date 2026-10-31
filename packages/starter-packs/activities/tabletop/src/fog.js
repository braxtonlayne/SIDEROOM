// Fog of war: a bitset of hidden cells the host paints. Fog here is a table courtesy, not a secret: every
// participant's client holds the shared state (the room key is shared), so a modified client could look under
// it. The UI says so. Encoding for the shared state is run lengths of alternating revealed/hidden cells.

import { cellsInRadius } from './grid.js';

export class Fog {
  /** @param {number} cols @param {number} rows @param {boolean} hidden initial state of every cell */
  constructor(cols, rows, hidden = false) {
    this.cols = cols;
    this.rows = rows;
    this.bits = new Uint8Array(cols * rows);
    if (hidden) this.bits.fill(1);
  }

  index(c, r) {
    return r * this.cols + c;
  }
  has(c, r) {
    return Number.isInteger(c) && Number.isInteger(r) && c >= 0 && r >= 0 && c < this.cols && r < this.rows;
  }
  isHidden(c, r) {
    return this.has(c, r) && this.bits[this.index(c, r)] === 1;
  }
  set(c, r, hidden) {
    if (this.has(c, r)) this.bits[this.index(c, r)] = hidden ? 1 : 0;
  }
  hiddenCount() {
    let n = 0;
    for (const b of this.bits) n += b;
    return n;
  }
  fill(hidden) {
    this.bits.fill(hidden ? 1 : 0);
  }
  /** Paint a rectangle of cells (corners in either order). */
  paintRect(c0, r0, c1, r1, hidden) {
    const [ca, cb] = [Math.min(c0, c1), Math.max(c0, c1)];
    const [ra, rb] = [Math.min(r0, r1), Math.max(r0, r1)];
    for (let r = Math.max(ra, 0); r <= Math.min(rb, this.rows - 1); r++)
      for (let c = Math.max(ca, 0); c <= Math.min(cb, this.cols - 1); c++) this.set(c, r, hidden);
  }
  /** Paint a round brush of `radius` cells around a cell, per the map's geometry. */
  paintBrush(map, c, r, radius, hidden) {
    for (const cell of cellsInRadius(map, c, r, radius)) this.set(cell.c, cell.r, hidden);
  }
  /** Reveal everything within `radius` of each of the cells, for a token's sight. */
  revealAround(map, cells, radius) {
    for (const { c, r } of cells) this.paintBrush(map, c, r, radius, false);
  }
  clone() {
    const f = new Fog(this.cols, this.rows);
    f.bits.set(this.bits);
    return f;
  }
  /** Run-length encoding: `cols x rows : n0 n1 n2 ...`, runs alternate revealed, hidden, revealed... */
  encode() {
    const runs = [];
    let cur = 0;
    let n = 0;
    for (const b of this.bits) {
      if (b === cur) n += 1;
      else {
        runs.push(n);
        cur = b;
        n = 1;
      }
    }
    runs.push(n);
    return `${this.cols}x${this.rows}:${runs.join(',')}`;
  }
  /** The inverse of `encode`; `null` for anything malformed or sized wrong. */
  static decode(text, maxCells = 40_000) {
    const m = /^(\d{1,5})x(\d{1,5}):(\d+(?:,\d+)*)$/.exec(String(text));
    if (!m) return null;
    const cols = Number(m[1]);
    const rows = Number(m[2]);
    if (cols < 1 || rows < 1 || cols * rows > maxCells) return null;
    const f = new Fog(cols, rows);
    let pos = 0;
    let cur = 0;
    for (const part of m[3].split(',')) {
      const n = Number(part);
      if (pos + n > f.bits.length) return null;
      if (cur === 1) f.bits.fill(1, pos, pos + n);
      pos += n;
      cur ^= 1;
    }
    return pos === f.bits.length ? f : null;
  }
}
