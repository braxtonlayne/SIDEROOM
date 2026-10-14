import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';

/**
 * Cinema publisher tools against a real server: the Owner creates a draft title in the catalog editor,
 * cannot publish it while nothing is playable (409 shown), links a video asset id, publishes it, and
 * unpublishes it again. No transcoding is involved; the asset link accepts an id from the ingest work.
 */

const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

test('Owner creates a draft title, links a video and publishes it', async ({ page }) => {
  const name = `E2E Film ${Date.now().toString(36)}`;
  const { owner } = state();
  await page.goto('/login');
  await page.getByLabel('Username or email').fill(owner.username);
  await page.getByLabel('Password', { exact: true }).fill(owner.password);
  await page.getByRole('button', { name: 'Step inside' }).click();
  await expect(page).toHaveURL(/\/channels/);

  await page.goto('/admin/cinema-catalog');
  await expect(page.getByRole('heading', { name: 'Cinema catalog' })).toBeVisible();
  await page.getByRole('button', { name: 'New title' }).click();
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill(name);
  await page.getByRole('spinbutton', { name: 'Year' }).fill('2001');
  await page.getByRole('button', { name: 'Create draft' }).click();
  await expect(page.getByRole('tab', { name: 'Artwork' })).toBeVisible();

  // Nothing playable yet: the API refuses and the editor says why.
  await page.getByRole('button', { name: 'Publish', exact: true }).click();
  await expect(page.locator('.ss-status[role="alert"]')).toContainText(/playable/i);

  await page.getByRole('tab', { name: 'Video and transcoding' }).click();
  await page.getByRole('textbox', { name: /Asset id for/ }).fill('4242');
  await page.getByRole('button', { name: 'Link video' }).click();
  await expect(page.getByText('Linked.')).toBeVisible();

  await page.getByRole('button', { name: 'Publish', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Unpublish', exact: true })).toBeVisible();

  // The catalog list shows it published, then a draft again after unpublishing.
  await page.getByRole('button', { name: 'Back to titles' }).click();
  await page.getByRole('searchbox', { name: 'Search titles' }).fill(name);
  await expect(page.getByRole('button', { name: `Unpublish ${name}` })).toBeVisible();
  await page.getByRole('button', { name: `Unpublish ${name}` }).click();
  await expect(page.getByRole('button', { name: `Publish ${name}` })).toBeVisible();
});
