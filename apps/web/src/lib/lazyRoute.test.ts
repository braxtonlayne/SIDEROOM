import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { reloadOnStaleModule } from './lazyRoute';

describe('reloadOnStaleModule', () => {
  const reload = vi.fn();
  const realLocation = window.location;

  beforeEach(() => {
    sessionStorage.clear();
    reload.mockReset();
    Object.defineProperty(window, 'location', {
      configurable: true,
      value: { ...realLocation, reload },
    });
  });
  afterEach(() => {
    Object.defineProperty(window, 'location', { configurable: true, value: realLocation });
  });

  const stale = () =>
    new TypeError('Failed to fetch dynamically imported module: http://localhost:5173/src/x.tsx');

  it('reloads once when a lazy page module is stale, and not again within the window', () => {
    expect(reloadOnStaleModule(stale(), 1_000_000)).toBe(true);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(reloadOnStaleModule(stale(), 1_005_000)).toBe(false);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('reloads again once the window has passed', () => {
    expect(reloadOnStaleModule(stale(), 1_000_000)).toBe(true);
    expect(reloadOnStaleModule(stale(), 1_020_000)).toBe(true);
    expect(reload).toHaveBeenCalledTimes(2);
  });

  it('leaves every other error to the error boundary', () => {
    expect(reloadOnStaleModule(new Error('boom'), 1_000_000)).toBe(false);
    expect(reloadOnStaleModule('not an error', 1_000_000)).toBe(false);
    expect(reload).not.toHaveBeenCalled();
  });
});
