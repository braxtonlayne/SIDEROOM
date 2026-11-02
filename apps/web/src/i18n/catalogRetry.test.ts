import { afterEach, describe, expect, it, vi } from 'vitest';

/**
 * XC-VIS-001: a failed rest-catalog chunk is tried again before the gates open degraded, and once more in the
 * background, so one dropped request does not leave raw message keys on every screen.
 */
describe('startFullCatalog retries', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.doUnmock('./messages/en.rest');
    vi.resetModules();
  });

  it('keeps the gate shut after one failure and opens it after the second', async () => {
    vi.useFakeTimers();
    vi.resetModules();
    let calls = 0;
    vi.doMock('./messages/en.rest', () => {
      calls++;
      throw new Error('chunk failed');
    });
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const rt = await import('./runtime');
    rt.startFullCatalog();
    await vi.advanceTimersByTimeAsync(0);
    expect(calls).toBe(1);
    expect(rt.fullCatalogSettled()).toBe(false);
    await vi.advanceTimersByTimeAsync(600);
    expect(calls).toBe(2);
    expect(rt.fullCatalogSettled()).toBe(true);
    expect(errors).toHaveBeenCalledTimes(1);
    // Still trying in the background.
    await vi.advanceTimersByTimeAsync(2000);
    expect(calls).toBe(3);
    errors.mockRestore();
  });
});
