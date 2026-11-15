import { renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PLAYER_CLEAR_VAR, trackPlayerDock, usePlayerPageEnd } from '../pageEnd';

const observers: Array<() => void> = [];
class FakeRO {
  constructor(cb: () => void) {
    observers.push(cb);
  }
  observe() {}
  disconnect() {}
}

function dock(
  attr: string,
  top: number,
  height = 236,
): HTMLElement & { rect: { top: number; height: number } } {
  const e = document.createElement('div') as unknown as HTMLElement & {
    rect: { top: number; height: number };
  };
  e.setAttribute('data-dock', attr);
  e.rect = { top, height };
  e.getBoundingClientRect = () => e.rect as DOMRect;
  return e;
}
const read = () => document.documentElement.style.getPropertyValue(PLAYER_CLEAR_VAR);

describe('page end clearance (D-805)', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', FakeRO);
    Object.defineProperty(window, 'innerHeight', { value: 812, configurable: true });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    observers.length = 0;
    document.documentElement.style.removeProperty(PLAYER_CLEAR_VAR);
  });

  it('a bottom dock publishes the viewport height minus its top edge', () => {
    const stop = trackPlayerDock(dock('bottom-end', 568));
    expect(read()).toBe('244px');
    stop();
    expect(read()).toBe('');
  });

  it('bottom-start publishes too; a top dock publishes nothing', () => {
    const stopA = trackPlayerDock(dock('bottom-start', 600));
    expect(read()).toBe('212px');
    stopA();
    const stopB = trackPlayerDock(dock('top-end', 8));
    expect(read()).toBe('');
    stopB();
  });

  it('a resize callback with a new rect publishes the new value', () => {
    const el = dock('bottom-end', 568);
    const stop = trackPlayerDock(el);
    el.rect = { top: 500, height: 304 };
    for (const cb of observers) cb();
    expect(read()).toBe('312px');
    stop();
  });

  it('moving the dock to the top removes the property, moving it back publishes again', async () => {
    const el = dock('bottom-end', 568);
    const stop = trackPlayerDock(el);
    el.setAttribute('data-dock', 'top-end');
    await Promise.resolve();
    expect(read()).toBe('');
    el.setAttribute('data-dock', 'bottom-start');
    await Promise.resolve();
    expect(read()).toBe('244px');
    stop();
  });

  it('an element without a box publishes nothing', () => {
    const stop = trackPlayerDock(dock('bottom-end', 0, 0));
    expect(read()).toBe('');
    stop();
  });

  it('the callback ref tracks the element, and removes the property when the element goes away', () => {
    const { result } = renderHook(() => usePlayerPageEnd());
    const ref = result.current;
    ref(dock('bottom-end', 568));
    expect(read()).toBe('244px');
    ref(null);
    expect(read()).toBe('');
  });
});
