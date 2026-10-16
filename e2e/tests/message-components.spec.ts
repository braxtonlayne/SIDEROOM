import type { APIRequestContext, Page } from '@playwright/test';
import { apiPost, expect, test } from '../support/fixtures';

/**
 * Message components in the browser (D-243, D-244, ADM-STF-004, G-01): the cards the system user posts
 * render their buttons, a press reaches the server, and the server rewrites the card so the answered
 * buttons do not come back after a reload.
 *
 *  - an AutoMod alert card in a server (Delete message, Kick, Ban, Mark resolved, the Timeout select);
 *  - a staff appointment offer in a DM (Accept / Decline);
 *  - an application tester invitation in a DM (Accept / Decline).
 *
 * The staff offer needs `SR_ADMIN_REQUIRE_REAUTH=0` on the server (staff changes otherwise want a fresh
 * second factor, which the e2e Owner has not enrolled).
 */

const API = '/api/v1';

async function apiGet<T>(request: APIRequestContext, path: string): Promise<T> {
  const res = await request.get(`${API}${path}`);
  if (!res.ok()) throw new Error(`GET ${path} -> ${res.status()}: ${await res.text()}`);
  return (await res.json()) as T;
}

/** The DM channel the system user (id 1) opened with the signed-in account. */
async function systemDm(request: APIRequestContext): Promise<string> {
  let found: string | undefined;
  await expect
    .poll(async () => {
      const dms = await apiGet<{ id: string; recipients?: { id: string }[] }[]>(
        request,
        '/users/@me/channels',
      );
      found = dms.find((c) => c.recipients?.some((r) => r.id === '1'))?.id;
      return found ?? null;
    })
    .not.toBeNull();
  return found!;
}

/** The message article that holds a button named `name`. */
function cardWith(page: Page, name: string | RegExp) {
  return page
    .getByRole('article')
    .filter({ has: page.getByRole('button', { name }) })
    .last();
}

test('AutoMod alert card: buttons render, a press is run by the server and the card is rewritten', async ({
  scenario,
}) => {
  const { a, b, guildId, channelId } = await scenario.ownerAndMember();
  const word = `zorbleflux${Date.now().toString(36)}`;
  await apiPost(a.context.request, `/guilds/${guildId}/auto-moderation/rules`, {
    name: 'E2E keyword alert',
    event_type: 1,
    trigger_type: 1,
    trigger_metadata: { keyword_filter: [word] },
    actions: [{ type: 2, metadata: { channel_id: channelId } }],
    enabled: true,
  });
  await apiPost(b.context.request, `/channels/${channelId}/messages`, {
    content: `hello ${word} there`,
  });

  const page = a.page;
  await page.goto(`/channels/${guildId}/${channelId}`);
  const card = cardWith(page, 'Mark resolved');
  await expect(card).toBeVisible({ timeout: 20_000 });
  await expect(card).toContainText('AutoMod flagged a message');
  for (const label of ['Delete message', 'Kick', 'Ban', 'Mark resolved'])
    await expect(card.getByRole('button', { name: label })).toBeVisible();
  await expect(card.getByRole('combobox')).toBeVisible();

  // Delete message: the flagged message goes, the card loses that button and names the presser.
  await card.getByRole('button', { name: 'Delete message' }).click();
  await expect(card.getByRole('button', { name: 'Delete message' })).toHaveCount(0);
  await expect(card).toContainText('Deleted by');
  await expect(page.getByText(`hello ${word} there`)).toHaveCount(0);

  // Mark resolved: every remaining control goes, and stays gone after a reload (server state).
  await card.getByRole('button', { name: 'Mark resolved' }).click();
  await expect(card).toContainText('Resolved by');
  await page.reload();
  const after = page.getByRole('article').filter({ hasText: 'Resolved by' }).last();
  await expect(after).toBeVisible({ timeout: 20_000 });
  await expect(after.getByRole('button', { name: 'Kick' })).toHaveCount(0);
  await expect(after.getByRole('combobox')).toHaveCount(0);
});

test('staff offer card: Decline reaches the server and the buttons stay gone after a reload', async ({
  scenario,
}) => {
  const { a, b } = await scenario.ownerAndMember();
  const roles = await apiGet<{ id: string; name: string }[]>(a.context.request, '/admin/staff/roles');
  const role = roles.find((r) => !/owner/i.test(r.name)) ?? roles[0];
  if (!role) throw new Error('no staff roles');
  await apiPost(a.context.request, '/admin/staff/offers', {
    account_id: b.user.id,
    role_ids: [role.id],
    reason: 'e2e offer',
  });

  const dm = await systemDm(b.context.request);
  const page = b.page;
  await page.goto(`/channels/@me/${dm}`);
  const card = cardWith(page, 'Decline');
  await expect(card).toContainText('Staff appointment', { timeout: 20_000 });
  await expect(card.getByRole('button', { name: 'Accept' })).toBeVisible();
  await card.getByRole('button', { name: 'Decline' }).click();
  await expect(card.getByRole('button', { name: 'Decline' })).toHaveCount(0, { timeout: 15_000 });

  await page.reload();
  const after = page.getByRole('article').filter({ hasText: 'Staff appointment' }).last();
  await expect(after).toContainText('You declined this offer.', { timeout: 20_000 });
  await expect(after.getByRole('button', { name: 'Accept' })).toHaveCount(0);
  await expect(after.getByRole('button', { name: 'Decline' })).toHaveCount(0);
});

test('tester invitation card: Accept reaches the server and the buttons stay gone after a reload', async ({
  scenario,
}) => {
  const { a, b } = await scenario.ownerAndMember();
  const app = await apiPost<{ id: string }>(a.context.request, '/applications', {
    name: `Tester card ${Date.now().toString(36)}`,
  });
  await apiPost(a.context.request, `/applications/${app.id}/testers`, {
    username: b.user.username,
  });

  const dm = await systemDm(b.context.request);
  const page = b.page;
  await page.goto(`/channels/@me/${dm}`);
  const card = cardWith(page, 'Accept');
  await expect(card).toContainText('Tester invitation', { timeout: 20_000 });
  await card.getByRole('button', { name: 'Accept' }).click();
  await expect(card.getByRole('button', { name: 'Accept' })).toHaveCount(0, { timeout: 15_000 });

  const testers = await apiGet<{ user_id: string; state: string }[]>(
    a.context.request,
    `/applications/${app.id}/testers`,
  );
  expect(testers.find((t) => t.user_id === b.user.id)?.state).toBe('accepted');

  await page.reload();
  const after = page.getByRole('article').filter({ hasText: 'Tester invitation' }).last();
  await expect(after).toContainText('You accepted this invitation.', { timeout: 20_000 });
  await expect(after.getByRole('button', { name: 'Accept' })).toHaveCount(0);
  await expect(after.getByRole('button', { name: 'Decline' })).toHaveCount(0);
});
