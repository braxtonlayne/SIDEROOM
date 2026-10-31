import { describe, expect, it } from 'vitest';
import { KeyboardPad, PAD, bit, diffMasks, maskFromGamepad } from '../input';

const pad = (pressed: number[], axes: number[] = [0, 0]) => ({
  buttons: Array.from({ length: 17 }, (_, i) => ({ pressed: pressed.includes(i) })),
  axes,
});

describe('keyboard to RetroPad', () => {
  it('maps the default layout', () => {
    const k = new KeyboardPad();
    expect(k.press('KeyZ')).toBe(true);
    expect(k.press('ArrowLeft')).toBe(true);
    expect(k.press('Enter')).toBe(true);
    expect(k.mask).toBe(bit(PAD.B) | bit(PAD.LEFT) | bit(PAD.START));
    expect(k.release('KeyZ')).toBe(true);
    expect(k.mask).toBe(bit(PAD.LEFT) | bit(PAD.START));
  });

  it('ignores keys that are not on the pad, so the page keeps them', () => {
    const k = new KeyboardPad();
    expect(k.press('KeyM')).toBe(false);
    expect(k.release('KeyM')).toBe(false);
    expect(k.mask).toBe(0);
  });

  it('cancels opposite directions', () => {
    const k = new KeyboardPad();
    k.press('ArrowUp');
    k.press('ArrowDown');
    k.press('ArrowRight');
    expect(k.mask).toBe(bit(PAD.RIGHT));
  });

  it('clear releases everything', () => {
    const k = new KeyboardPad();
    k.press('KeyX');
    k.clear();
    expect(k.mask).toBe(0);
  });
});

describe('gamepad to RetroPad', () => {
  it('maps the standard layout like a SNES pad', () => {
    expect(maskFromGamepad(pad([0]))).toBe(bit(PAD.B));
    expect(maskFromGamepad(pad([1]))).toBe(bit(PAD.A));
    expect(maskFromGamepad(pad([2, 3]))).toBe(bit(PAD.Y) | bit(PAD.X));
    expect(maskFromGamepad(pad([8, 9]))).toBe(bit(PAD.SELECT) | bit(PAD.START));
    expect(maskFromGamepad(pad([12, 15]))).toBe(bit(PAD.UP) | bit(PAD.RIGHT));
  });

  it('turns the left stick into the d-pad past the deadzone only', () => {
    expect(maskFromGamepad(pad([], [-0.9, 0.2]))).toBe(bit(PAD.LEFT));
    expect(maskFromGamepad(pad([], [0.49, -0.49]))).toBe(0);
    expect(maskFromGamepad(pad([], [0.6, 0.7]))).toBe(bit(PAD.RIGHT) | bit(PAD.DOWN));
    expect(maskFromGamepad(pad([12, 13]))).toBe(0); // up + down cancel
  });
});

describe('diffMasks', () => {
  it('lists the buttons that changed, in order', () => {
    expect(diffMasks(0, bit(PAD.A) | bit(PAD.B))).toEqual([
      [PAD.B, true],
      [PAD.A, true],
    ]);
    expect(diffMasks(bit(PAD.A) | bit(PAD.UP), bit(PAD.A))).toEqual([[PAD.UP, false]]);
    expect(diffMasks(5, 5)).toEqual([]);
  });
});
