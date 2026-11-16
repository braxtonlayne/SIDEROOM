import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';

/**
 * SEC-AUD-322 and SEC-AUD-323 (wave 80; D-823, D-824), on the real stack.
 *  1. A tool's single field is as tall as what it holds: for every tool of wave 79's list, the height of each
 *     `.ud-field` that is a direct child of `.ud-root` or `.ud-timer` and of the control in it, BASE (the new rule
 *     `.ud-root > .ud-field, .ud-timer > .ud-field` deleted from the live style sheet, so the page wears what the base
 *     CSS gave) and AFTER (as built). After: label height = caption + control + 4 px gap, within 2 px.
 *  2. The three last bare `--sr-*` colours: the verified privacy shield (a real call, Bob verified), the scope list of
 *     an authorised application (the token list faked), the elder vouch badge (the vouch summary faked: 500 vouches
 *     are not reachable). Computed colour and its contrast against the background behind it, base (the three old
 *     declarations put back on the live rules) and after, Lounge Light and Dark.
 *  3. Findings only: the Wheel (Light, Dark) and the kit Tabs (Saved items under dir=rtl on a phone; a strip in Admin
 *     and in a server's settings at 375).
 * Local only (SR_P80_TOOLS=1; the call needs a server with SR_VOICE__ENABLED=true, SR_P80_VOICE=1). Shots and
 * `measurements.txt` go to SR_P80_TOOLS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P80_TOOLS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-tools';
const VOICE = process.env.SR_P80_VOICE === '1';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P80_TOOLS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

const note = (text: string) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(`REPORT ${text}`);
};

type Size = { width: number; height: number };

async function context(browser: Browser, base: BrowserContext, size: Size, theme: string) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    permissions: ['microphone'],
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const setTheme = (a: BrowserContext['request'], theme: string) =>
  apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });

/** The theme the page is really wearing, read from the room's root. */
const themeOf = (page: Page) =>
  page.evaluate(
    () =>
      document.querySelector<HTMLElement>('.lng-root')?.dataset.theme ??
      document.documentElement.dataset.theme ??
      '?',
  );

function rgb(c: string): [number, number, number] {
  const m = c.match(/[\d.]+/g)!.map(Number);
  return [m[0] ?? 0, m[1] ?? 0, m[2] ?? 0];
}
function lum([r, g, b]: [number, number, number]) {
  const f = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}
function contrast(a: string, b: string) {
  const [x = 0, y = 0] = [lum(rgb(a)), lum(rgb(b))].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

// ---- Part 1: the tools' fields ---------------------------------------------------------------------------------

async function openTool(page: Page, query: string, ready: string) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.keyboard.press('Enter');
  await expect(page.locator(ready).first()).toBeVisible({ timeout: 15_000 });
}

interface FieldRow {
  caption: string;
  tag: string;
  label: number;
  span: number;
  control: number;
}

/** Every `.ud-field` that is a direct child of the tool's column, and the control in it. */
const fields = (page: Page): Promise<FieldRow[]> =>
  page.evaluate(() =>
    Array.from(
      document.querySelectorAll<HTMLElement>('.ud-root > .ud-field, .ud-timer > .ud-field'),
    ).map((f) => {
      const span = f.querySelector<HTMLElement>(':scope > span');
      const ctl = f.querySelector<HTMLElement>(
        ':scope > input, :scope > textarea, :scope > select',
      );
      return {
        caption: (span?.textContent ?? '').trim().slice(0, 24),
        tag: ctl?.tagName.toLowerCase() ?? '-',
        label: f.getBoundingClientRect().height,
        span: span?.getBoundingClientRect().height ?? 0,
        control: ctl?.getBoundingClientRect().height ?? 0,
      };
    }),
  );

/** Delete the new rule from the live style sheets: the page then wears the base CSS. Returns how many were deleted. */
const holdBackFieldRule = (page: Page) =>
  page.evaluate(() => {
    let n = 0;
    for (const sheet of Array.from(document.styleSheets)) {
      let rules: CSSRuleList;
      try {
        rules = sheet.cssRules;
      } catch {
        continue;
      }
      for (let i = rules.length - 1; i >= 0; i--) {
        const r = rules[i] as CSSStyleRule;
        if (r.selectorText && /\.ud-root\s*>\s*\.ud-field/.test(r.selectorText)) {
          sheet.deleteRule(i);
          n++;
        }
      }
    }
    return n;
  });

type Tool = {
  slug: string;
  query: string;
  ready?: string;
  act?: (page: Page) => Promise<void>;
  /** shot in all four variants + base (Regex, Diff), or Light desktop only */
  shots?: 'all' | 'light';
};

const fillFirst = (text: string) => async (page: Page) => {
  const ta = page
    .locator('.ud-root textarea, .ud-root input[type=text], .ud-root input:not([type])')
    .first();
  if (await ta.count()) await ta.fill(text);
};

const TOOLS: Tool[] = [
  { slug: 'json', query: 'json', act: fillFirst('{"a":1,"b":[1,2,3]}'), shots: 'light' },
  {
    slug: 'regex',
    query: 'regex tester',
    shots: 'all',
    act: async (p) => {
      await p.locator('.ud-root input').first().fill('a+b');
      await p.locator('.ud-root textarea').first().fill('aab ab b aaab');
      await expect(p.locator('.ud-history li').first()).toBeVisible();
      note(`regex click-through: ${await p.locator('.ud-history li').count()} matches listed`);
    },
  },
  { slug: 'base64', query: 'base64', act: fillFirst('hello world') },
  { slug: 'url', query: 'url encode', act: fillFirst('a b&c=d/é') },
  { slug: 'jwt', query: 'jwt decoder', act: fillFirst('eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig') },
  { slug: 'mdtable', query: 'markdown table', act: fillFirst('a,b\n1,2') },
  { slug: 'counter', query: 'word counter', act: fillFirst('one two three four') },
  {
    slug: 'hash',
    query: 'hashes',
    shots: 'light',
    act: async (page) => {
      await page
        .locator('input[type=file]')
        .first()
        .setInputFiles({ name: 'a.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
      await page.waitForTimeout(600);
    },
  },
  {
    slug: 'diff',
    query: 'diff',
    shots: 'all',
    act: async (p) => {
      const tas = p.locator('.ud-root textarea');
      await tas.nth(0).fill('one\ntwo\nthree');
      await tas.nth(1).fill('one\n2\nthree\nfour');
      await p.waitForTimeout(500);
      note(
        `diff click-through: result text length ${(await p.locator('.ud-root').innerText()).length}`,
      );
    },
  },
  { slug: 'dice', query: 'dice' },
  { slug: 'coin', query: 'coin flip' },
  { slug: 'wheel', query: 'wheel' },
  { slug: 'loot', query: 'loot tables' },
  { slug: 'notes', query: 'private notes', shots: 'light' },
  {
    slug: 'pomodoro',
    query: 'pomodoro',
    ready: '.ud-timer',
    shots: 'light',
    act: async (p) => {
      await p.locator('.ud-timer button.ud-btn-primary').first().click();
      await p.waitForTimeout(1200);
    },
  },
  { slug: 'stopwatch', query: 'stopwatch', ready: '.ud-timer' },
  { slug: 'countdown', query: 'countdown', ready: '.ud-timer' },
];

test('a tool field is as tall as what it holds', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  let shots = 0;
  const variants: Array<[string, string, Size]> = [
    ['lounge-light', 'desktop', DESKTOP],
    ['lounge-dark', 'desktop', DESKTOP],
    ['lounge-light', 'phone', PHONE],
    ['lounge-dark', 'phone', PHONE],
  ];
  for (const [theme, vp, size] of variants) {
    await setTheme(octx.request, theme);
    const c = await context(browser, octx, size, theme);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    for (const tool of TOOLS) {
      const main = theme === 'lounge-light' && vp === 'desktop';
      if (!main && tool.shots !== 'all') continue;
      const tag = `${tool.slug}-${theme}-${vp}`;
      try {
        await page.goto('/channels/@me');
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        expect(await themeOf(page), 'the room wears the chosen theme').toBe(theme);
        await openTool(page, tool.query, tool.ready ?? '.ud-root, .ud-timer');
        if (tool.act) await tool.act(page);
        await page.waitForTimeout(300);
        const after = await fields(page);
        const over = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(over, `${tag}: no sideways scroll`).toBeLessThanOrEqual(0);
        // BASE first when it is wanted as a shot (Regex, Diff in Light desktop), then restore by reloading is not
        // needed: the AFTER shot is taken before the rule is deleted.
        await page.screenshot({ path: resolve(OUT, `${tag}-after.png`) });
        shots++;
        for (const f of after) {
          note(
            `AFTER ${tag} field "${f.caption}" <${f.tag}> label ${f.label.toFixed(1)} = span ${f.span.toFixed(1)} + control ${f.control.toFixed(1)} + 4 -> diff ${(f.label - f.span - f.control - 4).toFixed(1)}`,
          );
          if (f.tag !== '-') {
            expect
              .soft(
                Math.abs(f.label - (f.span + f.control + 4)),
                `${tag} "${f.caption}" is as tall as its content`,
              )
              .toBeLessThanOrEqual(2);
          }
        }
        if (main) {
          const n = await holdBackFieldRule(page);
          await page.waitForTimeout(300);
          const base = await fields(page);
          note(`BASE ${tag}: ${n} rule(s) held back`);
          for (const f of base) {
            note(
              `BASE ${tag} field "${f.caption}" <${f.tag}> label ${f.label.toFixed(1)} span ${f.span.toFixed(1)} control ${f.control.toFixed(1)}`,
            );
          }
          if (tool.slug === 'regex') {
            expect(n, 'the rule was found and held back').toBeGreaterThan(0);
            expect(
              base[0]?.label ?? 0,
              'base: the regex pattern label is a 140px box',
            ).toBeGreaterThan(100);
          }
          if (tool.shots === 'all') {
            await page.screenshot({ path: resolve(OUT, `${tool.slug}-${theme}-${vp}-base.png`) });
            shots++;
          }
        }
      } catch (e) {
        note(`UNREACHED ${tag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      }
    }
    await c.close();
  }
  // Click-through: switch between tools in one open drawer.
  await setTheme(octx.request, 'lounge-light');
  const c = await context(browser, octx, DESKTOP, 'lounge-light');
  const page = await c.newPage();
  await page.goto('/channels/@me');
  await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
  await openTool(page, 'regex tester', '.ud-root');
  for (const q of ['diff', 'json', 'stopwatch']) {
    await page.keyboard.press('ControlOrMeta+k');
    await page.keyboard.type(`>${q}`);
    await page.waitForTimeout(600);
    await page.keyboard.press('Enter');
    await expect(page.locator('.ud-root, .ud-timer').first()).toBeVisible();
  }
  note('switch regex -> diff -> json -> stopwatch ok');
  await c.close();
  await octx.close();
  note(`tools: ${shots} shots`);
  expect(shots).toBeGreaterThan(0);
});

// ---- Part 2: the three colours -----------------------------------------------------------------------------------

/** Put the old declaration back on the live rule (the base), or report whether the rule was found. */
const asBase = (page: Page, which: 'shield' | 'scope' | 'elder') =>
  page.evaluate((w) => {
    const want =
      w === 'shield'
        ? { re: /vc-e2ee-shield\[data-verified\]/, old: 'var(--sr-gold)' }
        : w === 'scope'
          ? { re: /^\.us-scope-list$/, old: 'var(--sr-text-2)' }
          : { re: /vouch-badge\[data-tier="elder"\]/, old: 'var(--sr-gold, #c9a24d)' };
    let n = 0;
    for (const sheet of Array.from(document.styleSheets)) {
      let rules: CSSRuleList;
      try {
        rules = sheet.cssRules;
      } catch {
        continue;
      }
      for (const r of Array.from(rules) as CSSStyleRule[]) {
        if (r.selectorText && want.re.test(r.selectorText)) {
          r.style.setProperty('color', want.old);
          n++;
        }
      }
    }
    return n;
  }, which);

/** Computed colour of the element and the first opaque background above it. */
const colourOf = (page: Page, selector: string) =>
  page.evaluate((sel) => {
    const el = document.querySelector<HTMLElement>(sel)!;
    let bg = 'rgba(0, 0, 0, 0)';
    for (let n: HTMLElement | null = el; n; n = n.parentElement) {
      const c = getComputedStyle(n).backgroundColor;
      if (!/rgba\(.*, 0\)$|transparent/.test(c)) {
        bg = c;
        break;
      }
    }
    return { color: getComputedStyle(el).color, bg };
  }, selector);

async function report(
  page: Page,
  tag: string,
  which: 'shield' | 'scope' | 'elder',
  sel: string,
  min: number,
) {
  const after = await colourOf(page, sel);
  const ra = contrast(after.color, after.bg);
  note(`AFTER ${tag}: colour ${after.color} on ${after.bg} = ${ra.toFixed(2)}:1 (needs ${min})`);
  await page.screenshot({ path: resolve(OUT, `${tag}-after.png`) });
  const n = await asBase(page, which);
  await page.waitForTimeout(200);
  const base = await colourOf(page, sel);
  const rb = contrast(base.color, base.bg);
  note(
    `BASE ${tag}: ${n} rule(s) put back; colour ${base.color} on ${base.bg} = ${rb.toFixed(2)}:1`,
  );
  expect.soft(ra, `${tag}: contrast after`).toBeGreaterThanOrEqual(min);
}

test('the scope list and the elder badge read the theme', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  const owner = bootstrapState().owner;
  await loginApi(octx.request, owner);
  for (const theme of ['lounge-light', 'lounge-dark']) {
    await setTheme(octx.request, theme);
    const c = await context(browser, octx, DESKTOP, theme);
    await c.route('**/api/v1/oauth2/tokens', (route) =>
      route.request().method() === 'GET'
        ? route.fulfill({
            json: [
              {
                id: '1',
                application: { id: '2', name: 'Demo App', icon: null },
                scopes: ['identify', 'guilds'],
                created_at: new Date().toISOString(),
              },
            ],
          })
        : route.continue(),
    );
    await c.route('**/api/v1/users/*/vouches', (route) =>
      route.fulfill({
        json: {
          count: 640,
          tier: 'elder',
          multiplier: 3,
          pillar_min: 50,
          elder_min: 500,
          pillar_multiplier: 1.5,
          elder_multiplier: 3,
          viewer_vouched: false,
          can_vouch: false,
          is_self: false,
        },
      }),
    );
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    try {
      await page.goto('/settings/user/apps');
      await expect(page.locator('.us-scope-list').first()).toBeVisible({ timeout: 30_000 });
      note(`scope ${theme}: page theme ${await themeOf(page)}`);
      await report(page, `scope-${theme}`, 'scope', '.us-scope-list', 4.5);
    } catch (e) {
      note(
        `UNREACHED scope-${theme}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`,
      );
    }
    try {
      await page.goto(`/u/${owner.username}`);
      await expect(page.locator('.vouch-badge[data-tier="elder"]').first()).toBeVisible({
        timeout: 30_000,
      });
      note(`elder ${theme}: page theme ${await themeOf(page)}`);
      await report(page, `elder-${theme}`, 'elder', '.vouch-badge[data-tier="elder"]', 4.5);
    } catch (e) {
      note(
        `UNREACHED elder-${theme}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`,
      );
    }
    await c.close();
  }
  await octx.close();
});

test('the verified shield reads the theme', async ({ browser }) => {
  test.skip(!VOICE, 'voice run only (SR_P80_VOICE=1, server with SR_VOICE__ENABLED=true)');
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    {
      name: 'Shield Room',
    },
  );
  const text = guild.channels.find((c) => c.type === 0)!.id;
  const voice =
    guild.channels.find((c) => c.type === 2)?.id ??
    (await apiPost<{ id: string }>(a, `/guilds/${guild.id}/channels`, { name: 'Calls', type: 2 }))
      .id;
  const invite = await apiPost<{ code: string }>(a, `/channels/${text}/invites`, {});
  const bobCtx = await browser.newContext({ baseURL: ORIGIN, permissions: ['microphone'] });
  const bob = await registerUser(bobCtx.request, 'bobby');
  await loginApi(bobCtx.request, bob);
  await apiPost(bobCtx.request, `/invites/${invite.code}`, {});
  const bobPage = await bobCtx.newPage();
  await bobPage.addInitScript(() => {
    localStorage.setItem(
      'sr.user.prefs',
      JSON.stringify({ state: { prefs: { theme: 'lounge-dark', zoom: 100 } }, version: 2 }),
    );
  });
  let bobIn = false;
  for (const theme of ['lounge-light', 'lounge-dark']) {
    await setTheme(a, theme);
    const c = await context(browser, octx, DESKTOP, theme);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    await page.goto(`/channels/${guild.id}/${voice}`);
    await page.waitForTimeout(1500);
    note(`shield ${theme}: room wears ${await themeOf(page)}`);
    await page.getByRole('button', { name: 'Join Voice' }).first().click();
    const ok = await page
      .locator('.vc-state[data-ok]')
      .first()
      .waitFor({ state: 'attached', timeout: 60_000 })
      .then(() => true)
      .catch(() => false);
    if (!ok) {
      note(`UNREACHED shield-${theme}: the call did not connect`);
      await page.screenshot({ path: resolve(OUT, `shield-${theme}-notconnected.png`) });
      await c.close();
      continue;
    }
    if (!bobIn) {
      await bobPage.goto(`/channels/${guild.id}/${voice}`);
      await bobPage.getByRole('button', { name: 'Join Voice' }).first().click();
      await bobPage
        .locator('.vc-state[data-ok]')
        .first()
        .waitFor({ state: 'attached', timeout: 60_000 });
      bobIn = true;
    }
    await page.waitForTimeout(2500);
    const shield = page.locator('.vc-e2ee-shield:visible').first();
    await shield.click();
    const dialog = page.getByRole('dialog').last();
    const verify = dialog.locator('.vc-e2ee-verify').first();
    if (await verify.isVisible().catch(() => false)) {
      if ((await verify.getAttribute('aria-pressed')) !== 'true') await verify.click();
    } else {
      note(`shield ${theme}: no verify button in the popover`);
    }
    await page.waitForTimeout(500);
    await page.screenshot({ path: resolve(OUT, `shield-popover-${theme}.png`) });
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
    const verified = await page.locator('.vc-e2ee-shield[data-verified]:visible').count();
    note(`shield ${theme}: shields with data-verified: ${verified}`);
    if (verified > 0) {
      await report(page, `shield-${theme}`, 'shield', '.vc-e2ee-shield[data-verified]', 3);
    } else {
      note(`UNREACHED shield-${theme}: the shield did not become verified`);
    }
    const leave = page.getByRole('button', { name: 'Disconnect' }).first();
    if (await leave.isVisible().catch(() => false)) await leave.click().catch(() => undefined);
    await page.waitForTimeout(800);
    await c.close();
  }
  await bobCtx.close();
  await octx.close();
});

// ---- Part 3: findings only -----------------------------------------------------------------------------------------

interface Strip {
  overflow: string | null;
  selected: string;
  clientW: number;
  scrollW: number;
  scrollLeft: number;
  listLeft: number;
  listRight: number;
  tabs: { name: string; left: number; right: number; selected: boolean }[];
  mask: string;
  dir: string;
}

async function strip(page: Page, label?: string): Promise<Strip | null> {
  return page.evaluate((lbl) => {
    const lists = Array.from(document.querySelectorAll<HTMLElement>('[role="tablist"]'));
    const el = lbl ? lists.find((l) => l.getAttribute('aria-label') === lbl) : lists.at(-1);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    const tabs = Array.from(el.querySelectorAll<HTMLElement>('[role="tab"]')).map((t) => {
      const b = t.getBoundingClientRect();
      return {
        name: (t.textContent ?? '').trim(),
        left: b.left,
        right: b.right,
        selected: t.getAttribute('aria-selected') === 'true',
      };
    });
    return {
      overflow: el.getAttribute('data-overflow'),
      selected: tabs.find((t) => t.selected)?.name ?? '',
      clientW: el.clientWidth,
      scrollW: el.scrollWidth,
      scrollLeft: el.scrollLeft,
      listLeft: r.left + el.clientLeft,
      listRight: r.left + el.clientLeft + el.clientWidth,
      tabs,
      mask: cs.maskImage || 'none',
      dir: getComputedStyle(document.documentElement).direction,
    };
  }, label);
}

const fmt = (where: string, s: Strip | null) =>
  s
    ? `${where}: dir=${s.dir} overflow=${s.overflow} selected=${s.selected} scroll ${s.scrollLeft.toFixed(0)}/${s.scrollW}/${s.clientW} list ${s.listLeft.toFixed(0)}..${s.listRight.toFixed(0)} tabs ${s.tabs.map((t) => `${t.name}[${t.left.toFixed(0)}..${t.right.toFixed(0)}]`).join(' ')} mask ${s.mask === 'none' ? 'none' : 'yes'}`
    : `${where}: NOT REACHED (no tab list)`;

test('findings: the Wheel and the kit Tabs', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    {
      name: 'Tabs Room',
    },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  await apiPost(a, `/channels/${channel}/messages`, { content: 'Something to save' });

  for (const theme of ['lounge-light', 'lounge-dark']) {
    await setTheme(a, theme);
    // The Wheel, desktop.
    {
      const c = await context(browser, octx, DESKTOP, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      try {
        await page.goto('/channels/@me');
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        await openTool(page, 'wheel', '.ud-root');
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `wheel-${theme}.png`) });
        note(`wheel ${theme}: shot (${await themeOf(page)})`);
      } catch (e) {
        note(
          `UNREACHED wheel-${theme}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`,
        );
      }
      await c.close();
    }
    // Saved items under dir=rtl, phone.
    {
      const c = await context(browser, octx, PHONE, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      try {
        await page.goto(`/channels/${guild.id}/${channel}`);
        await expect(page.locator('.lng-composer-wrap').last().locator('textarea')).toBeVisible();
        await page.waitForTimeout(600);
        await page.evaluate(() => {
          document.documentElement.dir = 'rtl';
        });
        await page.waitForTimeout(300);
        await page.getByRole('button', { name: 'Saved items', exact: true }).first().click();
        await expect(page.getByRole('tab', { name: 'Bookmarks' })).toBeVisible();
        await page.waitForTimeout(600);
        note(fmt(`RTL saved open ${theme}`, await strip(page, 'Saved items')));
        await page.screenshot({ path: resolve(OUT, `tabs-rtl-saved-${theme}.png`) });
        await page.getByRole('tab', { name: /^Scheduled/ }).click();
        await page.waitForTimeout(700);
        note(fmt(`RTL saved scheduled ${theme}`, await strip(page, 'Saved items')));
        await page.screenshot({ path: resolve(OUT, `tabs-rtl-saved-scheduled-${theme}.png`) });
      } catch (e) {
        note(
          `UNREACHED rtl-${theme}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`,
        );
      }
      await c.close();
    }
  }
  // Admin and a server's settings, phone, Light.
  await setTheme(a, 'lounge-light');
  const c = await context(browser, octx, PHONE, 'lounge-light');
  const page = await c.newPage();
  page.setDefaultTimeout(20_000);
  for (const [name, url] of [
    ['admin', '/admin/moderation'],
    ['server-bridge', `/settings/server/${guild.id}/bridge`],
  ] as const) {
    try {
      await page.goto(url);
      await page.waitForTimeout(2500);
      const s = await strip(page);
      note(fmt(`${name} phone Light`, s));
      await page.screenshot({ path: resolve(OUT, `tabs-${name}-phone-lounge-light.png`) });
    } catch (e) {
      note(`UNREACHED ${name}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
    }
  }
  await c.close();
  await octx.close();
});
