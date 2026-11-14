import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { COMPOSER_CLEAR_VAR, trackComposer } from '../composerClearance';

const observers: Array<() => void> = [];
class FakeRO {
  cb: () => void;
  constructor(cb: () => void) {
    this.cb = cb;
    observers.push(cb);
  }
  observe() {}
  disconnect() {}
}

function el(top: number): HTMLElement {
  const e = document.createElement('div');
  e.getBoundingClientRect = () => ({ top }) as DOMRect;
  return e;
}
const read = () => document.documentElement.style.getPropertyValue(COMPOSER_CLEAR_VAR);

describe('composer clearance (D-789)', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', FakeRO);
    Object.defineProperty(window, 'innerHeight', { value: 800, configurable: true });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    observers.length = 0;
  });

  it('writes the distance from the viewport bottom, largest wins, removed on the last unmount', () => {
    expect(read()).toBe('');
    const stopA = trackComposer(el(740));
    expect(read()).toBe('60px');
    const stopB = trackComposer(el(700));
    expect(read()).toBe('100px');
    stopB();
    expect(read()).toBe('60px');
    stopA();
    expect(read()).toBe('');
  });

  it('follows a resize of the viewport', () => {
    const stop = trackComposer(el(740));
    Object.defineProperty(window, 'innerHeight', { value: 600, configurable: true });
    window.dispatchEvent(new Event('resize'));
    expect(read()).toBe('0px');
    stop();
  });
});
