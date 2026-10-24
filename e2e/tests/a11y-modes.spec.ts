import { expect, test } from '../support/fixtures';
import { runAxe, settle } from '../support/a11y';
import { soloServer } from '../support/world';

/**
 * RDY-A11Y-004 in a real browser: reduced motion (the OS preference and the in-app setting) collapses every
 * animation and transition in the Lounge, Cinema and Library, and the in-app High contrast setting lifts the
 * muted text tokens to full strength with no colour-contrast violation left, on every browser. (The token
 * arithmetic is walked by packages/ui/src/theme/tokens-contrast.test.ts; the CSS guard by
 * apps/web/src/a11y/reducedMotion.test.ts.)
 */
test.use({ bypassCSP: true });

/** The longest animation or transition duration, in seconds, of any element on the page. */
const longestMotion = () =>
  Array.from(document.querySelectorAll('*')).reduce((max, el) => {
    const cs = getComputedStyle(el);
    const secs = (v: string) =>
      v
        .split(',')
        .map((p) => p.trim())
        .map((p) => (p.endsWith('ms') ? parseFloat(p) / 1000 : parseFloat(p)))
        .reduce((a, b) => Math.max(a, Number.isFinite(b) ? b : 0), 0);
    const hasAnimation = cs.animationName && cs.animationName !== 'none';
    return Math.max(max, secs(cs.transitionDuration), hasAnimation ? secs(cs.animationDuration) : 0);
  }, 0);

test('reduced motion: nothing animates in the Lounge, Cinema and Library', async ({ owner, signedIn }) => {
  const { a, guildId, channelId } = await soloServer(signedIn, owner);
  const page = a.page;
  const routes = [`/channels/${guildId}/${channelId}`, '/cinema', '/library'];

  // Without a preference the app does animate (otherwise the checks below prove nothing).
  await page.goto(routes[0]!);
  await expect(page.getByRole('log')).toBeVisible();
  await settle(page);
  const normal = await page.evaluate(longestMotion);
  expect(normal, 'the Lounge has some transition when motion is allowed').toBeGreaterThan(0.05);

  // The OS preference.
  await page.emulateMedia({ reducedMotion: 'reduce' });
  for (const route of routes) {
    await page.goto(route);
    await expect(page.getByRole('main').first()).toBeVisible();
    await settle(page);
    expect(await page.evaluate(longestMotion), `${route} with prefers-reduced-motion`).toBeLessThanOrEqual(0.002);
  }

  // The in-app setting: every themed root carries data-reduced-motion, whatever the OS says.
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  for (const route of routes) {
    await page.goto(route);
    await expect(page.getByRole('main').first()).toBeVisible();
    await page.evaluate(() =>
      document.querySelectorAll('[data-reduced-motion]').forEach((el) => el.setAttribute('data-reduced-motion', 'true')),
    );
    await settle(page);
    expect(await page.evaluate(longestMotion), `${route} with the in-app setting`).toBeLessThanOrEqual(0.002);
  }
});

test('high contrast: muted text becomes full-strength text and no contrast violation is left', async ({
  owner,
  signedIn,
}) => {
  const { a, guildId, channelId } = await soloServer(signedIn, owner);
  const page = a.page;
  const toggle = async () => {
    await page.goto('/settings/user/accessibility');
    const sw = page.getByRole('switch', { name: /High contrast/ });
    await expect(sw).toBeVisible();
    await sw.click();
    await page.getByRole('button', { name: /^Save/ }).click();
    await expect(page.getByRole('button', { name: /^Save/ })).toBeHidden();
  };
  await toggle();
  try {
    for (const route of [`/channels/${guildId}/${channelId}`, '/cinema', '/library/browse']) {
      await page.goto(route);
      await expect(page.getByRole('main').first()).toBeVisible();
      await settle(page);
      const colours = await page.evaluate(() => {
        const root = document.querySelector('[data-contrast="high"]') as HTMLElement | null;
        if (!root) return null;
        const cs = getComputedStyle(root);
        const pick = (n: string) => cs.getPropertyValue(n).trim();
        return {
          text: pick('--ui-text'),
          muted: pick('--ui-text-muted'),
          faint: pick('--ui-text-faint'),
          sr2: pick('--sr-text-2'),
          sr3: pick('--sr-text-3'),
          legacy: pick('--text-muted'),
        };
      });
      expect(colours, `${route} has a high-contrast root`).not.toBeNull();
      expect(colours!.muted).toBe(colours!.text);
      expect(colours!.faint).toBe(colours!.text);
      // The legacy and Side Room tokens follow (they resolve to var(...) of the text colour).
      for (const key of ['sr2', 'sr3', 'legacy'] as const) expect(colours![key]).not.toBe('');
      const contrast = (await runAxe(page)).filter((v) => v.id === 'color-contrast');
      expect(contrast.map((v) => `${v.id}: ${v.targets.slice(0, 3).join(' | ')}`), route).toEqual([]);
    }
  } finally {
    await toggle();
  }
});
