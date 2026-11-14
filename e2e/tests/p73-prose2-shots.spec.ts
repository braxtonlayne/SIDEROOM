import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-291 (wave 73; D-775): About Me starts the parser itself, so the Settings Profile preview and the card
 * it previews are markdown when Settings is the first page loaded. Real stack, one account. Editorial, Lounge
 * Light, Lounge Dark; desktop and phone. Local only (SR_P73_PROSE2=1); shots go to SR_P73_PROSE2_OUT.
 * SR_P73_BASE=1 records the base without asserting.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P73_PROSE2_OUT ?? resolve('test-results/p73-prose2');
const BASE = process.env.SR_P73_BASE === '1';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P73_PROSE2 !== '1', 'local visual check only');
test.setTimeout(1_200_000);

type Size = { width: number; height: number };

async function context(browser: Browser, base: BrowserContext, size: Size, theme: string) {
  const phone = size.width < 500;
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100, spoilers: 'mine' },
  );
  return c;
}

test('About Me as markdown with Settings loaded first', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  const o = octx.request;
  const me = (await (await o.get('/api/v1/users/@me')).json()) as { id: string };
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Bio Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  await apiPatch(o, '/users/@me', {
    bio: `About me: **bold**, *italic*, \`inline code\`, [a link](https://example.com) and ||a bio spoiler|| <@${me.id}>\n# a bio heading\n> a quoted line\nlast line`,
  });
  await apiPost(o, `/channels/${channel}/messages`, { content: `chip <@${me.id}>` });
  await new Promise((r) => setTimeout(r, 1500));

  const shot = (page: Page, name: string) => page.screenshot({ path: resolve(OUT, `${name}.png`) });
  const overflowOf = (page: Page) =>
    page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
  const open = async (t: string, size: Size, path: string) => {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: t, zoom: 100, messageDisplay: 'cozy', spoilers: 'mine' } },
    });
    const c = await context(browser, octx, size, t);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    const wasm: string[] = [];
    page.on('request', (r) => {
      if (/\.wasm(\?|$)/.test(r.url())) wasm.push(r.url().split('/').pop()!.slice(0, 60));
    });
    await page.goto(path);
    await page.waitForTimeout(3000);
    // The dev proxy sometimes refuses one request (localhost resolves to ::1); press Try again, as a member would.
    for (let i = 0; i < 3; i++) {
      const retry = page.getByRole('button', { name: 'Try again' });
      if (!(await retry.isVisible().catch(() => false))) break;
      console.log('RECORDED retry', path);
      await retry.click();
      await page.waitForTimeout(2000);
    }
    return { c, page, wasm };
  };
  const state = (page: Page) =>
    page
      .locator('[data-profile-bio]')
      .first()
      .evaluate((el) => {
        const code = el.querySelector('.sr-md-code');
        const sp = el.querySelector('.sr-md-spoiler');
        return {
          text: (el as HTMLElement).innerText,
          strong: el.querySelectorAll('strong').length,
          codeBg: code ? getComputedStyle(code).backgroundColor : null,
          spBg: sp ? getComputedStyle(sp).backgroundColor : null,
          spoiler: sp?.getAttribute('data-revealed') ?? null,
        };
      });

  for (const t of THEMES) {
    for (const [vp, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const tag = `${vp}-${t}`;
      // Settings is the first page loaded.
      const d = await open(t, size, '/settings/user/profile');
      const bio = d.page.locator('[data-profile-bio]').first();
      await expect(bio).toBeVisible({ timeout: 20_000 });
      const s = await state(d.page);
      console.log(`RECORDED direct ${tag}:`, JSON.stringify(s), 'wasm', JSON.stringify(d.wasm));
      await bio.scrollIntoViewIfNeeded();
      await d.page.waitForTimeout(300);
      await shot(d.page, `settings-profile-direct-${tag}`);
      if (!BASE) {
        expect(s.text).not.toContain('**');
        expect(s.text).not.toContain('||');
        expect(s.strong).toBeGreaterThan(0);
        expect(s.spoiler).toBe('false');
        expect(await overflowOf(d.page)).toBeLessThanOrEqual(0);
      }
      if (vp === 'desktop') {
        // Type in the field: the preview follows as markdown; click the spoiler.
        const area = d.page.getByLabel('About me').first();
        await area.fill('typed **now bold** and ||typed secret|| and `code`');
        await d.page.waitForTimeout(500);
        const typed = await state(d.page);
        console.log(`RECORDED typed ${tag}:`, JSON.stringify(typed));
        await shot(d.page, `settings-profile-typed-${tag}`);
        if (!BASE) {
          expect(typed.text).not.toContain('**');
          expect(typed.spoiler).toBe('false');
          await d.page.locator('[data-profile-bio] .sr-md-spoiler').first().click();
          await d.page.waitForTimeout(300);
          const revealed = await state(d.page);
          expect(revealed.spoiler).toBe('true');
          expect(revealed.text).toContain('typed secret');
          await shot(d.page, `settings-profile-spoiler-open-${tag}`);
        }
      }
      await d.c.close();

      if (vp === 'desktop' || t === 'lounge-light') {
        // Entered from the Lounge: must look the same as loaded directly.
        const l = await open(t, size, `/channels/${guild.id}/${channel}`);
        await l.page.evaluate(() => {
          history.pushState({}, '', '/settings/user/profile');
          window.dispatchEvent(new PopStateEvent('popstate'));
        });
        await expect(l.page.locator('[data-profile-bio]').first()).toBeVisible({
          timeout: 20_000,
        });
        await l.page.waitForTimeout(800);
        await shot(l.page, `settings-profile-fromlounge-${tag}`);
        await l.c.close();
      }
      if (vp === 'desktop') {
        // The popout in the Lounge (unchanged).
        const p = await open(t, size, `/channels/${guild.id}/${channel}`);
        const chip = p.page.locator('.lng-msg-content button.sr-md-mention').first();
        await chip.click({ timeout: 8000 }).catch(() => chip.dispatchEvent('click'));
        await expect(p.page.locator('[data-profile-bio]').first()).toBeVisible({
          timeout: 10_000,
        });
        await p.page.waitForTimeout(600);
        const ps = await state(p.page);
        console.log(`RECORDED popout ${tag}:`, JSON.stringify(ps));
        if (!BASE) expect(ps.text).not.toContain('**');
        await shot(p.page, `popout-${tag}`);
        await p.c.close();
      }
    }
  }
  // A Settings page with no bio, loaded first: does it fetch the WASM?
  for (const path of ['/settings/user/account', '/settings/user/appearance']) {
    const n = await open('lounge-light', DESKTOP, path);
    console.log(`RECORDED nobio ${path}: wasm`, JSON.stringify(n.wasm));
    await shot(n.page, `nobio-${path.split('/').pop()}-desktop-lounge-light`);
    await n.c.close();
  }
  await octx.close();
});
