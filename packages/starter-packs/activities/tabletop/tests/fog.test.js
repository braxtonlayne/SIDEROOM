import { Fog } from '../src/fog.js';

const square = { kind: 'square', size: 50, ox: 0, oy: 0, width: 500, height: 500, unit: 5, unitName: 'ft' };
const hex = { kind: 'hex', size: 60, ox: 0, oy: 0, width: 600, height: 600, unit: 5, unitName: 'ft' };

describe('Fog', () => {
  it('starts clear or covered', () => {
    expect(new Fog(4, 3).hiddenCount()).toBe(0);
    expect(new Fog(4, 3, true).hiddenCount()).toBe(12);
  });
  it('hides and reveals single cells, ignoring out of range', () => {
    const f = new Fog(4, 3);
    f.set(1, 1, true);
    f.set(9, 9, true);
    f.set(-1, 0, true);
    expect(f.isHidden(1, 1)).toBe(true);
    expect(f.hiddenCount()).toBe(1);
    expect(f.isHidden(9, 9)).toBe(false);
    f.set(1, 1, false);
    expect(f.hiddenCount()).toBe(0);
  });
  it('paints rectangles in either corner order and clips them', () => {
    const f = new Fog(10, 10);
    f.paintRect(3, 4, 1, 2, true);
    expect(f.hiddenCount()).toBe(9);
    expect(f.isHidden(1, 2) && f.isHidden(3, 4)).toBe(true);
    f.paintRect(8, 8, 20, 20, true);
    expect(f.hiddenCount()).toBe(9 + 4);
    f.paintRect(0, 0, 9, 9, false);
    expect(f.hiddenCount()).toBe(0);
  });
  it('paints a round brush on a square grid', () => {
    const f = new Fog(11, 11, true);
    f.paintBrush(square, 5, 5, 2, false);
    expect(f.isHidden(5, 5)).toBe(false);
    expect(f.isHidden(7, 5)).toBe(false);
    expect(f.isHidden(8, 5)).toBe(true);
    expect(f.isHidden(7, 7)).toBe(true); // the corner of the box is outside the disc
    expect(f.isHidden(6, 6)).toBe(false);
  });
  it('paints a hex range on a hex grid (radius 1 is the seven cells)', () => {
    const f = new Fog(10, 10, true);
    f.paintBrush(hex, 4, 4, 1, false);
    expect(10 * 10 - f.hiddenCount()).toBe(7);
    expect(f.isHidden(4, 4)).toBe(false);
  });
  it('reveals around several tokens', () => {
    const f = new Fog(11, 11, true);
    f.revealAround(square, [{ c: 1, r: 1 }, { c: 9, r: 9 }], 1);
    expect(f.isHidden(1, 1)).toBe(false);
    expect(f.isHidden(9, 9)).toBe(false);
    expect(f.isHidden(5, 5)).toBe(true);
  });
  it('encodes to run lengths and back', () => {
    const f = new Fog(6, 4);
    f.paintRect(1, 1, 3, 2, true);
    f.set(0, 0, true);
    const text = f.encode();
    expect(text).toMatch(/^6x4:/);
    const g = Fog.decode(text);
    expect(g.cols).toBe(6);
    expect([...g.bits]).toEqual([...f.bits]);
    expect(Fog.decode(new Fog(5, 5).encode()).hiddenCount()).toBe(0);
    expect(Fog.decode(new Fog(5, 5, true).encode()).hiddenCount()).toBe(25);
  });
  it('rejects malformed or wrongly sized encodings', () => {
    for (const bad of ['', 'x', '3x3:9,1', '3x3:4', '3x3:a', '0x3:0', '3x3:5,5', null, undefined, '99999x99999:1']) {
      expect(Fog.decode(bad)).toBeNull();
    }
  });
  it('round-trips random fogs', () => {
    let s = 12345;
    const rnd = () => (s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
    for (let n = 0; n < 50; n++) {
      const f = new Fog(1 + Math.floor(rnd() * 30), 1 + Math.floor(rnd() * 30));
      for (let i = 0; i < f.bits.length; i++) f.bits[i] = rnd() < 0.3 ? 1 : 0;
      expect([...Fog.decode(f.encode()).bits]).toEqual([...f.bits]);
    }
  });
});
