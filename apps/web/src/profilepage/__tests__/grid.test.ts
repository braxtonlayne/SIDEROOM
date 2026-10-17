import { addBlock, cellAt, fits, findSpot, moveBlock, newId, resizeBlock, rowsFor } from '../grid';
import type { Block } from '../types';

const b = (id: string, x: number, y: number, w: number, h: number): Block => ({
  id,
  type: 'custom',
  x,
  y,
  w,
  h,
  config: {},
});

describe('profile grid (EXT-PRF-005)', () => {
  const blocks = [b('a', 0, 0, 6, 3), b('c', 6, 0, 6, 3)];

  it('moves a block into free space and clamps it to the grid', () => {
    const moved = moveBlock(blocks, 'a', 0, 4);
    expect(moved[0]).toMatchObject({ x: 0, y: 4 });
    // Past the right edge: clamped to the last column that fits.
    expect(moveBlock(blocks, 'a', 30, 5)[0]).toMatchObject({ x: 6, y: 5 });
    expect(moveBlock(blocks, 'a', -3, -3)[0]).toMatchObject({ x: 0, y: 0 });
  });

  it('refuses a move or a resize that would overlap another block', () => {
    expect(moveBlock(blocks, 'a', 3, 0)).toEqual(blocks);
    expect(resizeBlock(blocks, 'a', 7, 3)).toEqual(blocks);
    expect(resizeBlock(blocks, 'a', 6, 5)[0]).toMatchObject({ w: 6, h: 5 });
    expect(resizeBlock(blocks, 'a', 0, 0)[0]).toMatchObject({ w: 1, h: 1 });
    expect(moveBlock(blocks, 'nope', 1, 1)).toEqual(blocks);
  });

  it('checks fit against the grid and the other blocks', () => {
    expect(fits(blocks, { x: 0, y: 3, w: 12, h: 2 }, null)).toBe(true);
    expect(fits(blocks, { x: 0, y: 2, w: 12, h: 2 }, null)).toBe(false);
    expect(fits(blocks, { x: 8, y: 3, w: 6, h: 2 }, null)).toBe(false);
    expect(fits(blocks, { x: 0, y: 59, w: 3, h: 2 }, null)).toBe(false);
    // A block does not collide with itself.
    expect(fits(blocks, { x: 0, y: 0, w: 6, h: 3 }, 'a')).toBe(true);
  });

  it('finds the first free spot and adds blocks there with unique ids', () => {
    expect(findSpot(blocks, 6, 3)).toEqual({ x: 0, y: 3 });
    const next = addBlock(blocks, 'about');
    expect(next).not.toBeNull();
    expect(next![2]).toMatchObject({ type: 'about', x: 0, y: 3, w: 6, h: 4, id: 'about-1' });
    expect(newId(next!, 'about')).toBe('about-2');
    expect(newId([], 'top_friends')).toBe('top-friends-1');
  });

  it('fills the grid and then says there is no room', () => {
    let cur: Block[] = [];
    for (let i = 0; i < 30; i++) {
      const next = addBlock(cur, 'header');
      if (!next) break;
      cur = next;
    }
    // Sixty rows of three: twenty full-width headers fit, a twenty-first does not.
    expect(cur).toHaveLength(20);
    expect(addBlock(cur, 'header')).toBeNull();
  });

  it('maps a point to a cell and sizes the canvas', () => {
    expect(cellAt(0, 0, 50, 40)).toEqual({ x: 0, y: 0 });
    expect(cellAt(125, 85, 50, 40)).toEqual({ x: 2, y: 2 });
    expect(cellAt(9999, 9999, 50, 40).x).toBe(11);
    expect(rowsFor([])).toBe(11);
    expect(rowsFor([b('a', 0, 20, 4, 4)])).toBe(27);
  });
});
