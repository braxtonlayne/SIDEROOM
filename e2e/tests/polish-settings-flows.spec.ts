import { expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { repoRoot } from '../support/cli';
import { test } from '../support/fixtures';

/**
 * XC-VIS-002 (settings polish): click through the settings flows like a member would, on the sweep world.
 * Every flow cleans up what it makes. Opt-in with SR_E2E_SWEEP=1 (needs the sweep's owner state and world).
 * Fails on any 4xx/5xx API answer (bar the expected ones) and on console errors.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const world = (): Record<string, string> =>
  JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as Record<string, string>;
const stamp = () => Date.now().toString(36).slice(-5);

/** 4xx/5xx API answers and console errors. A missing profile page or watch party (404) is expected. */
function watch(page: Page) {
  const problems: string[] = [];
  page.on('response', (r) => {
    const u = r.url();
    if (!u.includes('/api/')) return;
    if (r.status() === 404 && /profile-page|watch-party/.test(u)) return;
    if (r.status() >= 400 && r.status() !== 401)
      problems.push(`${r.status()} ${r.request().method()} ${u.replace(/^https?:\/\/[^/]+/, '')}`);
  });
  page.on('console', (m) => {
    if (m.type() === 'error' && !/favicon|Failed to load resource/.test(m.text()))
      problems.push(`console: ${m.text().slice(0, 200)}`);
  });
  return problems;
}

/** The save bar is gone: no Save changes, and no Saving... either (it hides only once the save has landed). */
const settled = (page: Page) =>
  expect(page.getByRole('button', { name: /^Sav(e changes|ing)/ })).toHaveCount(0);

test.describe('settings polish flows', () => {
  test.skip(!ON, 'opt-in: SR_E2E_SWEEP=1');
  test.use({
    storageState: resolve(STATE, 'owner.json'),
    baseURL: ORIGIN,
    viewport: { width: 1280, height: 800 },
  });

  test('server: create, rename and delete a role', async ({ page }) => {
    const problems = watch(page);
    await page.goto(`/settings/server/${world().guildId}/roles`);
    await page.getByRole('button', { name: 'Create Role' }).click();
    // The new role opens in the editor.
    const name = page.getByRole('textbox', { name: 'Role name' });
    await expect(name).toHaveValue('new role');
    const role = `Polish ${stamp()}`;
    await name.fill(role);
    await page.getByRole('button', { name: 'Save changes' }).click();
    const row = page
      .getByRole('list', { name: 'Roles' })
      .getByRole('button', { name: role, exact: true });
    await expect(row).toBeVisible();
    await page.getByRole('button', { name: `Delete role ${role}` }).click();
    await page
      .getByRole('dialog', { name: `Delete ${role}` })
      .getByRole('button', { name: 'Delete role' })
      .click();
    await expect(row).toHaveCount(0);
    expect(problems).toEqual([]);
  });

  test('server: create and delete a channel', async ({ page }) => {
    const problems = watch(page);
    const url = `/settings/server/${world().guildId}/channels`;
    await page.goto(url);
    await page.getByRole('button', { name: 'Create Channel', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Create Channel' });
    const ch = `polish-${stamp()}`;
    await dialog.getByRole('textbox', { name: 'Channel name' }).fill(ch);
    await dialog.getByRole('button', { name: 'Create Channel' }).click();
    await expect(dialog).toHaveCount(0);
    // The owner stays in Settings and the new channel opens in the editor.
    await expect(page).toHaveURL(/\/settings\/server\/.+\/channels/);
    const row = page.getByRole('button', { name: `# ${ch}` });
    await expect(row).toBeVisible();
    await expect(page.getByRole('textbox', { name: 'Channel name' })).toHaveValue(ch);
    await page.getByRole('button', { name: 'Delete channel' }).click();
    await page
      .getByRole('dialog', { name: /^Delete / })
      .getByRole('button', { name: /^Delete/ })
      .click();
    await expect(row).toHaveCount(0);
    expect(problems).toEqual([]);
  });

  test('server: overview reset, save and restore', async ({ page }) => {
    const problems = watch(page);
    await page.goto(`/settings/server/${world().guildId}/overview`);
    const name = page.getByRole('textbox', { name: 'Server name' });
    await expect(name).not.toHaveValue('');
    const before = await name.inputValue();
    await name.fill(`${before}?`);
    await page.getByRole('button', { name: 'Reset' }).click();
    await expect(name).toHaveValue(before);
    await settled(page);
    await name.fill(`${before}?`);
    await page.getByRole('button', { name: 'Save changes' }).click();
    await settled(page);
    await expect(name).toHaveValue(`${before}?`);
    await name.fill(before);
    await page.getByRole('button', { name: 'Save changes' }).click();
    await settled(page);
    expect(problems).toEqual([]);
  });

  test('server: invite dialog, kick and ban dialogs open and cancel', async ({ page }) => {
    const problems = watch(page);
    const g = world().guildId;
    await page.goto(`/settings/server/${g}/invites`);
    await page.getByRole('button', { name: 'Create invite' }).click();
    const invite = page.getByRole('dialog', { name: /^Invite people to / });
    await expect(invite.getByRole('combobox', { name: 'Invite to channel' })).toBeVisible();
    await invite.getByRole('button', { name: 'Close' }).click();
    await expect(invite).toHaveCount(0);

    await page.goto(`/settings/server/${g}/members`);
    const member = page.getByRole('button', { name: /^Details for (?!.*owner)/ }).first();
    const who = ((await member.textContent()) ?? '').trim();
    for (const what of ['Kick', 'Ban']) {
      // Behind the row's "..." menu (XC-VIS-003 T4).
      await page.getByRole('button', { name: `More actions for ${who}`, exact: true }).click();
      await page.getByRole('menuitem', { name: `${what} ${who}`, exact: true }).click();
      const dialog = page.getByRole('dialog', { name: `${what} ${who}` });
      await expect(dialog.getByRole('textbox', { name: 'Reason' })).toBeVisible();
      await dialog.getByRole('button', { name: 'Cancel' }).click();
      await expect(dialog).toHaveCount(0);
    }
    // The member is still there.
    await expect(member).toBeVisible();
    expect(problems).toEqual([]);
  });

  test('user: profile edit and reset', async ({ page }) => {
    const problems = watch(page);
    await page.goto('/settings/user/profile');
    const pronouns = page.getByRole('textbox', { name: 'Pronouns' });
    const before = await pronouns.inputValue();
    await pronouns.fill('they/them');
    await page.getByRole('button', { name: 'Reset' }).click();
    await expect(pronouns).toHaveValue(before);
    await settled(page);
    expect(problems).toEqual([]);
  });

  test('admin: switch a feature module off and back on', async ({ page }) => {
    const problems = watch(page);
    await page.goto('/admin/features');
    const sw = page.getByRole('switch', { name: 'Poll templates enabled' });
    await expect(sw).toBeChecked();
    try {
      await sw.click();
      const off = page.getByRole('dialog', { name: 'Disable Poll templates' });
      await off.getByRole('textbox', { name: /reason/i }).fill('Settings polish flow check');
      await off.getByRole('button', { name: 'Disable' }).click();
      await expect(sw).not.toBeChecked();
    } finally {
      // Always leave the module on: the sweep world is shared.
      if (!(await sw.isChecked())) {
        await sw.click();
        const on = page.getByRole('dialog', { name: 'Enable Poll templates' });
        await on.getByRole('textbox', { name: /reason/i }).fill('Settings polish flow check');
        await on.getByRole('button', { name: 'Enable' }).click();
      }
    }
    await expect(sw).toBeChecked();
    expect(problems).toEqual([]);
  });

  test('developers: create an app, visit each tab, save, delete it', async ({ page, owner }) => {
    const problems = watch(page);
    await page.goto('/developers/applications');
    await page.getByRole('button', { name: 'New application' }).click();
    const create = page.getByRole('dialog', { name: 'New application' });
    const app = `Polish App ${stamp()}`;
    await create.getByRole('textbox', { name: 'Name' }).fill(app);
    await create.getByRole('checkbox').check();
    await create.getByRole('button', { name: 'Create' }).click();
    const creds = page.getByRole('dialog', { name: 'Save your credentials' });
    await expect(creds.getByRole('textbox', { name: 'Client secret' })).not.toHaveValue('');
    // Done opens the new application.
    await creds.getByRole('button', { name: 'Done' }).click();
    await expect(page.getByRole('heading', { name: app, level: 1 })).toBeVisible();
    const tabs = page.getByRole('tablist', { name: 'Application settings' });
    for (const tab of [
      'OAuth2',
      'Bot',
      'Installation',
      'Commands',
      'Emojis',
      'Rich Presence',
      'Activities',
      'Review',
      'General',
    ]) {
      await tabs.getByRole('tab', { name: tab, exact: true }).click();
      await expect(page.getByRole('tabpanel', { name: tab })).toBeVisible();
    }
    await page.getByRole('textbox', { name: 'Description' }).fill('Made by the polish flow.');
    await page.getByRole('button', { name: 'Save changes' }).first().click();
    await expect(page.getByText(/saved/i).first()).toBeVisible();
    await page.reload();
    await expect(page.getByRole('textbox', { name: 'Description' })).toHaveValue(
      'Made by the polish flow.',
    );
    await page.getByRole('button', { name: 'Delete application' }).click();
    const confirm = page.getByRole('dialog', { name: new RegExp(`^Delete ${app}`) });
    await confirm.getByRole('textbox', { name: /type/i }).fill(app);
    await confirm.getByLabel(/your password/i).fill(owner.password);
    await confirm.getByRole('button', { name: 'Delete application' }).click();
    await expect(page).toHaveURL(/\/developers\/applications$/);
    await expect(page.getByRole('link', { name: new RegExp(app) })).toHaveCount(0);
    expect(problems).toEqual([]);
  });
});
