import { cellCenter, cellDistance, gridDims, inBounds, measure, pixelToCell, validMap } from '../src/grid.js';

const sq = { kind: 'square', size: 50, ox: 10, oy: 20, width: 510, height: 420, unit: 5, unitName: 'ft', diagonal: 'chebyshev' };
const hx = { kind: 'hex', size: 60, ox: 0, oy: 0, width: 600, height: 600, unit: 1, unitName: 'mi' };

describe('square grid', () => {
  it('counts cells from the origin', () => {
    expect(gridDims(sq)).toEqual({ cols: 10, rows: 8 });
  });
  it('round-trips cell centres', () => {
    for (const [c, r] of [[0, 0], [3, 4], [9, 7]]) {
      const p = cellCenter(sq, c, r);
      expect(pixelToCell(sq, p.x, p.y)).toEqual({ c, r });
    }
  });
  it('measures with the three diagonal rules', () => {
    const a = { c: 0, r: 0 };
    const b = { c: 4, r: 3 };
    expect(cellDistance(sq, a, b)).toBe(4);
    expect(cellDistance({ ...sq, diagonal: 'alternate' }, a, b)).toBe(5);
    expect(cellDistance({ ...sq, diagonal: 'euclid' }, a, b)).toBe(5);
    expect(measure(sq, a, b)).toEqual({ cells: 4, value: 20, unitName: 'ft' });
    expect(measure(sq, a, a).value).toBe(0);
  });
  it('knows its bounds', () => {
    expect(inBounds(sq, 9, 7)).toBe(true);
    expect(inBounds(sq, 10, 0)).toBe(false);
    expect(inBounds(sq, -1, 0)).toBe(false);
    expect(inBounds(sq, 1.5, 0)).toBe(false);
  });
});

describe('hex grid', () => {
  it('round-trips cell centres on even and odd rows', () => {
    for (const [c, r] of [[0, 0], [2, 1], [3, 2], [4, 3], [5, 6]]) {
      const p = cellCenter(hx, c, r);
      expect(pixelToCell(hx, p.x, p.y)).toEqual({ c, r });
    }
  });
  it('counts steps between hexes', () => {
    expect(cellDistance(hx, { c: 0, r: 0 }, { c: 0, r: 0 })).toBe(0);
    expect(cellDistance(hx, { c: 2, r: 2 }, { c: 3, r: 2 })).toBe(1);
    // the six neighbours of an even-row cell (odd-r layout)
    for (const [c, r] of [[1, 2], [3, 2], [1, 1], [2, 1], [1, 3], [2, 3]]) {
      expect(cellDistance(hx, { c: 2, r: 2 }, { c, r })).toBe(1);
    }
    expect(measure(hx, { c: 0, r: 0 }, { c: 0, r: 0 })).toEqual({ cells: 0, value: 0, unitName: 'mi' });
  });
});

describe('validMap', () => {
  it('accepts good maps and refuses bad ones', () => {
    expect(validMap(sq)).toBe(true);
    expect(validMap(hx)).toBe(true);
    expect(validMap({ ...sq, size: 2 })).toBe(false);
    expect(validMap({ ...sq, kind: 'tri' })).toBe(false);
    expect(validMap({ ...sq, width: NaN })).toBe(false);
    expect(validMap({ ...sq, size: 8, width: 5000, height: 5000 })).toBe(false); // too many cells
    expect(validMap(null)).toBe(false);
  });
});
