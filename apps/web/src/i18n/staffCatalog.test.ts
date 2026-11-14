import { afterEach, describe, expect, it, vi } from 'vitest';
import { en } from './messages/en';
import { getVersion } from './runtime';
import { resetStaffCatalogForTests, startStaffCatalog } from './staffCatalog';

/**
 * SEC-AUD-187 (D-663): the staff catalog loads like the rest catalog (catalogRetry.test.ts): one failure keeps the
 * door waiting, the second settles it degraded (the Admin app shows keys), the tries go on in the background, and a
 * late success re-renders what showed keys.
 */
describe('startStaffCatalog', () => {
  afterEach(() => {
    vi.useRealTimers();
    resetStaffCatalogForTests();
  });

  it('loads the real staff module (registered into the catalog) and settles', async () => {
    await startStaffCatalog();
    expect(en['admin.adminApp.section.dashboard']).toBe('Dashboard');
  });

  it('waits after one failure, settles after the second, and keeps trying in the background', async () => {
    vi.useFakeTimers();
    let calls = 0;
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    let settled = false;
    const done = startStaffCatalog(() => {
      calls++;
      return Promise.reject(new Error('chunk failed'));
    }).then(() => (settled = true));
    await vi.advanceTimersByTimeAsync(0);
    expect(calls).toBe(1);
    expect(settled).toBe(false);
    await vi.advanceTimersByTimeAsync(600);
    expect(calls).toBe(2);
    await done;
    expect(settled).toBe(true);
    expect(errors).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(2000);
    expect(calls).toBe(3);
    errors.mockRestore();
  });

  it('a first failure followed by a success settles without an error and without a re-render', async () => {
    vi.useFakeTimers();
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const before = getVersion();
    let calls = 0;
    const done = startStaffCatalog(() =>
      ++calls === 1 ? Promise.reject(new Error('dropped')) : Promise.resolve(),
    );
    await vi.advanceTimersByTimeAsync(600);
    await done;
    expect(calls).toBe(2);
    expect(errors).not.toHaveBeenCalled();
    expect(getVersion()).toBe(before);
    errors.mockRestore();
  });

  it('a late success (after the door opened degraded) re-renders the subscribers', async () => {
    vi.useFakeTimers();
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    let calls = 0;
    const done = startStaffCatalog(() =>
      ++calls <= 2 ? Promise.reject(new Error('offline')) : Promise.resolve(),
    );
    await vi.advanceTimersByTimeAsync(600);
    await done;
    const before = getVersion();
    await vi.advanceTimersByTimeAsync(2000);
    await vi.advanceTimersByTimeAsync(0);
    expect(calls).toBe(3);
    expect(getVersion()).toBeGreaterThan(before);
    errors.mockRestore();
  });
});
