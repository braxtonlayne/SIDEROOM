import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { repoRoot } from '../support/cli';

/**
 * XC-VIS-002 (settings polish): click through the settings flows like a member would, on the sweep world.
 * Every flow cleans up what it makes. Opt-in with SR_E2E_SWEEP=1 (needs the sweep's owner state and world).
 * Fails on any 5xx or unexpected 4xx response and on console errors.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const world = (): Record<string, string> =>
  JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as Record<string, string>;
const stamp = () => Date.now().toString(36).slice(-5);

function watch(page: Page) {
  const problems: string[] = [];
  page.on('response', (r) => {
    const u = r.url();
    if (!u.includes('/api/') && !u.includes(':8080')) return;
    if (r.status() >= 500 || (r.status() >= 400 && r.status() !== 401 && r.status() !== 404))
      problems.push(`${r.status()} ${r.request().method()} ${u.replace(/^https?:\/\/[^/]+/, '')}`);
  });
  page.on('console', (m) => {
    if (m.type() === 'error' && !/favicon|Failed to load resource/.test(m.text()))
      problems.push(`console: ${m.text().slice(0, 200)}`);
  });
  return problems;
}

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
    await page.getByRole('button', { name: /create role/i }).click();
    const name = page.getByLabel(/role name/i);
    await expect(name).toBeVisible();
    const role = `Polish ${stamp()}`;
    await name.fill(role);
    await page.getByRole('button', { name: /^save( changes)?$/i }).click();
    await expect(page.getByRole('button', { name: role, exact: true })).toBeVisible();
    await page.getByRole('button', { name: /delete role/i }).first().click();
    await page.getByRole('dialog').getByRole('button', { name: /delete/i }).click();
    await expect(page.getByRole('button', { name: role, exact: true })).toHaveCount(0);
    expect(problems).toEqual([]);
  });

  test('server: create and delete a channel', async ({ page }) => {
    const problems = watch(page);
    await page.goto(`/settings/server/${world().guildId}/channels`);
    await page.getByRole('button', { name: /^create channel$/i }).click();
    const dialog = page.getByRole('dialog');
    const ch = `polish-${stamp()}`;
    await dialog.getByLabel(/channel name/i).fill(ch);
    await dialog.getByRole('button', { name: /^create( channel)?$/i }).click();
    await expect(page.getByRole('button', { name: `# ${ch}` })).toBeVisible();
    await page.getByRole('button', { name: `# ${ch}` }).click();
    await page.getByRole('button', { name: /delete channel/i }).click();
    await page.getByRole('dialog').getByRole('button', { name: /delete/i }).click();
    await expect(page.getByRole('button', { name: `# ${ch}` })).toHaveCount(0);
    expect(problems).toEqual([]);
  });

  test('server: overview save and reset', async ({ page }) => {
    const problems = watch(page);
    await page.goto(`/settings/server/${world().guildId}/overview`);
    const name = page.getByLabel(/server name/i).first();
    const before = await name.inputValue();
    await name.fill(`${before}!`);
    await expect(page.getByRole('button', { name: /reset/i })).toBeVisible();
    await page.getByRole('button', { name: /reset/i }).click();
    await expect(name).toHaveValue(before);
    await name.fill(`${before}!`);
    await page.getByRole('button', { name: /^save( changes)?$/i }).click();
    await expect(page.getByRole('button', { name: /^save( changes)?$/i })).toHaveCount(0);
    await name.fill(before);
    await page.getByRole('button', { name: /^save( changes)?$/i }).click();
    await expect(page.getByRole('button', { name: /^save( changes)?$/i })).toHaveCount(0);
    expect(problems).toEqual([]);
  });

  test('server: invites, members kick and ban dialogs open and cancel', async ({ page }) => {
    const problems = watch(page);
    await page.goto(`/settings/server/${world().guildId}/invites`);
    await expect(page.getByRole('heading', { name: /invites/i }).first()).toBeVisible();
    await page.goto(`/settings/server/${world().guildId}/members`);
    const row = page.getByRole('row', { name: new RegExp(world().memberName ? 'Juniper' : '.') }).first();
    await row.getByRole('button').last().click();
    for (const what of [/kick/i, /ban/i]) {
      const item = page.getByRole('menuitem', { name: what }).first();
      if (!(await item.isVisible().catch(() => false))) {
        await page.keyboard.press('Escape');
        await row.getByRole('button').last().click();
      }
      await page.getByRole('menuitem', { name: what }).first().click();
      const dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      await dialog.getByRole('button', { name: /cancel/i }).click();
      await expect(dialog).toHaveCount(0);
      await row.getByRole('button').last().click();
    }
    expect(problems).toEqual([]);
  });

  test('user: profile save and reset', async ({ page }) => {
    const problems = watch(page);
    await page.goto('/settings/user/profile');
    const pronouns = page.getByLabel(/pronouns/i).first();
    await pronouns.fill('they/them');
    await page.getByRole('button', { name: /reset/i }).click();
    await expect(pronouns).toHaveValue('');
    expect(problems).toEqual([]);
  });

  test('developers: create an app, visit each tab, delete it', async ({ page }) => {
    const problems = watch(page);
    await page.goto('/developers/applications');
    await page.getByRole('button', { name: /new application/i }).click();
    const dialog = page.getByRole('dialog');
    const app = `Polish App ${stamp()}`;
    await dialog.getByLabel(/name/i).first().fill(app);
    const agree = dialog.getByRole('checkbox');
    if (await agree.count()) await agree.first().check();
    await dialog.getByRole('button', { name: /^create$/i }).click();
    await expect(page.getByRole('heading', { name: app })).toBeVisible();
    for (const tab of ['OAuth2', 'Bot', 'Installation', 'Commands', 'Emojis', 'Rich Presence', 'Activities', 'Review', 'General']) {
      await page.getByRole('tab', { name: tab, exact: true }).or(page.getByRole('link', { name: tab, exact: true })).first().click();
      await page.waitForTimeout(300);
    }
    await page.getByRole('textbox', { name: /description/i }).fill('Made by the polish flow.');
    await page.getByRole('button', { name: /save changes/i }).click();
    await page.getByRole('button', { name: /delete application/i }).click();
    const confirm = page.getByRole('dialog');
    const typed = confirm.getByRole('textbox');
    if (await typed.count()) await typed.fill(app);
    await confirm.getByRole('button', { name: /delete/i }).click();
    await expect(page).toHaveURL(/\/developers\/applications$/);
    expect(problems).toEqual([]);
  });
});
