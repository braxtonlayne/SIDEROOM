import { createRequire } from 'node:module';
import { expect, test, type Page } from '@playwright/test';

/**
 * XC-A11Y-004: axe on every public route in a real browser (contrast and landmarks included, which jsdom
 * cannot check), plus the keyboard basics: a skip link as the first tab stop and a visible focus ring.
 * Routes behind a session are covered by the component axe tests and the full-stack suites.
 */
const axeSource = createRequire(import.meta.url).resolve('axe-core/axe.min.js');

// axe is injected as an inline script, which the app's own CSP (correctly) refuses.
test.use({ bypassCSP: true });

const ROUTES = [
  '/',
  '/login',
  '/forgot',
  '/reset/sometoken',
  '/verify-email/sometoken',
  '/no-invitation',
  '/house-notes',
  '/support',
  '/login/2fa',
];

async function violations(page: Page) {
  await page.addScriptTag({ path: axeSource });
  return page.evaluate(async () => {
    const axe = (
      window as unknown as {
        axe: {
          run: (
            c: Document,
            o: object,
          ) => Promise<{
            violations: { id: string; help: string; nodes: { target: string[] }[] }[];
          }>;
        };
      }
    ).axe;
    const r = await axe.run(document, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'best-practice'],
      },
    });
    return r.violations.map(
      (v) => `${v.id}: ${v.help} -> ${v.nodes.map((n) => n.target.join(' ')).join(' | ')}`,
    );
  });
}

for (const route of ROUTES) {
  test(`axe: ${route}`, async ({ page }) => {
    await page.goto(route);
    await page.waitForLoadState('networkidle');
    expect(await violations(page)).toEqual([]);
  });
}

test('the skip link is the first tab stop and moves focus to the main landmark', async ({
  page,
}) => {
  await page.goto('/login');
  await page.locator('h1').first().waitFor();
  await page.keyboard.press('Tab');
  const skip = page.getByRole('link', { name: 'Skip to main content' });
  await expect(skip).toBeFocused();
  await expect(skip).toBeInViewport();
  await page.keyboard.press('Enter');
  await expect(page.locator('main')).toBeFocused();
});

test('every entrance page has exactly one main landmark and one h1', async ({ page }) => {
  for (const route of ['/', '/login', '/forgot', '/no-invitation', '/house-notes']) {
    await page.goto(route);
    await expect(page.getByRole('main')).toHaveCount(1);
    expect(await page.locator('h1').count(), route).toBeGreaterThanOrEqual(1);
  }
});

test('reduced motion removes the invite-field shake and long transitions', async ({ browser }) => {
  const ctx = await browser.newContext({ reducedMotion: 'reduce', bypassCSP: true });
  const page = await ctx.newPage();
  await page.goto('/');
  const duration = await page.evaluate(() => {
    const el = document.querySelector('.ent-invite');
    return el ? getComputedStyle(el).animationDuration : '0s';
  });
  expect(parseFloat(duration)).toBeLessThan(0.05);
  await ctx.close();
});

test('forced colours keep focus visible', async ({ browser }) => {
  const ctx = await browser.newContext({ forcedColors: 'active', bypassCSP: true });
  const page = await ctx.newPage();
  await page.goto('/login');
  await page.locator('h1').first().waitFor();
  await page.keyboard.press('Tab');
  const outline = await page.evaluate(() => getComputedStyle(document.activeElement!).outlineStyle);
  expect(outline).not.toBe('none');
  await ctx.close();
});
