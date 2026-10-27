import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';

/**
 * OPS-IDX-001 (D-384): only the landing page is indexable. Run by the `stack` job of prod-smoke.yml against the
 * installed Caddy (the dev server does not send X-Robots-Tag). Later waves append their hosts and routes to SAMPLE
 * (`/public/profiles/x` in wave 34, the activities host in wave 36).
 */
const SAMPLE = [
  '/login',
  '/register',
  '/invite/x',
  '/help',
  '/u/x',
  '/~x',
  '/cinema',
  '/library',
  '/app',
  '/api/v1/healthz',
  '/cdn/',
  '/gateway',
];

test('GET / carries no X-Robots-Tag and no noindex meta once rendered', async ({
  page,
  request,
}) => {
  const res = await request.get('/');
  expect(res.status()).toBe(200);
  expect(res.headers()['x-robots-tag']).toBeUndefined();
  await page.goto('/');
  await expect(page.locator('#invite-code')).toBeVisible();
  await expect(page.locator('meta[name="robots"]')).toHaveCount(0);
});

test('GET /robots.txt is byte-for-byte the shipped file', async ({ request }) => {
  const res = await request.get('/robots.txt');
  expect(res.status()).toBe(200);
  const shipped = readFileSync(
    resolve(dirname(fileURLToPath(import.meta.url)), '../../apps/web/public/robots.txt'),
  );
  expect(Buffer.from(await res.body()).equals(shipped)).toBe(true);
});

for (const path of SAMPLE) {
  test(`${path} carries X-Robots-Tag: noindex, nofollow`, async ({ request }) => {
    const res = await request.get(path, { maxRedirects: 0 });
    expect(res.headers()['x-robots-tag']).toBe('noindex, nofollow');
  });
}

test('a rendered route other than / carries the robots meta', async ({ page }) => {
  await page.goto('/login');
  await expect(page.locator('meta[name="robots"][content="noindex"]')).toHaveCount(1);
});
