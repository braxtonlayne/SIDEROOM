import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';

/**
 * SEC-AUD-389 (wave 90; D-900): the Entrance admin page's hero image control. The Owner of the visual sweep's seeded
 * world (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never
 * edited) opens Admin > Entrance pages (`/admin/entrance`, the way `p84-admin-shots.spec.ts` gets in). Three settings:
 * 375x812 touch in Lounge Light and in Lounge Dark, and 1280x800 in Editorial (the account's main theme by PATCH and
 * localStorage, `data-theme` read back before each shot).
 *
 * BASE (`SR_P90_ENTRANCE_BASE=1`, with the page's file control as it was: the bare `<input type="file">`): measures the
 * control's box, hit test, accessible name, the hero card, every other bare native control of the page, uploads a
 * small in-memory PNG, tries a text file; shoots the card at 375 Lounge Light and 1280 Editorial.
 * AFTER (default): the same, plus: Tab reaches the control, Enter and Space open the picker (the `filechooser`
 * event), a text file is refused, a PNG chosen through `setInputFiles` uploads and the preview shows; shoots the card
 * at 375 Lounge Light (a second file chosen, the preview showing), 375 Lounge Dark and 1280 Editorial.
 * One JSON line per setting in `measure.json`.
 *
 * Local only. The commander's command, from `e2e/`, on a stack whose DB holds the sweep world (ports as the wave's
 * table: API 8685, Vite 5685):
 *   SR_P90_ENTRANCE_ON=1 SR_P90_ENTRANCE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-entrance \
 *   SR_E2E_BASE_URL=http://localhost:5685 npx playwright test tests/p90-entrance-shots.spec.ts \
 *   --project=chromium --workers=1 --retries=0 --reporter=line
 * (add SR_P90_ENTRANCE_BASE=1 and SR_P90_ENTRANCE_TAG=base for the base case against the old file.)
 */
const ON = process.env.SR_P90_ENTRANCE_ON === '1';
const BASE = process.env.SR_P90_ENTRANCE_BASE === '1';
const TAG = process.env.SR_P90_ENTRANCE_TAG ?? (BASE ? 'base' : 'after');
const OUT =
  process.env.SR_P90_ENTRANCE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-entrance';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SETTINGS = [
  { key: 'phone-light', phone: true, theme: 'lounge-light', w: 375, h: 812 },
  { key: 'phone-dark', phone: true, theme: 'lounge-dark', w: 375, h: 812 },
  { key: 'desk', phone: false, theme: 'lounge-editorial', w: 1280, h: 800 },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

/** In the page: the box, the hit test (D-856) and the accessible name of one control. */
async function probe(el: Locator, phone: boolean) {
  return el.evaluate((e, ph) => {
    e.scrollIntoView({ block: 'center' });
    const r = e.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const labels = (e as HTMLInputElement).labels
      ? Array.from((e as HTMLInputElement).labels!)
      : [];
    const own = e.closest('label');
    const ok = (y: number) => {
      const h = document.elementFromPoint(cx, y);
      return (
        !!h &&
        (h === e ||
          e.contains(h) ||
          labels.some((l) => l === h || l.contains(h)) ||
          (own !== null && own.contains(h)))
      );
    };
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const name =
      e.getAttribute('aria-label') ||
      (labels[0]?.textContent ?? '').trim() ||
      (e.textContent ?? '').trim() ||
      '';
    return {
      tag: e.tagName.toLowerCase(),
      type: (e as HTMLInputElement).type ?? null,
      box: [rnd(r.left), rnd(r.top), rnd(r.width), rnd(r.height)],
      hit: ph ? ok(cy - 21) && ok(cy + 21) : null,
      name: name.slice(0, 60),
      cls: (typeof e.className === 'string' ? e.className : '').slice(0, 60),
    };
  }, phone);
}

/** A 640x360 PNG made in the page (no file on disk, no download). */
async function makePng(page: Page, hue: number): Promise<Buffer> {
  const b64 = await page.evaluate((h) => {
    const c = document.createElement('canvas');
    c.width = 640;
    c.height = 360;
    const g = c.getContext('2d')!;
    const gr = g.createLinearGradient(0, 0, 640, 360);
    gr.addColorStop(0, `hsl(${h} 70% 55%)`);
    gr.addColorStop(1, `hsl(${(h + 80) % 360} 70% 35%)`);
    g.fillStyle = gr;
    g.fillRect(0, 0, 640, 360);
    return c.toDataURL('image/png').split(',')[1]!;
  }, hue);
  return Buffer.from(b64, 'base64');
}

async function openEntrance(browser: Browser, s: (typeof SETTINGS)[number]) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: s.w, height: s.h },
    storageState: OWNER_STATE,
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
  page.setDefaultTimeout(15_000);
  await page.goto('/admin/entrance', { waitUntil: 'domcontentloaded', timeout: 20_000 });
  await page.locator('#ent-hero-title').waitFor({ timeout: 30_000 });
  await page.waitForTimeout(1500);
  return { ctx, page };
}

test('SEC-AUD-389: the Entrance hero image control', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const log = resolve(OUT, `measure-${TAG}.json`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const results: Record<string, unknown>[] = [];

  for (const s of SETTINGS) {
    const o = ownerCtx.request;
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${s.theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );

    const { ctx, page } = await openEntrance(browser, s);
    const card = page.locator('section[aria-labelledby="ent-hero-title"]');
    const row: Record<string, unknown> = { tag: TAG, setting: s.key };
    const theme = await page.evaluate(
      () =>
        (
          document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
        )?.getAttribute('data-theme') ?? 'no-theme',
    );
    row.theme = theme;
    expect(theme).toBe(s.theme);

    // Start from no image: remove one an earlier setting uploaded (the page's own Remove, then its confirm).
    const removeBtn = card.getByRole('button', { name: 'Remove image' });
    if (await removeBtn.isEnabled()) {
      await removeBtn.click();
      await page
        .getByRole('dialog', { name: 'Remove the hero image?' })
        .getByRole('button', { name: 'Remove image' })
        .click();
      await expect(card.getByText('No image set.')).toBeVisible();
    }
    await page.waitForTimeout(400);

    const control = BASE
      ? card.locator('input[type=file]')
      : card.getByRole('button', { name: 'Choose an image' });
    row.control = await probe(control, s.phone);
    row.heroCardEmpty = await card.evaluate(
      (e) => Math.round(e.getBoundingClientRect().height * 10) / 10,
    );
    // The page's bare native controls: no kit class (kit classes are CSS-module hashes starting `_` or `module`).
    row.bare = await page.evaluate((ph) => {
      const out: unknown[] = [];
      const els = Array.from(
        document.querySelectorAll<HTMLElement>(
          'main input, main select, main textarea, main button, input, select, textarea',
        ),
      );
      const seen = new Set<Element>();
      for (const e of els) {
        if (seen.has(e)) continue;
        seen.add(e);
        const r = e.getBoundingClientRect();
        if (e.closest('nav') || (e as HTMLInputElement).type === 'hidden') continue;
        const cls = typeof e.className === 'string' ? e.className : '';
        const kit = cls.split(/\s+/).some((c) => /^_/.test(c) || /module/.test(c));
        if (kit || (r.width === 0 && r.height === 0)) continue;
        const cx = r.left + r.width / 2;
        const cy = r.top + r.height / 2;
        const labels = (e as HTMLInputElement).labels
          ? Array.from((e as HTMLInputElement).labels!)
          : [];
        const okAt = (y: number) => {
          const h = document.elementFromPoint(cx, y);
          return !!h && (h === e || e.contains(h) || labels.some((l) => l === h || l.contains(h)));
        };
        out.push({
          tag: e.tagName.toLowerCase(),
          type: (e as HTMLInputElement).type,
          cls: cls.slice(0, 40),
          box: [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10),
          hitNote: ph ? 'checked only when on screen' : null,
          hit:
            ph && r.top > 0 && r.bottom < window.innerHeight
              ? okAt(cy - 21) && okAt(cy + 21)
              : null,
          aria: e.getAttribute('aria-label') ?? (labels[0]?.textContent ?? '').trim().slice(0, 40),
        });
      }
      return out;
    }, s.phone);

    const input = card.locator('input[type=file]');
    // A wrong type.
    await input.setInputFiles({
      name: 'notes.txt',
      mimeType: 'text/plain',
      buffer: Buffer.from('hello'),
    });
    await page.waitForTimeout(300);
    row.wrongTypeNotice = await card.getByRole('alert').allTextContents();
    row.uploadEnabledAfterWrongType = await card
      .getByRole('button', { name: 'Upload image' })
      .isEnabled();

    if (!BASE) {
      // Tab reaches the control; Enter and Space open the picker.
      await card.locator('h2').evaluate((e) => {
        (e as HTMLElement).tabIndex = -1;
        e.focus();
      });
      let reached = -1;
      for (let i = 1; i <= 8; i++) {
        await page.keyboard.press('Tab');
        const on = await page.evaluate(() => {
          const a = document.activeElement as HTMLElement | null;
          return a?.tagName === 'BUTTON' && (a.textContent ?? '').trim() === 'Choose an image';
        });
        if (on) {
          reached = i;
          break;
        }
      }
      row.tabsToControl = reached;
      for (const key of ['Enter', 'Space']) {
        const chooser = page.waitForEvent('filechooser', { timeout: 5000 }).then(
          () => true,
          () => false,
        );
        await page.keyboard.press(key === 'Space' ? ' ' : 'Enter');
        row[`filechooser${key}`] = await chooser;
      }
    }

    // A PNG chosen through the input uploads, and the preview shows.
    const png1 = await makePng(page, 210);
    await input.setInputFiles({ name: 'door.png', mimeType: 'image/png', buffer: png1 });
    await expect(card.getByRole('button', { name: 'Upload image' })).toBeEnabled();
    row.noticeAfterPng = await card.getByRole('alert').allTextContents();
    await card.getByRole('button', { name: 'Upload image' }).click();
    const dlg = page.getByRole('dialog', { name: 'Replace the hero image?' });
    await dlg.getByLabel(/Reason/).fill('p90 shot');
    await dlg.getByRole('button', { name: 'Upload image' }).click();
    const reauth = page.getByRole('dialog', { name: 'Confirm it is you' });
    await Promise.race([
      card.getByText('Hero image saved.').waitFor({ timeout: 15_000 }),
      reauth.waitFor({ timeout: 15_000 }).then(() => {
        throw new Error('re-auth asked on upload');
      }),
    ]);
    await expect(card.locator('img')).toBeVisible();
    await page.waitForTimeout(600);
    row.uploaded = true;
    row.heroCardWithImage = await card.evaluate(
      (e) => Math.round(e.getBoundingClientRect().height * 10) / 10,
    );

    // The second file chosen (its name shows on the kit control), for the shot.
    const png2 = await makePng(page, 20);
    await input.setInputFiles({ name: 'door-two.png', mimeType: 'image/png', buffer: png2 });
    await page.waitForTimeout(400);
    row.heroCardWithImageAndFile = await card.evaluate(
      (e) => Math.round(e.getBoundingClientRect().height * 10) / 10,
    );
    row.fileNameShown = BASE ? null : await card.getByText('door-two.png').isVisible();
    const de = await page.evaluate(() => ({
      doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    }));
    row.sidewaysScroll = de.doc;

    const wantShot = BASE ? s.key !== 'phone-dark' : true;
    if (wantShot) {
      await card.scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      const f = `${TAG}-${s.key}-${theme}.png`;
      await page.screenshot({ path: resolve(OUT, f), timeout: 15_000 });
      row.shot = f;
    }
    results.push(row);
    appendFileSync(log, `${JSON.stringify(row)}\n`);
    console.log(`REPORT ${JSON.stringify(row)}`);
    await ctx.close();
  }
  await ownerCtx.close();
});
