// SEC-AUD-039 (D-570): both sign-out buttons remove what the session kept on this device before the session ends.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const order = vi.hoisted(() => [] as string[]);
vi.mock('../../../pwa/signOutPurge', () => ({
  purgeOnSignOut: () => {
    order.push('purge');
    return Promise.resolve();
  },
}));
vi.mock('../../../lib/uploadLedger', () => ({ clearUploadsOnSignOut: () => Promise.resolve() }));
vi.mock('../../../api/client', async (orig) => ({
  ...(await orig<typeof import('../../../api/client')>()),
  apiPost: (path: string) => {
    order.push(path);
    return Promise.resolve();
  },
}));

import { logout, logoutAll } from '../api';

describe('sign-out purge', () => {
  beforeEach(() => void order.splice(0));

  it('sign out purges the device first', async () => {
    await logout();
    expect(order).toEqual(['purge', '/sr/auth/logout']);
  });

  it('sign out everywhere purges the device first', async () => {
    await logoutAll();
    expect(order).toEqual(['purge', '/sr/auth/logout-all']);
  });
});
