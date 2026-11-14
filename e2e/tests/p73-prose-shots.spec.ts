import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  apiPut,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';

/**
 * SEC-AUD-283..285 (wave 73; D-763, D-764, D-766): headings sized as Discord's, the thread starter line as a plain
 * line, and the markdown surfaces wave 72 did not look at (rich-card mentions, the Settings bio preview, the
 * hover-reveal spoiler). Real stack, two accounts. Editorial, Lounge Light, Lounge Dark; desktop and phone.
 * Local only (SR_P73_PROSE=1); shots go to SR_P73_PROSE_OUT. SR_P73_ASSERT=0 records the base without asserting.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P73_PROSE_OUT ?? resolve('test-results/p73-prose');
const ASSERT = process.env.SR_P73_ASSERT !== '0';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P73_PROSE !== '1', 'local visual check only');
test.setTimeout(2_400_000);

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

const LONGWORD = 'Supercalifragilisticexpialidocious'.repeat(4);

test('headings, the thread starter line and the surfaces not looked at', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const me = (await (await o.get('/api/v1/users/@me')).json()) as { id: string };
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Heading Booth' },
  );
  const g = guild.id;
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const bctx = await browser.newContext({ baseURL: ORIGIN });
  const bob = await registerUser(bctx.request, 'bobby');
  await loginApi(bctx.request, bob);
  await apiPost(bctx.request, `/invites/${invite.code}`, {});
  try {
    await apiPatch(o, '/users/@me', {
      bio: `About me: **bold** and \`inline code\` and ||a bio spoiler|| and <@${me.id}>\n> a quoted line\n# a bio heading\nlast line`,
    });
  } catch (e) {
    console.log('RECORDED bio patch failed', String(e));
  }

  const postAs = async (req: APIRequestContext, content: string) =>
    (await apiPost<{ id: string }>(req, `/channels/${channel}/messages`, { content })).id;
  const post = (content: string) => postAs(bctx.request, content);
  const wait = () => new Promise((r) => setTimeout(r, 1200));
  const starter = await post(`see ||the ending|| and \`code\` <@${me.id}> end`);
  await wait();
  const opening = await post('# Opening heading\nbody text right after it');
  await wait();
  const mixed = await post(
    'before the headings\n# Heading one\n## Heading two\n### Heading three\na sentence between\n-# a subtext line, small\n- tea\n- biscuits\n- a **map**\n\n1. first\n2. second\nand after the list',
  );
  await wait();
  const long = await post(
    `## A long second level heading that has to wrap onto more than one line on a phone ${LONGWORD}`,
  );
  await wait();
  await post(`mention of you <@${me.id}> then @everyone and @here`);
  await wait();
  await post(
    'rich: </ping:111111111111111111> <111111111111111111:customize> <111111111111111111:browse> <111111111111111111:guide> <111111111111111111:linked-roles>',
  );
  await wait();
  await post('cards <cinema:t1> and <library:s1> and <cinema:missing1> <library:missing2>');
  await wait();
  await postAs(o, 'my own ||hover secret words|| in a sentence');
  for (const id of [opening, mixed, starter, long])
    await apiPut(o, `/channels/${channel}/pins/${id}`, {});
  const th = await apiPost<{ id: string }>(
    bctx.request,
    `/channels/${channel}/messages/${starter}/threads`,
    { name: 'Spoiler thread' },
  );
  await new Promise((r) => setTimeout(r, 1500));
  await apiPost(bctx.request, `/channels/${th.id}/messages`, { content: 'a reply in the thread' });
  const mixedThread = await apiPost<{ id: string }>(
    bctx.request,
    `/channels/${channel}/messages/${mixed}/threads`,
    { name: 'Heading thread' },
  );
  await new Promise((r) => setTimeout(r, 1200));
  await apiPost(bctx.request, `/channels/${mixedThread.id}/messages`, {
    content:
      '# Heading one\n## Heading two\n### Heading three\na sentence between\n-# a subtext line\n- tea\n- biscuits\n\n1. first\n2. second',
  });
  await new Promise((r) => setTimeout(r, 2500));

  const open = async (t: string, size: Size, path?: string) => {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: t, zoom: 100, messageDisplay: 'cozy', spoilers: 'mine' } },
    });
    const c = await context(browser, octx, size, t);
    await c.route('**/api/v1/cinema/titles/t1', (r) =>
      r.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          title: {
            id: 't1',
            kind: 'movie',
            title: 'The Long Spoiler',
            year: 2021,
            runtime_min: 104,
            synopsis:
              'A film about a very long synopsis that has to wrap onto a second line in the card.',
            genres: [],
            poster_url: null,
          },
          seasons: [],
        }),
      }),
    );
    await c.route('**/api/v1/library/series/s1', (r) =>
      r.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          series: {
            id: 's1',
            kind: 'manga',
            title: 'Garden Chronicles',
            alt_titles: [],
            authors: [],
            artists: [],
            status: 'ongoing',
            synopsis: 'A gardener, a map and a very long synopsis for the card.',
            genres: [],
            tags: [],
            reading_direction: 'rtl',
            age_restricted: false,
            published: true,
            chapter_count: 42,
            follower_count: 3,
            cover_url: null,
            year: 2020,
          },
        }),
      }),
    );
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    await page.goto(path ?? `/channels/${g}/${channel}`);
    await page.waitForTimeout(1800);
    return { c, page };
  };
  const shot = async (target: Locator | Page, name: string) => {
    await target.screenshot({ path: resolve(OUT, `${name}.png`) });
  };
  const overflowOf = (page: Page) =>
    page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
  const sizesOf = (page: Page, root: string) =>
    page.evaluate((r) => {
      type P = 'fontSize' | 'marginBlockStart' | 'marginBlockEnd' | 'fontWeight' | 'lineHeight';
      const px = (el: Element | null, p: P) => (el ? getComputedStyle(el)[p] : null);
      const q = (s: string) => {
        const want = s.startsWith('h1')
          ? 'Heading one'
          : s.startsWith('h2')
            ? 'Heading two'
            : s.startsWith('h3')
              ? 'Heading three'
              : null;
        const all = [...document.querySelectorAll(`${r} ${s}`)].filter(
          (e) => e.getClientRects().length > 0,
        );
        return (want && all.find((e) => e.textContent === want)) || all[0] || null;
      };
      return {
        text: px(q('.sr-md'), 'fontSize'),
        h1: px(q('h1.sr-md-heading'), 'fontSize'),
        h2: px(q('h2.sr-md-heading'), 'fontSize'),
        h3: px(q('h3.sr-md-heading'), 'fontSize'),
        h1w: px(q('h1.sr-md-heading'), 'fontWeight'),
        h1lh: px(q('h1.sr-md-heading'), 'lineHeight'),
        h1mt: px(q('h1.sr-md-heading'), 'marginBlockStart'),
        h1mb: px(q('h1.sr-md-heading'), 'marginBlockEnd'),
        sub: px(q('small.sr-md-subtext'), 'fontSize'),
      };
    }, root);

  for (const t of THEMES) {
    for (const [vp, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const tag = `${vp}-${t}`;
      const { c, page } = await open(t, size);
      await expect(page.locator('.lng-msg-content').first()).toBeVisible();
      // The list draws only what is near the viewport: walk up from the newest until the element exists.
      const scrollTo = async (loc: Locator) => {
        for (let i = 0; i < 60 && (await loc.count()) === 0; i++) {
          await page.evaluate(() => {
            const el = document.querySelector('.lng-msg-content');
            let p = el?.parentElement ?? null;
            while (p && p.scrollHeight <= p.clientHeight + 4) p = p.parentElement;
            if (p) p.scrollTop = Math.max(0, p.scrollTop - 250);
          });
          await page.waitForTimeout(250);
        }
        await loc.first().scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
      };
      await scrollTo(page.locator('.lng-msg-content h1.sr-md-heading').first());
      console.log(
        `RECORDED sizes list ${tag}:`,
        JSON.stringify(await sizesOf(page, '.lng-msg-content')),
      );
      await shot(page, `list-headings-${tag}`);
      if (vp === 'desktop' && t === 'lounge-editorial')
        console.log(
          'RECORDED html:',
          await page
            .locator('.lng-msg-content', { hasText: 'before the headings' })
            .first()
            .evaluate((el) => el.outerHTML.slice(0, 900)),
        );
      const mixedMsg = page.locator('.lng-msg-content', { hasText: 'before the headings' }).first();
      await scrollTo(mixedMsg);
      await shot(page, `list-mixed-${tag}`);
      const longH = page.locator('.lng-msg-content h2.sr-md-heading', { hasText: 'long second' });
      await scrollTo(longH);
      await shot(page, `list-longheading-${tag}`);
      console.log(`RECORDED overflow list ${tag}:`, await overflowOf(page));
      expect(await overflowOf(page)).toBeLessThanOrEqual(0);

      // Rich-card mentions and the viewer's mention.
      await scrollTo(page.locator('.lng-msg-content', { hasText: 'rich:' }));
      await shot(page, `list-rich-${tag}`);
      await scrollTo(page.locator('.lng-msg-content', { hasText: 'cards' }));
      await shot(page, `list-cards-${tag}`);
      const mention = page.locator('.lng-msg-content', { hasText: 'mention of you' });
      await scrollTo(mention);
      const row = mention.first().locator('xpath=ancestor::article[1]');
      console.log(
        `RECORDED mention row ${tag}:`,
        JSON.stringify(
          await row.evaluate((el) => ({
            cls: el.className,
            attrs: el.getAttributeNames().join(','),
            bg: getComputedStyle(el).backgroundColor,
            bar: getComputedStyle(el).boxShadow,
          })),
        ),
      );
      await shot(page, `list-viewer-mention-${tag}`);
      const chip = page.locator('.lng-msg-content button.sr-md-mention').first();
      await scrollTo(chip);
      await chip.click({ timeout: 4000 }).catch(() => chip.dispatchEvent('click'));
      await page.waitForTimeout(700);
      await shot(page, `profile-card-${tag}`);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(200);

      // The hover-reveal spoiler (desktop only: a phone has no hover).
      if (vp === 'desktop') {
        const own = page.locator('.lng-msg-content', { hasText: 'my own' }).last();
        await scrollTo(own);
        const sp = own.locator('.sr-md-spoiler').first();
        await expect(sp).toHaveAttribute('data-revealed', 'false');
        await shot(page, `spoiler-hover-before-${tag}`);
        // Contrast of the shown text on the hovered spoiler's backdrop (composited over the nearest opaque ancestor).
        const ratio = async (loc: Locator) =>
          loc.evaluate((el) => {
            const rgba = (css: string) => {
              const c = document.createElement('canvas');
              c.width = c.height = 1;
              const x = c.getContext('2d')!;
              x.clearRect(0, 0, 1, 1);
              x.fillStyle = css;
              x.fillRect(0, 0, 1, 1);
              const d = x.getImageData(0, 0, 1, 1).data;
              return [d[0]!, d[1]!, d[2]!, d[3]! / 255] as const;
            };
            let under: readonly number[] = [255, 255, 255, 1];
            for (let p = el.parentElement; p; p = p.parentElement) {
              const b = rgba(getComputedStyle(p).backgroundColor);
              if (b[3] > 0.99) {
                under = b;
                break;
              }
            }
            const bg = rgba(getComputedStyle(el).backgroundColor);
            const mix = [0, 1, 2].map((i) => bg[i]! * bg[3] + under[i]! * (1 - bg[3]));
            const fg = rgba(getComputedStyle(el).color);
            const lum = (c: readonly number[]) => {
              const f = [0, 1, 2].map((i) => {
                const v = c[i]! / 255;
                return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
              });
              return 0.2126 * f[0]! + 0.7152 * f[1]! + 0.0722 * f[2]!;
            };
            const a = lum(mix) + 0.05;
            const b = lum(fg) + 0.05;
            return Math.round((Math.max(a, b) / Math.min(a, b)) * 100) / 100;
          });
        const closedRatio = await ratio(sp);
        await sp.hover();
        await page.waitForTimeout(300);
        const hoverRatio = await ratio(sp);
        console.log(`RECORDED hover contrast ${tag}: closed ${closedRatio} hovered ${hoverRatio}`);
        if (ASSERT) expect(hoverRatio).toBeGreaterThanOrEqual(4.5);
        await page.mouse.move(5, 5);
        await page.waitForTimeout(200);
        await sp.hover();
        await page.waitForTimeout(300);
        await shot(page, `spoiler-hover-open-${tag}`);
        const op = await sp
          .locator('.sr-md-spoiler-content')
          .evaluate((el) => getComputedStyle(el).opacity);
        console.log(`RECORDED hover spoiler opacity ${tag}:`, op);
        if (ASSERT) expect(op).toBe('1');
        await page.mouse.move(5, 5);
        await sp.click({ timeout: 4000 }).catch(() => sp.dispatchEvent('click'));
        await page.waitForTimeout(200);
        console.log(`RECORDED click reveal ${tag}:`, await sp.getAttribute('data-revealed'));
        await shot(page, `spoiler-click-${tag}`);
      }

      // The pins panel.
      await page.getByRole('button', { name: 'Pinned Messages' }).first().click();
      const pins = page.locator('.lng-pins-pop');
      await expect(pins).toBeVisible();
      await page.waitForTimeout(900);
      console.log(
        `RECORDED sizes pins ${tag}:`,
        JSON.stringify(await sizesOf(page, '.lng-pins-pop')),
      );
      await shot(page, `pins-${tag}`);
      const caps = await pins.locator('.lng-pin-body').evaluateAll((els) =>
        els.map((el) => ({
          h: (el as HTMLElement).clientHeight,
          max: parseFloat(getComputedStyle(el).maxHeight),
        })),
      );
      console.log(`RECORDED pin caps ${tag}:`, JSON.stringify(caps));
      if (ASSERT) for (const k of caps) expect(k.h).toBeLessThanOrEqual(k.max + 1);
      await pins.evaluate((el) => (el.scrollTop = el.scrollHeight));
      await page.waitForTimeout(200);
      await shot(page, `pins-scrolled-${tag}`);
      expect(await overflowOf(page)).toBeLessThanOrEqual(0);
      await c.close();
    }

    // The thread panel (the "Thread started from" line, then the heading thread).
    for (const [vp, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const tag = `${vp}-${t}`;
      const { c, page } = await open(t, size, `/channels/${g}/${th.id}`);
      const q = page.locator('q.lng-sys-quote');
      await expect(q.first()).toBeVisible();
      const text = await q.first().innerText();
      console.log(`RECORDED thread line ${tag}:`, JSON.stringify(text));
      if (ASSERT) {
        expect(text).not.toContain('the ending');
        expect(text).not.toContain('`');
        expect(text).not.toContain('<@');
      }
      await page.waitForTimeout(500);
      await shot(page, `thread-starter-${tag}`);
      expect(await overflowOf(page)).toBeLessThanOrEqual(0);
      await c.close();
      const h = await open(t, size, `/channels/${g}/${mixedThread.id}`);
      await expect(h.page.locator('h1.sr-md-heading:visible').first()).toBeVisible();
      await h.page.waitForTimeout(700);
      console.log(`RECORDED sizes thread ${tag}:`, JSON.stringify(await sizesOf(h.page, 'body')));
      await shot(h.page, `thread-headings-${tag}`);
      await h.c.close();
    }

    // The Settings Profile bio preview.
    for (const [vp, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const tag = `${vp}-${t}`;
      // Entered from the Lounge, as a member does (the parser is loaded there; a direct load of the page is shot below).
      const { c, page } = await open(t, size);
      await page.waitForTimeout(1500);
      await page.evaluate(() => {
        history.pushState({}, '', '/settings/user/profile');
        window.dispatchEvent(new PopStateEvent('popstate'));
      });
      const bio = page.locator('[data-profile-bio]').first();
      await expect(bio).toBeVisible({ timeout: 20_000 });
      console.log(
        `RECORDED bio ${tag}:`,
        JSON.stringify(
          await bio.evaluate((el) => {
            const code = el.querySelector('.sr-md-code');
            const sp = el.querySelector('.sr-md-spoiler');
            return {
              text: (el as HTMLElement).innerText,
              codeBg: code ? getComputedStyle(code).backgroundColor : null,
              spBg: sp ? getComputedStyle(sp).backgroundColor : null,
            };
          }),
        ),
      );
      await bio.scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      await shot(page, `settings-profile-${tag}`);
      expect(await overflowOf(page)).toBeLessThanOrEqual(0);
      await c.close();
      if (vp === 'desktop') {
        const d = await open(t, size, '/settings/user/profile');
        await d.page.waitForTimeout(2500);
        await shot(d.page, `settings-profile-direct-${tag}`);
        await d.c.close();
      }
    }
  }
  await bctx.close();
  await octx.close();
});
