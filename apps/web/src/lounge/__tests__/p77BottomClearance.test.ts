import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  COMPOSER_CLEAR_VAR,
  trackComposer,
  useBottomClearance,
  useComposerClearance,
} from '../composerClearance';

const observers: Array<() => void> = [];
class FakeRO {
  constructor(cb: () => void) {
    observers.push(cb);
  }
  observe() {}
  disconnect() {}
}

function box(top: number, height = 40): HTMLElement & { rect: { top: number; height: number } } {
  const e = document.createElement('div') as unknown as HTMLElement & {
    rect: { top: number; height: number };
  };
  e.rect = { top, height };
  e.getBoundingClientRect = () => e.rect as DOMRect;
  return e;
}
const read = () => document.documentElement.style.getPropertyValue(COMPOSER_CLEAR_VAR);

describe('bottom clearance (D-800)', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', FakeRO);
    Object.defineProperty(window, 'innerHeight', { value: 800, configurable: true });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    observers.length = 0;
  });

  it('an element without a box (display: none) reports nothing, not the viewport height', () => {
    const stop = trackComposer(box(0, 0));
    expect(read()).toBe('0px');
    stop();
    expect(read()).toBe('');
  });

  it('a bar and a composer: the largest wins; each stop falls back; the last removes the property', () => {
    const stopBar = trackComposer(box(744, 56));
    expect(read()).toBe('56px');
    const stopComposer = trackComposer(box(692, 108));
    expect(read()).toBe('108px');
    stopComposer();
    expect(read()).toBe('56px');
    stopBar();
    expect(read()).toBe('');
  });

  it('a bar that loses its box (keyboard up) drops to 0 on the observer, and comes back', () => {
    const bar = box(744, 56);
    const stop = trackComposer(bar);
    expect(read()).toBe('56px');
    bar.rect = { top: 0, height: 0 };
    observers.forEach((cb) => cb());
    expect(read()).toBe('0px');
    bar.rect = { top: 744, height: 56 };
    observers.forEach((cb) => cb());
    expect(read()).toBe('56px');
    stop();
  });

  it('the bottom-bar hook is the composer hook', () => {
    expect(useBottomClearance).toBe(useComposerClearance);
  });
});
