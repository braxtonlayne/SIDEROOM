import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import {
  HttpResponse,
  account,
  asStaff,
  detail,
  http,
  openAdmin,
  readJson,
  server,
} from './helpers';
import type { AdminPerm } from '../permissions';

async function openBanDialog(perms: AdminPerm[]) {
  asStaff({ perms });
  server.use(
    http.get('*/api/v1/admin/accounts', () =>
      HttpResponse.json({ items: [account()], next_after: null }),
    ),
    http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())),
  );
  openAdmin('accounts');
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'View alice' }));
  await user.click(await screen.findByRole('button', { name: 'Platform-ban (standard)' }));
  const dialog = await screen.findByRole('dialog', { name: /Platform-ban the Person/ });
  return { user, dialog };
}

describe('the Accounts ban dialog goes through a case (N-25)', () => {
  it('opens a platform case when none is open, then bans from it with the reason category', async () => {
    const calls: { path: string; body: unknown }[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', ({ request }) => {
        calls.push({ path: 'list', body: new URL(request.url).search });
        return HttpResponse.json({ items: [], next_cursor: null });
      }),
      http.post('*/api/v1/moderation/cases', async ({ request }) => {
        calls.push({ path: 'open', body: await readJson(request) });
        return HttpResponse.json({ id: '9300' }, { status: 201 });
      }),
      http.post('*/api/v1/moderation/cases/9300/ban', async ({ request }) => {
        calls.push({ path: 'ban', body: await readJson(request) });
        return HttpResponse.json({ ban_id: '5' });
      }),
    );
    const { user, dialog } = await openBanDialog([
      'STAFF_BASIC',
      'MANAGE_ACCOUNTS',
      'PLATFORM_BAN',
      'MANAGE_CASES',
    ]);
    // Pick a serious reason: the toggle follows the D-211 preset.
    await user.click(within(dialog).getByRole('combobox', { name: 'Reason category' }));
    await user.click(await screen.findByRole('option', { name: 'Fraud or impersonation' }));
    expect(within(dialog).getByLabelText(/Serious ban/)).toBeChecked();
    await user.type(
      within(dialog).getByLabelText(/Reason/, { selector: 'textarea' }),
      'chargebacks',
    );
    await user.click(within(dialog).getByRole('button', { name: 'Ban' }));
    expect(await screen.findByText(/where the person can appeal/)).toBeInTheDocument();
    expect(calls[0]!.body).toContain('subject_account_id=100');
    expect(calls.slice(1)).toEqual([
      {
        path: 'open',
        body: {
          category: 'fraud',
          target_kind: 'user',
          target_id: '100',
          subject_account_id: '100',
          severity: 'serious',
        },
      },
      { path: 'ban', body: { reason_category: 'fraud', severity: 'serious', note: 'chargebacks' } },
    ]);
  });

  it('joins the open platform case instead of opening another', async () => {
    let opened = false;
    let banBody: unknown;
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [{ id: '9400', routing: 'platform' }], next_cursor: null }),
      ),
      http.post('*/api/v1/moderation/cases', () => {
        opened = true;
        return HttpResponse.json({ id: 'x' }, { status: 201 });
      }),
      http.post('*/api/v1/moderation/cases/9400/ban', async ({ request }) => {
        banBody = await readJson(request);
        return HttpResponse.json({ ban_id: '6' });
      }),
    );
    const { user, dialog } = await openBanDialog([
      'STAFF_BASIC',
      'MANAGE_ACCOUNTS',
      'PLATFORM_BAN',
      'MANAGE_CASES',
    ]);
    await user.type(within(dialog).getByLabelText(/Reason/, { selector: 'textarea' }), 'again');
    await user.click(within(dialog).getByRole('button', { name: 'Ban' }));
    await screen.findByText(/where the person can appeal/);
    expect(opened).toBe(false);
    expect(banBody).toEqual({ reason_category: 'other', severity: 'standard', note: 'again' });
  });

  it('keeps the severity-only request for staff without MANAGE_CASES', async () => {
    let sent: unknown;
    server.use(
      http.post('*/api/v1/admin/accounts/100/ban', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ accounts_banned: 1 });
      }),
    );
    const { user, dialog } = await openBanDialog([
      'STAFF_BASIC',
      'MANAGE_ACCOUNTS',
      'PLATFORM_BAN',
    ]);
    await user.type(within(dialog).getByLabelText(/Reason/, { selector: 'textarea' }), 'spam wave');
    await user.click(within(dialog).getByRole('button', { name: 'Ban' }));
    expect(await screen.findByText('1 account(s) banned.')).toBeInTheDocument();
    expect(sent).toMatchObject({ category: 'standard', reason: 'spam wave' });
  });
});
