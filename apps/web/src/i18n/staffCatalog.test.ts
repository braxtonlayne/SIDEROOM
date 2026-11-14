import { afterEach, describe, expect, it, vi } from 'vitest';
import { en } from './messages/en';
import { isStaffLoaded } from './messages/store';
import { resetStaffCatalogForTests, startStaffCatalog } from './staffCatalog';

/**
 * SEC-AUD-187 (D-663): the staff catalog loads like the rest catalog (catalogRetry.test.ts): one failure keeps the
 * door waiting, the second settles it degraded (the Admin app shows keys), and the tries go on in the background.
 */
describe('startStaffCatalog', () => {
  afterEach(() => {
    vi.useRealTimers();
    resetStaffCatalogForTests();
  });

  it('the test setup registered the staff catalog, so the door has nothing to wait for', async () => {
    expect(isStaffLoaded()).toBe(true);
    expect(en['admin.adminApp.section.dashboard']).toBe('Dashboard');
    await expect(startStaffCatalog(() => Promise.reject(new Error('never called')))).resolves.toBe(
      undefined,
    );
  });
});

describe('startStaffCatalog retries (the catalog not yet registered)', () => {
  // The store is a module singleton that setup.ts has already filled: reload it fresh for these cases.
  afterEach(() => {
    vi.useRealTimers();
    vi.resetModules();
  });

  async function fresh() {
    vi.resetModules();
    return import('./staffCatalog');
  }

  it('waits after one failure, settles after the second, and keeps trying in the background', async () => {
    vi.useFakeTimers();
    const mod = await fresh();
    let calls = 0;
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    let settled = false;
    const done = mod
      .startStaffCatalog(() => {
        calls++;
        return Promise.reject(new Error('chunk failed'));
      })
      .then(() => (settled = true));
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

  it('a first failure followed by a success settles without an error', async () => {
    vi.useFakeTimers();
    const mod = await fresh();
    const errors = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    let calls = 0;
    const done = mod.startStaffCatalog(() =>
      ++calls === 1 ? Promise.reject(new Error('dropped')) : Promise.resolve(),
    );
    await vi.advanceTimersByTimeAsync(600);
    await done;
    expect(calls).toBe(2);
    expect(errors).not.toHaveBeenCalled();
    errors.mockRestore();
  });
});
