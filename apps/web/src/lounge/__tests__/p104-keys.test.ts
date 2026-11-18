import { describe, expect, it } from 'vitest';
import { yieldsToSelect } from '../shortcuts';

const select = () => document.createElement('select');
const MODS = [
  { altKey: false, shiftKey: false, ctrlKey: false },
  { altKey: true, shiftKey: false, ctrlKey: false },
  { altKey: false, shiftKey: true, ctrlKey: false },
  { altKey: false, shiftKey: false, ctrlKey: true },
  { altKey: true, shiftKey: true, ctrlKey: false },
  { altKey: true, shiftKey: false, ctrlKey: true },
];

describe('yieldsToSelect (D-995)', () => {
  for (const key of ['ArrowUp', 'ArrowDown']) {
    it(`is true for a select with ${key}, with and without Alt, Shift, Ctrl`, () => {
      for (const m of MODS) expect(yieldsToSelect({ key, ...m }, select())).toBe(true);
    });
  }

  it('is false for a select with other keys', () => {
    for (const key of ['k', 'Escape', 'ArrowLeft', 'ArrowRight', 'Enter'])
      expect(yieldsToSelect({ key }, select())).toBe(false);
  });

  it('is false for other targets with ArrowDown', () => {
    for (const el of [
      document.createElement('input'),
      document.createElement('textarea'),
      document.createElement('button'),
      document.body,
      null,
    ])
      expect(yieldsToSelect({ key: 'ArrowDown' }, el)).toBe(false);
  });
});
