import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-307 (wave 78; D-804), on the real stack: every visible native file input draws its picker button as the
 * kit's secondary button. For each input a fresh stack reaches the spec reads `getComputedStyle(input,
 * '::file-selector-button')` (background, radius, border, font), compares it with the theme's tokens (read from a
 * probe element next to the input), and takes a clipped shot of the control with its label. Pages: the Library's
 * submission page (the folder input and a chapter's input; shipped theme and a light theme chosen for the
 * section, D-797; desktop and phone), the hash tool, the notes tool and the audio trimmer (the drawer), the
 * server settings' soundboard upload, the Admin's Entrance page; Lounge Dark and Lounge Light. A target a fresh
 * stack cannot reach is logged as `UNREACHED` and the run goes on.
 * Local only (SR_P78_FILES=1); shots go to SR_P78_FILES_OUT. SR_P78_BASE=1: the base case (probe and shots, no
 * assertion on the look).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P78_FILES_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-files';
const BASE = process.env.SR_P78_BASE === '1';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P78_FILES !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number; sectionThemes?: Record<string, string> };

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    bypassCSP: true,
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

const log = (line: string) => console.log(`REPORT ${line}`);
let shots = 0;
let probed = 0;

/** The picker button's computed look, and the tokens it should have (read where the input sits). */
async function look(input: Locator) {
  return input.evaluate((el) => {
    const pseudo = getComputedStyle(el, '::file-selector-button');
    const host = el.parentElement ?? el;
    const probe = document.createElement('span');
    probe.style.cssText =
      'position:absolute;visibility:hidden;background:var(--ui-bg-hover, var(--sr-bg-2));color:var(--ui-text, var(--sr-text));border:1px solid var(--ui-border, var(--sr-line));border-radius:var(--ui-radius, var(--sr-radius))';
    host.appendChild(probe);
    const want = getComputedStyle(probe);
    const out = {
      bg: pseudo.backgroundColor,
      color: pseudo.color,
      radius: pseudo.borderTopLeftRadius,
      border: pseudo.borderTopColor,
      borderWidth: pseudo.borderTopWidth,
      font: pseudo.fontFamily,
      cursor: pseudo.cursor,
      wantBg: want.backgroundColor,
      wantColor: want.color,
      wantRadius: want.borderTopLeftRadius,
      wantBorder: want.borderTopColor,
      hostFont: getComputedStyle(el).fontFamily,
      inputWidth: el.getBoundingClientRect().width,
      parentWidth: (el.parentElement ?? el).getBoundingClientRect().width,
      scrollOver: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
    probe.remove();
    return out;
  });
}

/** Probe one visible input, shoot its surroundings; with the change in, the picker button matches the kit's. */
async function check(page: Page, input: Locator, name: string, wide = false) {
  await input.scrollIntoViewIfNeeded();
  await expect(input).toBeVisible();
  const l = await look(input);
  probed++;
  log(`${name} ${JSON.stringify(l)}`);
  if (!BASE) {
    expect.soft(l.bg, `${name}: background`).toBe(l.wantBg);
    expect.soft(l.color, `${name}: colour`).toBe(l.wantColor);
    expect.soft(l.radius, `${name}: radius`).toBe(l.wantRadius);
    expect.soft(l.border, `${name}: border colour`).toBe(l.wantBorder);
    expect.soft(l.borderWidth, `${name}: border width`).toBe('1px');
    expect.soft(l.font, `${name}: font`).toBe(l.hostFont);
    expect.soft(l.cursor, `${name}: cursor`).toBe('pointer');
    expect
      .soft(l.inputWidth, `${name}: not wider than its box`)
      .toBeLessThanOrEqual(l.parentWidth + 1);
  }
  // The Library's section bar and the review table already overflow a phone on the base (logged as OVERFLOW).
  if (page.viewportSize()!.width >= 500) {
    expect.soft(l.scrollOver, `${name}: no sideways scroll`).toBeLessThanOrEqual(0);
  }
  if (l.scrollOver > 0) {
    const wide = await page.evaluate(() =>
      Array.from(document.querySelectorAll('body *'))
        .filter((e) => e.getBoundingClientRect().right > document.documentElement.clientWidth + 1)
        .slice(0, 8)
        .map(
          (e) =>
            e.tagName +
            '.' +
            String((e as HTMLElement).className).slice(0, 30) +
            '@' +
            Math.round(e.getBoundingClientRect().right),
        ),
    );
    log(name + ' OVERFLOW ' + JSON.stringify(wide));
  }
  await page.waitForTimeout(250);
  const vp = page.viewportSize()!;
  if (vp.width < 500) {
    await input.evaluate((el) => {
      window.scrollTo(0, 0);
      el.scrollIntoView({ block: 'center', inline: 'start' });
    });
    await page.waitForTimeout(200);
    await page.screenshot({ path: resolve(OUT, name + '.png') });
    shots++;
    return;
  }
  const box = (await input.boundingBox())!;
  const x = Math.max(0, box.x - 260);
  const y = Math.max(0, box.y - 90);
  const w = Math.min(vp.width - x, wide ? 760 : 620);
  const h = Math.min(vp.height - y, 220);
  await page.screenshot({ path: resolve(OUT, `${name}.png`), clip: { x, y, width: w, height: h } });
  shots++;
}

async function reach(name: string, run: () => Promise<void>) {
  try {
    await run();
  } catch (e) {
    const msg = String(e instanceof Error ? e.message : e).split('\n')[0];
    log(`UNREACHED ${name}: ${msg}`);
  }
}

async function openTool(page: Page, query: string, ready: Locator) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.keyboard.press('Enter');
  await expect(ready).toBeVisible({ timeout: 15_000 });
}

test('file inputs wear the kit button', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });
  const guild = await apiPost<{ id: string }>(octx.request, '/guilds', { name: 'File Booth' });
  const d = await apiPost<{ submission: { id: string } }>(octx.request, '/submissions', {
    kind: 'library.series',
    payload: { kind: 'manga', title: 'Files 78', authors: ['E2E'], genres: [], synopsis: '' },
  });
  const sub = d.submission.id;
  try {
    await apiPost(octx.request, `/submissions/${sub}/library/chapters`, {
      number: 1,
      title: null,
      format: 'images',
    });
  } catch {
    /* chapter 1 may exist already */
  }

  // ---- The Library's submission page: shipped theme and a light theme chosen for the section (D-797).
  const variants: Array<[string, Prefs, string]> = [
    [
      'shipped',
      { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: 'sideroom' } },
      'sideroom',
    ],
    [
      'light',
      { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: 'lounge-light' } },
      'lounge-light',
    ],
  ];
  for (const [vname, prefs, want] of variants) {
    await setPrefs(prefs);
    for (const [vp, size] of [
      ['desktop', DESKTOP],
      ['phone', PHONE],
    ] as const) {
      const c = await context(browser, octx, size, prefs);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/library/submissions/${sub}`);
      await expect(page.getByRole('heading', { name: 'Files 78' })).toBeVisible({
        timeout: 60_000,
      });
      const theme = await page.locator('.lib-root').first().getAttribute('data-theme');
      log(`library-${vname}-${vp} data-theme=${theme}`);
      expect(theme, 'the room wears the chosen theme').toBe(want);
      await reach(`library-chapter-${vname}-${vp}`, async () => {
        await check(
          page,
          page.locator('input[type=file]:not([webkitdirectory])').first(),
          `library-chapter-${vname}-${vp}`,
        );
      });
      await reach(`library-folder-${vname}-${vp}`, async () => {
        await check(
          page,
          page.locator('input[type=file][webkitdirectory]').first(),
          `library-folder-${vname}-${vp}`,
        );
      });
      if (vp === 'desktop' && vname === 'light') {
        // States: hover, keyboard focus.
        const chapter = page.locator('input[type=file]:not([webkitdirectory])').first();
        await reach('library-states', async () => {
          await chapter.hover();
          await page.waitForTimeout(250);
          const hovered = await chapter.evaluate(
            (el) => getComputedStyle(el, '::file-selector-button').borderTopColor,
          );
          log(`hover border ${hovered}`);
          await chapter.scrollIntoViewIfNeeded();
          const b = (await chapter.boundingBox())!;
          await page.screenshot({
            path: resolve(OUT, 'library-hover-light-desktop.png'),
            clip: { x: Math.max(0, b.x - 260), y: Math.max(0, b.y - 90), width: 620, height: 200 },
          });
          shots++;
          await chapter.focus();
          await page.keyboard.press('Shift+Tab');
          await page.keyboard.press('Tab');
          await expect(chapter).toBeFocused();
          const ring = await chapter.evaluate((el) => getComputedStyle(el).outlineStyle);
          log(`focus-visible outline ${ring}`);
          if (!BASE) expect.soft(ring).not.toBe('none');
          const fb = (await chapter.boundingBox())!;
          await page.screenshot({
            path: resolve(OUT, 'library-focus-light-desktop.png'),
            clip: { x: Math.max(0, fb.x - 20), y: Math.max(0, fb.y - 60), width: 620, height: 160 },
          });
          shots++;
        });
        // Click through: a file set on the chapter input is taken by the page (a bad archive is refused with a message).
        await reach('library-upload-click', async () => {
          await chapter.setInputFiles({
            name: 'chapter-1.cbz',
            mimeType: 'application/zip',
            buffer: Buffer.from('abcd'),
          });
          await page.waitForTimeout(1500);
          const cell = await page.locator('.rv-file').first().locator('xpath=..').innerText();
          log(`after setInputFiles: ${cell.replace(/\s+/g, ' ').slice(0, 200)}`);
          await page.screenshot({ path: resolve(OUT, 'library-file-set-light-desktop.png') });
          shots++;
        });
      }
      await c.close();
    }
  }

  // ---- Lounge themed pages, desktop: Lounge Dark and Lounge Light (the account's main theme).
  for (const theme of ['lounge-dark', 'lounge-light'] as const) {
    const prefs: Prefs = { theme, zoom: 100 };
    await setPrefs(prefs);
    const c = await context(browser, octx, DESKTOP, prefs);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);

    await reach(`tool-hash-${theme}`, async () => {
      await page.goto('/channels/@me');
      await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
      await openTool(page, 'hashes', page.getByLabel('File', { exact: false }).first());
      await check(page, page.locator('input[type=file]').first(), `tool-hash-${theme}`);
      // Click through: a file hashes.
      await page
        .locator('input[type=file]')
        .first()
        .setInputFiles({
          name: 'a.txt',
          mimeType: 'text/plain',
          buffer: Buffer.from('hello'),
        });
      await page.waitForTimeout(500);
      await page.screenshot({ path: resolve(OUT, `tool-hash-file-set-${theme}.png`) });
      shots++;
    });

    await reach(`tool-notes-${theme}`, async () => {
      await page.goto('/channels/@me');
      await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
      await openTool(page, 'private notes', page.locator('input[type=file]').first());
      await check(page, page.locator('input[type=file]').first(), `tool-notes-${theme}`);
    });

    await reach(`tool-audio-${theme}`, async () => {
      await page.goto('/channels/@me');
      await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
      await openTool(page, 'audio trimmer', page.locator('input[type=file]').first());
      await check(page, page.locator('input[type=file]').first(), `tool-audio-${theme}`);
    });

    await reach(`settings-soundboard-${theme}`, async () => {
      await page.goto(`/settings/server/${guild.id}/emoji`);
      await page.getByRole('tab', { name: /soundboard/i }).click();
      await page.getByRole('button', { name: 'Upload a sound' }).click();
      await check(page, page.locator('#ss-sound-file'), `settings-soundboard-${theme}`);
    });

    await reach(`admin-entrance-${theme}`, async () => {
      await page.goto('/admin/entrance');
      await check(page, page.locator('input[type=file]').first(), `admin-entrance-${theme}`, true);
    });

    await reach(`developers-media-${theme}`, async () => {
      await page.goto('/developers');
      await check(page, page.locator('input[type=file]:visible').first(), `developers-${theme}`);
    });

    await c.close();
  }

  // ---- A page outside a theme root keeps the --sr-* fallbacks: the public entrance has no file input, so the
  // only check is that the rule is in the stylesheet the public pages load (the build checks assert the size).
  log(`SUMMARY probed=${probed} shots=${shots}`);
  await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { library: 'sideroom' } });
  await setStudySurfacesWith(octx.request, false);
  await octx.close();
});
