import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
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
 * SEC-AUD-274, 275 (wave 72; D-757): the message list and the pins panel style what the markdown renderer draws:
 * mention chips, code, quotes, headings, lists, emoji, spoilers. Real stack, two accounts. Editorial, Lounge Light and
 * Lounge Dark; desktop 1280x800 and phone 375x812. Local only (SR_P72_PROSE=1); shots go to SR_P72_PROSE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P72_PROSE_OUT ?? resolve('test-results/p72-prose');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const DESKTOP = { width: 1280, height: 800 };
const PHONE = { width: 375, height: 812 };
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(process.env.SR_P72_PROSE !== '1', 'local visual check only');
test.setTimeout(1_800_000);

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
    { theme, zoom: 100 },
  );
  return c;
}

const LONGWORD = 'Supercalifragilisticexpialidocious'.repeat(5);

test('every construct the renderer draws, in the list, the pins panel and search', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const me = (await (await o.get('/api/v1/users/@me')).json()) as { id: string };
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Prose Booth' },
  );
  const g = guild.id;
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const role = await apiPost<{ id: string }>(o, `/guilds/${g}/roles`, { name: 'Gardeners' });
  const invite = await apiPost<{ code: string }>(o, `/channels/${channel}/invites`, {});
  const bctx = await browser.newContext({ baseURL: ORIGIN });
  const bob = await registerUser(bctx.request, 'bobby');
  await loginApi(bctx.request, bob);
  await apiPost(bctx.request, `/invites/${invite.code}`, {});

  const post = async (content: string) =>
    (await apiPost<{ id: string }>(bctx.request, `/channels/${channel}/messages`, { content })).id;
  const ids: string[] = [];
  const messages = [
    `mentions: hello <@${me.id}> and <@&${role.id}> and @everyone and <#${channel}> in one line`,
    'inline `code here` inside a sentence, then more words after it',
    '```js\nconst answer = 42;\nfunction hello(name) { return name; }\n// a long line that must scroll sideways, not widen the page, because it is far too long for a phone column\n```',
    '```\nplain block without a language\n  indented line\n```',
    '> a single quoted line',
    '> first quoted line\n> second quoted line\n> third quoted line',
    '# Heading one\n## Heading two\n### Heading three\nand then plain text',
    '-# a subtext line, small',
    '- tea\n- biscuits\n- a **map**\n\n1. first\n2. second\n3. third',
    'a link https://example.com/some/path and a timestamp <t:1893456000:f> and <t:1893456000:R>',
    'custom emoji in a sentence <:blob:123456789012345678> then text, and <:blob:123456789012345678><:blob:123456789012345678> two in a row, and unicode \u{1F600}',
    '\u{1F600}\u{1F389}\u{1F680}',
    '<:blob:123456789012345678> <:blob:123456789012345678>',
    'a ||closed spoiler words|| and then more text',
    LONGWORD,
    'line one\nline two\nline three after breaks\n\n\nafter two blank lines',
  ];
  for (const m of messages) {
    ids.push(await post(m));
    await new Promise((r) => setTimeout(r, 1200));
  }
  const pinned = [ids[0], ids[2], ids[5], ids[messages.findIndex((m) => m.includes('||'))], ids[8]];
  for (const id of pinned) await apiPut(o, `/channels/${channel}/pins/${id}`, {});
  await new Promise((r) => setTimeout(r, 2500));

  // A custom emoji's signed URL cannot be served by the memory store: answer it with a small inline image.
  const EMOJI_SVG = `data:image/svg+xml;utf8,${encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><circle cx="48" cy="48" r="44" fill="#e8a33d"/><circle cx="32" cy="38" r="7"/><circle cx="64" cy="38" r="7"/><path d="M26 58q22 26 44 0" stroke="#000" stroke-width="6" fill="none"/></svg>',
  )}`;
  const fakeEmoji = (c: BrowserContext) =>
    c.route('**/sr/emojis/*/url*', (r) =>
      r.fulfill({ contentType: 'application/json', body: JSON.stringify({ url: EMOJI_SVG }) }),
    );

  const open = async (t: string, size: Size) => {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: t, zoom: 100, messageDisplay: 'cozy' } },
    });
    const c = await context(browser, octx, size, t);
    await fakeEmoji(c);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    await page.goto(`/channels/${g}/${channel}`);
    await expect(page.getByRole('main')).toBeVisible();
    await page.waitForTimeout(1500);
    return { c, page };
  };
  const shot = async (target: Locator | Page, name: string) => {
    await target.screenshot({ path: resolve(OUT, `${name}.png`) });
  };
  const overflowOf = (page: Page) =>
    page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );

  for (const t of THEMES) {
    for (const [vp, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const tag = `${vp}-${t}`;
      const { c, page } = await open(t, size);
      await expect(page.locator('.lng-msg-content').first()).toBeVisible();

      // The list scroller (the ancestor of the messages that scrolls), marked for the steps below.
      const marked = await page.evaluate(() => {
        const el = document.querySelector('.lng-msg-content');
        let p = el?.parentElement ?? null;
        while (p && p.scrollHeight <= p.clientHeight + 4) p = p.parentElement;
        if (!p) return false;
        p.setAttribute('data-p72-scroller', '1');
        return true;
      });
      expect(marked).toBe(true);
      const sc = page.locator('[data-p72-scroller]');
      // Scroll from the oldest to the newest, shooting each step, and read the computed styles of the first mention,
      // inline code and quote on the way (the list may draw only what is near the viewport).
      const seen: { bg?: string; font?: string; bar?: string; jumbo?: string } = {};
      await sc.evaluate((el) => (el.scrollTop = 0));
      await page.waitForTimeout(400);
      const h = await sc.evaluate((el) => el.scrollHeight - el.clientHeight);
      const step = size.height - 220;
      let n = 1;
      for (let y = 0; y < h + step && n < 12; y += step) {
        await sc.evaluate((el, yy) => (el.scrollTop = yy), y);
        await page.waitForTimeout(300);
        const got = await page.evaluate(() => {
          const cs = (
            sel: string,
            prop: 'backgroundColor' | 'fontFamily' | 'borderInlineStartWidth' | 'fontSize',
          ) => {
            const el = document.querySelector(sel);
            return el ? getComputedStyle(el)[prop] : undefined;
          };
          return {
            bg: cs('.lng-msg-content span.sr-md-mention', 'backgroundColor'),
            font: cs('.lng-msg-content .sr-md-code', 'fontFamily'),
            bar: cs('.lng-msg-content .sr-md-quote', 'borderInlineStartWidth'),
            jumbo: cs('.lng-msg-content .sr-md-jumbo', 'fontSize'),
          };
        });
        seen.bg ??= got.bg;
        seen.font ??= got.font;
        seen.bar ??= got.bar;
        seen.jumbo ??= got.jumbo;
        await shot(page, `list-${n++}-${tag}`);
      }
      console.log(`RECORDED styles ${tag}:`, JSON.stringify(seen));
      // These fail on the base: no rule styles a mention, inline code or a quote in the list.
      if (process.env.SR_P72_ASSERT !== '0') {
        expect(seen.bg).toBeDefined();
        expect(seen.bg).not.toBe('rgba(0, 0, 0, 0)');
        expect(seen.font).not.toBe('monospace');
        expect((seen.font ?? '').toLowerCase()).toMatch(/mono|consolas|menlo/);
        expect(seen.bar).toBe('4px');
        expect(parseFloat(seen.jumbo ?? '0')).toBeGreaterThanOrEqual(40);
      }
      console.log(`RECORDED page overflow ${tag}:`, await overflowOf(page));

      // Bring an element into view even when the list draws only what is near the viewport.
      const reveal = async (l: Locator) => {
        for (let y = 0; y < h + step && (await l.count()) === 0; y += step) {
          await sc.evaluate((el, yy) => (el.scrollTop = yy), y);
          await page.waitForTimeout(300);
        }
        await l.first().scrollIntoViewIfNeeded();
      };

      // A code block scrolls inside itself; the page does not widen.
      const block = page.locator('.lng-msg-content pre').first();
      await reveal(block);
      console.log(
        `RECORDED codeblock ${tag}:`,
        JSON.stringify(await block.evaluate((el) => ({ sw: el.scrollWidth, cw: el.clientWidth }))),
      );
      expect(await overflowOf(page)).toBeLessThanOrEqual(0);

      // A spoiler in the list opens on a click.
      const spoiler = page.locator('.lng-msg-content .sr-md-spoiler').first();
      await reveal(spoiler);
      await expect(spoiler).toHaveAttribute('data-revealed', 'false');
      await spoiler.click({ timeout: 4000 }).catch(() => spoiler.dispatchEvent('click'));
      await expect(spoiler).toHaveAttribute('data-revealed', 'true');
      await page.waitForTimeout(200);
      await shot(page, `spoiler-open-${tag}`);

      // A mention chip opens the profile card.
      const chip = page.locator('.lng-msg-content button.sr-md-mention').first();
      await reveal(chip);
      if ((await chip.count()) > 0) {
        await chip.click({ timeout: 4000 }).catch(() => chip.dispatchEvent('click'));
        await page.waitForTimeout(600);
        await shot(page, `mention-click-${tag}`);
        await page.keyboard.press('Escape');
        await page.waitForTimeout(200);
      } else {
        console.log(`RECORDED no button mention ${tag}`);
      }

      // The pins panel.
      await page.getByRole('button', { name: 'Pinned Messages' }).first().click();
      const pins = page.locator('.lng-pins-pop');
      await expect(pins).toBeVisible();
      await page.waitForTimeout(900);
      await shot(page, `pins-before-click-${tag}`);
      const pinSpoiler = pins.locator('.sr-md-spoiler').first();
      await expect(pinSpoiler).toHaveAttribute('data-revealed', 'false');
      await pinSpoiler.click();
      await expect(pinSpoiler).toHaveAttribute('data-revealed', 'true');
      await page.waitForTimeout(200);
      await shot(page, `pins-after-click-${tag}`);
      await pins.evaluate((el) => (el.scrollTop = el.scrollHeight));
      await page.waitForTimeout(200);
      await shot(page, `pins-scrolled-${tag}`);
      await c.close();
    }

    // A search hit that shows a mention (desktop).
    {
      const tag = `desktop-${t}`;
      const { c, page } = await open(t, DESKTOP);
      const box = page.getByRole('searchbox').first();
      await box.fill('mentions');
      await box.press('Enter');
      const panel = page.getByRole('region', { name: 'Search results' });
      await expect(panel).toBeVisible();
      await page.waitForTimeout(1800);
      await shot(page, `search-${tag}`);
      await c.close();
    }
  }
  await bctx.close();
  await octx.close();
});
