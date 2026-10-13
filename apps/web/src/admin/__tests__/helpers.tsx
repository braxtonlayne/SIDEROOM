import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { AP, type AdminPerm } from '../permissions';

export { http, HttpResponse, server };

export const permBits = (...names: AdminPerm[]) =>
  names.reduce((a, n) => a | (1n << BigInt(AP[n])), 0n).toString();

export interface Staff {
  perms: AdminPerm[];
  owner?: boolean;
}

/** Serve /admin/me for a staff member holding these platform permissions. */
export function asStaff({ perms, owner = false }: Staff) {
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/admin/me', () =>
      HttpResponse.json({
        is_owner: owner,
        platform_permissions: permBits(...(owner ? (['OWNER'] as AdminPerm[]) : perms)),
        user_id: '1',
      }),
    ),
  );
}

export function openAdmin(section = 'dashboard') {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [`/admin/${section}`] }));
  const view = render(<RouterProvider router={router} />);
  return { router, ...view };
}

export const nav = () => screen.getByRole('navigation', { name: /settings|admin/i });

export async function readJson(request: Request): Promise<unknown> {
  try {
    return await request.clone().json();
  } catch {
    return undefined;
  }
}

export const account = (over: Record<string, unknown> = {}) => ({
  id: '100',
  person_id: '500',
  username: 'alice',
  display_name: 'Alice',
  email: 'alice@example.test',
  email_verified: true,
  state: 'active',
  state_changed_at_ms: 1_700_000_000_000,
  created_at_ms: 1_700_000_000_000,
  flags: 0,
  ...over,
});

export const detail = (over: Record<string, unknown> = {}, acct: Record<string, unknown> = {}) => ({
  account: account(acct),
  credit_balance: 3,
  invited_by: { id: '1', username: 'owner' },
  invites_issued: 2,
  invites_redeemed: 1,
  mfa_enabled: true,
  person_banned: false,
  recent_logins: [{ at_ms: 1_700_000_000_000, ip: '203.0.113.5', success: true }],
  sessions_active: 2,
  suspicion_level: 0,
  ...over,
});
