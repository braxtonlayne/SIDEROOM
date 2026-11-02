import { expect, test, type Page } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

/**
 * XC-VIS-002 (lounge polish): the message flows a member uses every day, against the running sweep world.
 * Opt-in like page-sweep.spec.ts (SR_E2E_SWEEP=1); signs in as the sweep owner. Every step runs even when an
 * earlier one fails, and the log says which ones broke; the messages it posts are deleted at the end.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
test.skip(!ON, 'Opt-in: set SR_E2E_SWEEP=1.');

const STATE = resolve(process.cwd(), '.state', 'sweep');
const OUT =
  process.env.SR_SWEEP_OUT ?? resolve(process.cwd(), '..', '..', 'visual-sweep-shots', 'lounge');

test.use({
  baseURL: 'http://localhost:5173',
  viewport: { width: 1280, height: 800 },
  storageState: resolve(STATE, 'owner.json'),
  bypassCSP: true,
});

test('send, edit, react, reply, pin, thread, attach and delete a message', async ({ page }) => {
  test.setTimeout(240_000);
  const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8'));
  const log: string[] = [];
  const failed: string[] = [];
  page.on('response', (r) => {
    if (r.status() >= 400 && r.url().includes('/api/'))
      log.push(`http ${r.status()} ${r.request().method()} ${r.url()}`);
  });
  page.on('pageerror', (e) => log.push(`pageerror ${e.message}`));
  const step = async (name: string, fn: () => Promise<void>) => {
    try {
      await fn();
      log.push(`ok   ${name}`);
    } catch (e) {
      failed.push(name);
      log.push(`FAIL ${name}: ${String(e).split('\n')[0]}`);
      await page
        .screenshot({ path: `${OUT}-flow-fail-${name.replace(/\W+/g, '-')}.png` })
        .catch(() => {});
      await page.keyboard.press('Escape').catch(() => {});
    }
  };
  const stamp = Date.now().toString(36);
  const text = `polish flow ${stamp}`;
  const msg = (p: Page, s: string) =>
    p
      .locator('.lng-msg')
      .filter({ has: p.locator('.lng-msg-content', { hasText: s }) })
      .last();
  const menu = async (s: string, item: string) => {
    await msg(page, s).click({ button: 'right' });
    await page.getByRole('menuitem', { name: item }).click();
  };

  await page.goto(`/channels/${world.guildId}/${world.channelId}`);
  const composer = page.getByRole('textbox', { name: /^Message #/ });
  await composer.waitFor({ timeout: 30_000 });

  await step('send', async () => {
    await composer.fill(text);
    await composer.press('Enter');
    await expect(msg(page, text)).toBeVisible();
  });
  await step('edit', async () => {
    await msg(page, text).hover();
    await page.getByRole('button', { name: 'Edit', exact: true }).last().click();
    const box = msg(page, text).getByRole('textbox');
    await box.fill(`${text} edited`);
    await box.press('Enter');
    await expect(msg(page, `${text} edited`)).toContainText('(edited)');
  });
  await step('react', async () => {
    await msg(page, text).hover();
    await page
      .getByRole('button', { name: /^React with/ })
      .last()
      .click();
    await expect(msg(page, text).locator('.lng-reaction, [aria-pressed]').first()).toBeVisible();
  });
  await step('reply', async () => {
    await msg(page, text).hover();
    await page.getByRole('button', { name: 'Reply', exact: true }).last().click();
    await composer.fill(`reply ${stamp}`);
    await composer.press('Enter');
    await expect(msg(page, `reply ${stamp}`)).toContainText(text);
  });
  await step('pin', async () => {
    await menu(text, 'Pin Message');
    const confirm = page.getByRole('dialog').getByRole('button', { name: /pin/i });
    if (await confirm.isVisible().catch(() => false)) await confirm.click();
    await page
      .getByRole('button', { name: /pinned/i })
      .first()
      .click();
    await expect(
      page
        .getByRole('dialog')
        .or(page.locator('[role=region], [role=complementary]'))
        .getByText(text)
        .first(),
    ).toBeVisible();
    await page.screenshot({ path: `${OUT}-flow-pins.png` });
    await page.keyboard.press('Escape');
  });
  await step('thread', async () => {
    await menu(text, 'Create Thread');
    await page.screenshot({ path: `${OUT}-flow-thread-create.png` });
    const name = page.getByRole('textbox', { name: /thread name/i });
    if (await name.isVisible().catch(() => false)) {
      await name.fill(`thread ${stamp}`);
      await name.press('Enter');
    }
    await expect(page).toHaveURL(/threads/);
    await page.screenshot({ path: `${OUT}-flow-thread.png` });
    await page.goto(`/channels/${world.guildId}/${world.channelId}`);
    await composer.waitFor();
  });
  await step('attach a long file name', async () => {
    await page.getByTestId('file-input').setInputFiles({
      name: `a-very-long-file-name-for-the-polish-round-${stamp}-final-final-v3-really.pdf`,
      mimeType: 'application/pdf',
      buffer: Buffer.from('%PDF-1.4\n%polish\n'),
    });
    await page.screenshot({ path: `${OUT}-flow-file-queued.png` });
    await composer.fill(`file ${stamp}`);
    await composer.press('Enter');
    // Local stacks without object storage cannot finish an upload; that is the environment, not the UI.
    const noStorage = page.getByText(/The upload could not be completed|Upload failed/).first();
    const failedUpload = await noStorage
      .waitFor({ timeout: 8000 })
      .then(() => true)
      .catch(() => false);
    if (failedUpload) {
      log.push('skip attach: the upload failed (no object storage in this stack)');
      await page
        .getByRole('button', { name: /remove|delete/i })
        .last()
        .click()
        .catch(() => {});
      await composer.fill('');
      return;
    }
    await expect(msg(page, `file ${stamp}`).locator('.lng-attach-file')).toBeVisible({
      timeout: 30_000,
    });
    await page.setViewportSize({ width: 375, height: 812 });
    await page.waitForTimeout(800);
    const box = await msg(page, `file ${stamp}`).locator('.lng-attach-file').boundingBox();
    if (!box || box.x + box.width > 375)
      throw new Error(`file card runs off screen: ${JSON.stringify(box)}`);
    await page.screenshot({ path: `${OUT}-flow-file-phone.png` });
    await page.setViewportSize({ width: 1280, height: 800 });
  });
  await step('delete', async () => {
    for (const s of [`reply ${stamp}`, text]) {
      await menu(s, 'Delete Message');
      await page.getByRole('dialog').getByRole('button', { name: 'Delete' }).click();
      await expect(page.locator('.lng-msg-content', { hasText: s })).toHaveCount(0);
    }
  });

  writeFileSync(`${OUT}-flows.txt`, log.join('\n'));
  expect(failed, log.join('\n')).toEqual([]);
});

test('answer a message request, send and delete a DM, and edit the profile page', async ({
  page,
}) => {
  test.setTimeout(240_000);
  const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8'));
  const log: string[] = [];
  const failed: string[] = [];
  page.on('pageerror', (e) => log.push(`pageerror ${e.message}`));
  const step = async (name: string, fn: () => Promise<void>) => {
    try {
      await fn();
      log.push(`ok   ${name}`);
    } catch (e) {
      failed.push(name);
      log.push(`FAIL ${name}: ${String(e).split('\n')[0]}`);
      await page
        .screenshot({ path: `${OUT}-flow-fail-${name.replace(/\W+/g, '-')}.png` })
        .catch(() => {});
    }
  };
  const stamp = Date.now().toString(36);

  await step('open the DM by link (a pending request shows the request, O1)', async () => {
    await page.goto(`/channels/@me/${world.dmId}`);
    const accept = page.getByRole('button', { name: /^Accept/ });
    const composer = page.getByRole('textbox', { name: /^Message (?!#)/ });
    await expect(accept.or(composer).first()).toBeVisible({ timeout: 30_000 });
    if (await accept.isVisible()) {
      await page.screenshot({ path: `${OUT}-flow-dm-request.png` });
      await accept.click();
    }
    await expect(composer).toBeVisible();
  });
  await step('send and delete a DM', async () => {
    const composer = page.getByRole('textbox', { name: /^Message (?!#)/ });
    await composer.fill(`dm ${stamp}`);
    await composer.press('Enter');
    const row = page
      .locator('.lng-msg')
      .filter({ hasText: `dm ${stamp}` })
      .last();
    await expect(row).toBeVisible();
    await page.screenshot({ path: `${OUT}-flow-dm.png` });
    await row.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Delete Message' }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Delete' }).click();
    await expect(page.locator('.lng-msg-content', { hasText: `dm ${stamp}` })).toHaveCount(0);
  });
  await step('add a block to the profile page, save, view, remove it again', async () => {
    await page.goto('/profile-page/edit');
    await page.getByRole('button', { name: 'Add block' }).click();
    await page.getByRole('button', { name: 'Save page' }).click();
    await expect(page.getByText('Saved', { exact: true })).toBeVisible();
    await page.getByRole('link', { name: 'View page' }).click();
    await expect(page).toHaveURL(/\/u\//);
    await page.waitForTimeout(1000);
    await page.screenshot({ path: `${OUT}-flow-profile-page.png`, fullPage: true });
    await page.goto('/profile-page/edit');
    const tile = page.locator('.pp-tile').last();
    await tile.click();
    await page.keyboard.press('Delete');
    await page.getByRole('button', { name: 'Save page' }).click();
    await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  });

  writeFileSync(`${OUT}-flows-2.txt`, log.join('\n'));
  expect(failed, log.join('\n')).toEqual([]);
});
