import { describe, expect, it } from 'vitest';
import {
  EMPTY_LAYOUT,
  MAX_GADGETS,
  MAX_PINS,
  clampRect,
  closeGadget,
  openGadget,
  pinTool,
  raiseGadget,
  sanitizeLayout,
  setGadgetRect,
  unpinTool,
  type Layout,
} from '../gadgets/layout';

const SIZE = { width: 360, height: 420, minWidth: 240, minHeight: 160 };
const VP = { w: 1280, h: 800 };
const open = (l: Layout, id: string) => {
  const r = openGadget(l, id, SIZE, VP);
  return r.layout;
};

describe('the gadget limit', () => {
  it('allows six and refuses the seventh without changing anything', () => {
    let l = EMPTY_LAYOUT;
    for (let i = 0; i < MAX_GADGETS; i++) l = open(l, `tool-${i}`);
    expect(l.gadgets).toHaveLength(6);
    const seventh = openGadget(l, 'tool-extra', SIZE, VP);
    expect(seventh.ok).toBe(false);
    expect(seventh.layout).toBe(l);
  });
  it('opening an open tool raises it instead of adding another', () => {
    let l = open(open(EMPTY_LAYOUT, 'a'), 'b');
    const r = openGadget(l, 'a', SIZE, VP);
    expect(r.ok).toBe(true);
    l = r.layout;
    expect(l.gadgets).toHaveLength(2);
    expect(l.gadgets.find((g) => g.tool === 'a')?.z).toBe(2);
  });
  it('is still honoured when the limit is full and the tool is already open', () => {
    let l = EMPTY_LAYOUT;
    for (let i = 0; i < MAX_GADGETS; i++) l = open(l, `tool-${i}`);
    expect(openGadget(l, 'tool-0', SIZE, VP).ok).toBe(true);
  });
  it('closing frees a slot', () => {
    let l = EMPTY_LAYOUT;
    for (let i = 0; i < MAX_GADGETS; i++) l = open(l, `tool-${i}`);
    l = closeGadget(l, 'tool-3');
    expect(openGadget(l, 'tool-new', SIZE, VP).ok).toBe(true);
  });
});

describe('placement', () => {
  it('cascades new gadgets so title bars do not overlap exactly', () => {
    const l = open(open(EMPTY_LAYOUT, 'a'), 'b');
    const [a, b] = l.gadgets;
    expect(a && b && (a.x !== b.x || a.y !== b.y)).toBe(true);
  });
  it('fits a small viewport', () => {
    const r = openGadget(EMPTY_LAYOUT, 'a', SIZE, { w: 300, h: 300 });
    const g = r.layout.gadgets[0]!;
    expect(g.x + g.w).toBeLessThanOrEqual(300);
    expect(g.y + g.h).toBeLessThanOrEqual(300);
  });
  it('clamps into the viewport and to the minimum size', () => {
    expect(clampRect({ x: -50, y: 9000, w: 10, h: 10 }, VP, SIZE)).toEqual({
      x: 0,
      y: 800 - 160,
      w: 240,
      h: 160,
    });
    expect(clampRect({ x: 1200, y: 0, w: 5000, h: 5000 }, VP, SIZE)).toEqual({
      x: 0,
      y: 0,
      w: 1280,
      h: 800,
    });
  });
  it('setGadgetRect clamps and keeps the same layout object when nothing changed', () => {
    const l = open(EMPTY_LAYOUT, 'a');
    const g = l.gadgets[0]!;
    expect(setGadgetRect(l, 'a', g, VP, SIZE)).toBe(l);
    const moved = setGadgetRect(l, 'a', { ...g, x: -10 }, VP, SIZE);
    expect(moved.gadgets[0]?.x).toBe(0);
  });
});

describe('stacking', () => {
  it('raise puts a gadget in front and renumbers 1..n', () => {
    let l = open(open(open(EMPTY_LAYOUT, 'a'), 'b'), 'c');
    l = raiseGadget(l, 'a');
    expect(l.gadgets.map((g) => [g.tool, g.z]).sort()).toEqual([
      ['a', 3],
      ['b', 1],
      ['c', 2],
    ]);
    expect(raiseGadget(l, 'a')).toBe(l);
  });
});

describe('pins', () => {
  it('pins once, in order, up to the limit, and unpins', () => {
    let l = pinTool(pinTool(EMPTY_LAYOUT, 'a'), 'b');
    expect(pinTool(l, 'a')).toBe(l);
    expect(l.pins).toEqual(['a', 'b']);
    l = unpinTool(l, 'a');
    expect(l.pins).toEqual(['b']);
    expect(unpinTool(l, 'zzz')).toBe(l);
    let full = EMPTY_LAYOUT;
    for (let i = 0; i < MAX_PINS + 3; i++) full = pinTool(full, `tool-${i}`);
    expect(full.pins).toHaveLength(MAX_PINS);
  });
});

describe('sanitizeLayout', () => {
  it('returns the empty layout for garbage', () => {
    for (const bad of [null, undefined, 3, 'x', [], { gadgets: 'no', pins: 7 }])
      expect(sanitizeLayout(bad)).toEqual(EMPTY_LAYOUT);
  });
  it('drops bad ids and duplicates and caps the counts', () => {
    const gadgets = Array.from({ length: 10 }, (_, i) => ({
      tool: `t-${i}`,
      x: 1,
      y: 2,
      w: 300,
      h: 200,
      z: i + 1,
    }));
    const l = sanitizeLayout({
      gadgets: [
        { tool: 'Bad Id', x: 0, y: 0, w: 1, h: 1, z: 1 },
        { tool: 'dup', x: 0, y: 0, w: 1, h: 1, z: 1 },
        { tool: 'dup', x: 5, y: 5, w: 1, h: 1, z: 2 },
        ...gadgets,
      ],
      pins: ['ok', 'ok', 'UPPER', ...Array.from({ length: 30 }, (_, i) => `p-${i}`)],
    });
    expect(l.gadgets).toHaveLength(MAX_GADGETS);
    expect(l.gadgets.some((g) => g.tool === 'Bad Id')).toBe(false);
    expect(l.gadgets.filter((g) => g.tool === 'dup')).toHaveLength(1);
    expect(l.pins).toHaveLength(MAX_PINS);
    expect(l.pins[0]).toBe('ok');
    expect(l.gadgets.map((g) => g.z)).toEqual([1, 2, 3, 4, 5, 6]);
  });
  it('replaces non-numeric geometry with defaults', () => {
    const l = sanitizeLayout({ gadgets: [{ tool: 'a', x: 'no', y: NaN, w: null, h: undefined }] });
    expect(l.gadgets[0]).toMatchObject({ tool: 'a', x: 24, y: 24, w: 360, h: 420 });
  });
  it('round-trips through JSON', () => {
    const l = pinTool(open(open(EMPTY_LAYOUT, 'a'), 'b'), 'a');
    expect(sanitizeLayout(JSON.parse(JSON.stringify(l)))).toEqual(l);
  });
});
