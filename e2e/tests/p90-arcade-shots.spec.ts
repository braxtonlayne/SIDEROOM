import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-388 (wave 90; D-900): Admin > Arcade's heading is Admin's section header and its two selects are labelled
 * kit controls like Pricing's. Runs on the visual sweep's seeded world (`e2e/.state/sweep/world.json`, owner in
 * `e2e/.state/sweep/owner.json`, both made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never
 * edited), entering Admin by URL as wave 84's spec does (no re-auth step).
 * Three settings: 375x812 touch in Lounge Light and Lounge Dark, 1280x800 in Editorial (the account's main theme by
 * PATCH and localStorage; `data-theme` is read back from the page before each shot and written into the file name).
 * Per setting: Admin > Arcade and Admin > Pricing (the reference) are measured: heading font, size, weight, colour,
 * start edge; each select's box, visible label, accessible name, border, radius, hit test (phone); the document's
 * sideways scroll; the boxes of the page's other blocks. After case only, at 1280 Editorial: the flows (change the
 * tier select, change the row's "Released as" select, add an entry).
 *
 * Local only. Command (after the stack and the seed are up; the base case is the same with SR_P90_ARCADE_BASE=1):
 *   cd e2e && SR_P90_ARCADE_ON=1 SR_P90_ARCADE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-arcade \
 *   SR_E2E_BASE_URL=http://localhost:5684 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db url> DATABASE_URL=<db url> \
 *   npx playwright test tests/p90-arcade-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Variables: SR_P90_ARCADE_ON, SR_P90_ARCADE_OUT, SR_P90_ARCADE_BASE (file names get a `base-` prefix, no flows).
 */
const ON = process.env.SR_P90_ARCADE_ON === '1';
const BASE = process.env.SR_P90_ARCADE_BASE === '1';
const OUT =
  process.env.SR_P90_ARCADE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-arcade';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SETTINGS = [
  { key: 'phone-light', w: 375, h: 812, phone: true, theme: 'lounge-light' },
  { key: 'phone-dark', w: 375, h: 812, phone: true, theme: 'lounge-dark' },
  { key: 'desk', w: 1280, h: 800, phone: false, theme: 'lounge-editorial' },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

const idle = (p: Page, ms = 3000) =>
  p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);

async function newPage(browser: Browser, s: (typeof SETTINGS)[number], base: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: s.w, height: s.h },
    storageState: await base.storageState(),
    bypassCSP: true,
    ...(s.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((th) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: th, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

/** In the page: computed strings are logged as the browser gives them (no colour ratio is taken here). */
function measure(phone: boolean) {
  const de = document.documentElement;
  const r4 = (r: DOMRect) => [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10);
  const cs = (e: Element, ...props: string[]) => {
    const c = getComputedStyle(e);
    return Object.fromEntries(props.map((p) => [p, c.getPropertyValue(p)]));
  };
  const h = document.querySelector<HTMLElement>('#arc-admin, .ss-header .ss-title');
  const heading = h && {
    tag: h.tagName.toLowerCase(),
    text: h.textContent,
    box: r4(h.getBoundingClientRect()),
    ...cs(h, 'font-family', 'font-size', 'font-weight', 'color', 'line-height'),
  };
  const controls = Array.from(
    document.querySelectorAll<HTMLElement>(
      '.arc-section select, .arc-section [role=combobox], .adm-pick [role=combobox], .adm-pick select',
    ),
  ).map((el) => {
    const id = el.id;
    const lab = id ? document.querySelector(`label[for="${id}"]`) : null;
    const wrap = el.closest('label');
    const labelEl = lab ?? wrap;
    const labelText = (labelEl?.textContent ?? '').trim();
    const ariaName =
      el.getAttribute('aria-label') ??
      (el.getAttribute('aria-labelledby')
        ? (document.getElementById(el.getAttribute('aria-labelledby') ?? '')?.textContent ?? null)
        : null);
    el.scrollIntoView({ block: 'center' });
    const r = el.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const ok = (y: number) => {
      const t = document.elementFromPoint(cx, y);
      return !!t && (t === el || el.contains(t) || !!labelEl?.contains(t));
    };
    return {
      tag: el.tagName.toLowerCase(),
      id,
      box: r4(r),
      visibleLabel: labelText,
      labelledByFor: !!lab,
      labelWraps: !!wrap,
      accessibleName: ariaName ?? labelText,
      hit: phone ? ok(cy - 21) && ok(cy + 21) : null,
      ...cs(
        el,
        'border-top-width',
        'border-top-color',
        'border-top-left-radius',
        'height',
        'font-size',
        'background-color',
        'color',
      ),
      labelStyle: labelEl && cs(labelEl, 'font-size', 'font-weight', 'text-transform', 'color'),
    };
  });
  window.scrollTo(0, 0);
  for (const e of Array.from(document.querySelectorAll<HTMLElement>('body *')))
    if (e.scrollTop > 0) e.scrollTop = 0;
  const blocks = Array.from(
    document.querySelectorAll<HTMLElement>(
      '.arc-section h3, .arc-section .arc-form, .arc-admin-list',
    ),
  ).map((e) => ({
    sel: `${e.tagName.toLowerCase()}.${e.className}`.slice(0, 40),
    box: r4(e.getBoundingClientRect()),
  }));
  const themed =
    document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]');
  return {
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    heading,
    controls,
    blocks,
  };
}

test('SEC-AUD-388: Admin > Arcade heading and selects', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const log = resolve(OUT, BASE ? 'base-measure.json' : 'measure.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const pre = BASE ? 'base-' : '';
  let seeded = false;

  for (const s of SETTINGS) {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT ${s.key}: account theme set ${s.theme}, read back ${back.settings?.user_prefs?.theme}`,
    );
    const { ctx, page } = await newPage(browser, s, ownerCtx);

    const open = async (section: string) => {
      await page.goto(`/admin/${section}`, { waitUntil: 'domcontentloaded', timeout: 15_000 });
      await idle(page, 5000);
      await page.waitForTimeout(800);
    };
    const addEntry = async (title: string, repo: string) => {
      await page.getByLabel('Title').first().fill(title);
      await page.getByLabel('Summary').first().fill('A flow-check entry.');
      await page.getByLabel('Repository link').first().fill(repo);
      await page.getByRole('button', { name: /^Add to the showcase/ }).click();
      await page.waitForTimeout(1500);
    };

    // An entry for the row's select to sit in: made through the form once, in whichever case runs first.
    if (!seeded) {
      await open('arcade');
      if ((await page.locator('.arc-release').count()) === 0) {
        await addEntry('Starfall', 'https://example.com/starfall');
      }
      console.log(`REPORT coming-soon rows: ${await page.locator('.arc-release').count()}`);
      seeded = true;
    }

    for (const section of ['donation-pricing', 'arcade']) {
      await open(section);
      const m = await page.evaluate(measure, s.phone);
      const stem = `${pre}${section === 'arcade' ? 'arcade' : 'pricing'}-${s.w}-${m.theme}`;
      await page.screenshot({ path: resolve(OUT, `${stem}.png`) });
      appendFileSync(log, `${JSON.stringify({ key: s.key, section, ...m })}\n`);
      console.log(
        `REPORT ${s.key} ${section} theme=${m.theme} doc=${m.doc} heading=${JSON.stringify(m.heading)} controls=${JSON.stringify(m.controls.map((c) => ({ tag: c.tag, box: c.box, name: c.accessibleName, label: c.visibleLabel, hit: c.hit })))}`,
      );
    }

    if (!BASE && !s.phone) {
      await open('arcade');
      const tier = page.getByRole('combobox', { name: 'Content tier' });
      await tier.click();
      await page.getByRole('option', { name: /Reimplementation/i }).click();
      const tierText = await tier.textContent();
      const rel = page
        .locator('.arc-release')
        .first()
        .getByRole('combobox', { name: 'Released as' });
      await rel.click();
      const opts = await page.getByRole('option').allTextContents();
      await page.keyboard.press('Escape');
      let picked: string | null = null;
      if (opts.length > 1) {
        await rel.click();
        await page.getByRole('option').nth(1).click();
        picked = await rel.textContent();
      }
      await addEntry('Flow entry', 'https://example.com/flow');
      const rows = await page.locator('.arc-release').count();
      const notice = await page
        .locator('.arc-section [role=alert], .arc-section [role=status]')
        .allTextContents();
      console.log(
        'REPORT notices ' +
          JSON.stringify(notice) +
          ' titles ' +
          JSON.stringify(await page.locator('.arc-admin-list strong').allTextContents()),
      );
      console.log(
        `REPORT flows: tier now ${tierText}; released-as options ${JSON.stringify(opts)}; picked ${picked}; rows after add ${rows}`,
      );
      appendFileSync(log, `${JSON.stringify({ flows: true, tierText, opts, picked, rows })}\n`);
      // Remove the flow entry so a re-run starts from the same world.
      await page
        .getByRole('button', { name: 'Remove Flow entry' })
        .click()
        .catch(() => undefined);
      await page.waitForTimeout(800);
    }
    await ctx.close();
  }
});
