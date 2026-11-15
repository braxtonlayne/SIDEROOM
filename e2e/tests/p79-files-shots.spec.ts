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
  test,
} from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-311 (wave 79; D-813, D-814), on the real stack: the eight visible native file inputs wave 78 could not
 * reach (Admin > Cinema title artwork and subtitles, Admin > Live TV channel logo and guide import, Developers >
 * Media (emojis and rich-presence assets), an app command's attachment option, the Rings composer) are probed and
 * shot. For each: `getComputedStyle(input, '::file-selector-button')` against a probe element carrying the kit's
 * secondary tokens, the input inside its panel, the label not touching the button, the text readable. Also: the
 * Developers' corners (a card's radius against a kit button's) and the Admin > Entrance label gap.
 * Lounge Dark and Lounge Light (the account's main theme, set by PATCH and localStorage), 1280x800 and 375x812.
 * Local only (SR_P79_FILES=1); shots go to SR_P79_FILES_OUT. SR_P79_BASE=1: the base case (the two CSS changes held
 * back): the probe and shots, and the faults are logged instead of asserted.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P79_FILES_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-files';
const BASE = process.env.SR_P79_BASE === '1';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const THEMES = ['lounge-dark', 'lounge-light'] as const;

test.skip(process.env.SR_P79_FILES !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number };

const log = (line: string) => console.log(`REPORT ${line}`);
let shots = 0;
let probed = 0;

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
  fakes?: (c: BrowserContext) => Promise<void>,
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
  if (fakes) await fakes(c);
  return c;
}

/** The picker button's computed look and the tokens it should have, plus the placement checks. */
async function look(input: Locator) {
  return input.evaluate((el) => {
    const pseudo = getComputedStyle(el, '::file-selector-button');
    const host = el.parentElement ?? el;
    const probe = document.createElement('span');
    probe.style.cssText =
      'position:absolute;visibility:hidden;background:var(--ui-bg-hover, var(--sr-bg-2));color:var(--ui-text, var(--sr-text));border:1px solid var(--ui-border, var(--sr-line));border-radius:var(--ui-radius, var(--sr-radius))';
    host.appendChild(probe);
    const want = getComputedStyle(probe);
    const panel =
      el.closest('.ss-card, .dev-card, [role=dialog], .lng-appform, form, main') ??
      el.parentElement!;
    const pr = panel.getBoundingClientRect();
    const ir = el.getBoundingClientRect();
    // The label text beside the input: a gap of at least 4px on the same row, or the input sits below it.
    const label = el.closest('label');
    let labelGap: number | null = null;
    if (label) {
      const span = label.querySelector('span');
      if (span && span !== el) {
        const sr = span.getBoundingClientRect();
        const sameRow = ir.top < sr.bottom - 2 && ir.bottom > sr.top + 2;
        labelGap = sameRow ? Math.round(ir.left - sr.right) : 999;
      }
    }
    // Contrast of the input's text colour against the nearest opaque background.
    const parse = (s: string) => {
      const m = /rgba?\(([^)]+)\)/.exec(s);
      if (!m) return null;
      const p = m[1]!.split(/[ ,/]+/).map(Number);
      return { r: p[0]!, g: p[1]!, b: p[2]!, a: p[3] ?? 1 };
    };
    const lum = (c: { r: number; g: number; b: number }) => {
      const f = (v: number) => {
        const x = v / 255;
        return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
      };
      return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
    };
    let bgc: { r: number; g: number; b: number } = { r: 255, g: 255, b: 255 };
    for (let n: Element | null = el; n; n = n.parentElement) {
      const c = parse(getComputedStyle(n).backgroundColor);
      if (c && c.a > 0.95) {
        bgc = c;
        break;
      }
    }
    const fg = parse(getComputedStyle(el).color) ?? { r: 0, g: 0, b: 0, a: 1 };
    const [a, b] = [lum(fg), lum(bgc)].sort((x, y) => y - x);
    const contrast = Math.round(((a! + 0.05) / (b! + 0.05)) * 10) / 10;
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
      inputRight: Math.round(ir.right),
      panelRight: Math.round(pr.right),
      inputWidth: Math.round(ir.width),
      labelGap,
      contrast,
      scrollOver: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    };
    probe.remove();
    return out;
  });
}

/** Probe one visible input and shoot it. */
async function check(page: Page, input: Locator, name: string) {
  await input.scrollIntoViewIfNeeded();
  await expect(input).toBeVisible();
  const l = await look(input);
  probed++;
  log(`${name} ${JSON.stringify(l)}`);
  expect.soft(l.bg, `${name}: background`).toBe(l.wantBg);
  expect.soft(l.color, `${name}: colour`).toBe(l.wantColor);
  expect.soft(l.radius, `${name}: radius`).toBe(l.wantRadius);
  expect.soft(l.border, `${name}: border colour`).toBe(l.wantBorder);
  expect.soft(l.font, `${name}: font`).toBe(l.hostFont);
  expect.soft(l.cursor, `${name}: cursor`).toBe('pointer');
  expect.soft(l.inputRight, `${name}: inside its panel`).toBeLessThanOrEqual(l.panelRight + 1);
  expect.soft(l.contrast, `${name}: text contrast`).toBeGreaterThanOrEqual(4.5);
  if (l.labelGap !== null && !BASE) {
    expect.soft(l.labelGap, `${name}: label and button do not touch`).toBeGreaterThanOrEqual(4);
  }
  if (l.scrollOver > 0) log(`${name} SCROLLS-SIDEWAYS by ${l.scrollOver}`);
  await page.waitForTimeout(250);
  const vp = page.viewportSize()!;
  if (vp.width < 500) {
    await input.evaluate((el) => el.scrollIntoView({ block: 'center', inline: 'start' }));
    await page.waitForTimeout(200);
    await page.screenshot({ path: resolve(OUT, `${name}.png`) });
  } else {
    const box = (await input.boundingBox())!;
    const x = Math.max(0, box.x - 280);
    const y = Math.max(0, box.y - 110);
    await page.screenshot({
      path: resolve(OUT, `${name}.png`),
      clip: {
        x,
        y,
        width: Math.min(vp.width - x, 760),
        height: Math.min(vp.height - y, 240),
      },
    });
  }
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

const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64',
);

test('the eight file inputs, the Developers corners, the Entrance label', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  await setStudySurfacesWith(o, true);
  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });

  // ---- the world: a published title with a video asset, two TV channels, an application, a server
  const stamp = Date.now().toString(36);
  const titleName = `Booth Film ${stamp}`;
  let titleId: string | null = null;
  try {
    titleId = (
      await apiPost<{ id: string }>(o, '/admin/cinema/titles', {
        kind: 'movie',
        title: titleName,
        year: 2001,
        runtime_min: 100,
        content_rating: 'PG',
        synopsis: 'A fixture film for the file input check.',
        genres: ['Drama'],
        languages: ['en'],
        subtitle_languages: ['en'],
        cast: [],
        directors: [],
        writers: [],
        studios: [],
        crew: [],
        alt_titles: [],
        tags: [],
        age_restricted: false,
        external_ids: {},
      })
    ).id;
    await apiPut(o, `/admin/cinema/titles/${titleId}/asset`, { asset_id: '4242' });
  } catch (e) {
    log(`SEED title failed: ${String(e).slice(0, 200)}`);
  }
  const channels: Record<string, string> = {};
  for (const [kind, number, name] of [
    ['manual', 71, 'Booth Manual TV'],
    ['imported_guide', 72, 'Booth Guide TV'],
  ] as const) {
    try {
      channels[kind] = (
        await apiPost<{ id: string }>(o, '/admin/tv/channels', {
          number,
          name,
          kind,
          timezone: 'UTC',
          enabled: true,
        })
      ).id;
    } catch (e) {
      log(`SEED channel ${kind} failed: ${String(e).slice(0, 200)}`);
    }
  }
  let appId: string | null = null;
  try {
    appId = (
      await apiPost<{ application: { id: string } }>(o, '/applications', {
        name: `Booth app ${stamp}`,
      })
    ).application.id;
  } catch (e) {
    log(`SEED application failed: ${String(e).slice(0, 200)}`);
  }
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'File Booth' },
  );
  const textChannel = guild.channels.find((c) => c.type === 0)!.id;
  log(`world title=${titleId} channels=${JSON.stringify(channels)} app=${appId} guild=${guild.id}`);

  const fakeCommands = async (c: BrowserContext) => {
    await c.route(/\/api\/v1\/channels\/[^/]+\/application-commands/, (r) =>
      r.fulfill({
        status: 200,
        contentType: 'application/json',
        json: {
          applications: [{ id: '9001', name: 'Snap Bot', icon: null, bot_id: '9002' }],
          application_commands: [
            {
              id: '9003',
              application_id: '9001',
              type: 1,
              name: 'snapshot',
              description: 'Send a picture to the bot',
              options: [
                { type: 11, name: 'picture', description: 'The picture to send', required: true },
              ],
            },
          ],
        },
      }),
    );
  };

  for (const theme of THEMES) {
    const prefs: Prefs = { theme, zoom: 100 };
    await setPrefs(prefs);
    for (const [sname, size] of Object.entries(SIZES)) {
      const tag = `${theme === 'lounge-dark' ? 'dark' : 'light'}-${sname}`;
      const c = await context(browser, octx, size, prefs, fakeCommands);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const first = sname === 'desktop' && theme === 'lounge-dark';

      // 1 + 2: Admin > Cinema catalog > the title: artwork, then subtitles.
      await reach(`admin-title-${tag}`, async () => {
        await page.goto('/admin/cinema-catalog');
        await page.getByRole('button', { name: `Edit ${titleName}` }).click();
        await check(page, page.locator('input.adm-file').first(), `admin-artwork-${tag}`);
        await page
          .getByRole('button', { name: /Show the subtitles of/ })
          .first()
          .click();
        const sub = page.locator('form.adm-sub-add input[type=file]');
        await check(page, sub, `admin-subtitles-${tag}`);
        if (first) {
          await sub.setInputFiles({
            name: 'en.vtt',
            mimeType: 'text/vtt',
            buffer: Buffer.from('WEBVTT\n\n00:00.000 --> 00:02.000\nHello\n'),
          });
          await page.waitForTimeout(600);
          log(
            `after setInputFiles (subtitles): ${(await page.locator('form.adm-sub-add').innerText()).replace(/\s+/g, ' ').slice(0, 200)}`,
          );
          await page.screenshot({ path: resolve(OUT, `admin-subtitles-file-set-${tag}.png`) });
          shots++;
        }
      });

      // 3: Admin > Live TV > a manual channel: the logo.
      await reach(`admin-tvlogo-${tag}`, async () => {
        await page.goto('/admin/cinema-live-tv');
        await page.getByRole('button', { name: 'Open Booth Manual TV' }).click();
        const logo = page.locator('input.adm-file[type=file]').first();
        await check(page, logo, `admin-tvlogo-${tag}`);
        if (first) {
          await logo.setInputFiles({ name: 'logo.png', mimeType: 'image/png', buffer: PNG });
          await page.waitForTimeout(1200);
          log(
            `after setInputFiles (logo): ${(await page.locator('main').innerText()).replace(/\s+/g, ' ').slice(0, 240)}`,
          );
          await page.screenshot({ path: resolve(OUT, `admin-tvlogo-file-set-${tag}.png`) });
          shots++;
        }
      });

      // 4: Admin > Live TV > an imported-guide channel > Guide: the import.
      await reach(`admin-guide-${tag}`, async () => {
        await page.goto('/admin/cinema-live-tv');
        await page.getByRole('button', { name: 'Open Booth Guide TV' }).click();
        await page.getByRole('tab', { name: 'Guide' }).click();
        await check(page, page.locator('label.ss-field input[type=file]'), `admin-guide-${tag}`);
      });

      // 5 + 6: Developers > an application > Emojis, Rich Presence; and the corners.
      if (appId) {
        await reach(`developers-emojis-${tag}`, async () => {
          await page.goto(`/developers/applications/${appId}?tab=emojis`);
          await check(
            page,
            page.locator('label.dev-file input[type=file]').first(),
            `developers-emojis-${tag}`,
          );
          if (sname === 'desktop') {
            const r = await page.evaluate(() => {
              const card = document.querySelector('.dev-card');
              const btn = document.querySelector('.dev-card button');
              const probe = (el: Element | null) =>
                el ? getComputedStyle(el).borderTopLeftRadius : null;
              return {
                card: probe(card),
                button: probe(btn),
                ui: getComputedStyle(document.body).getPropertyValue('--ui-radius'),
              };
            });
            log(`developers radius ${tag} ${JSON.stringify(r)}`);
            if (!BASE)
              expect.soft(r.card, 'developers card radius is the kit radius').toBe(r.button);
            await page.screenshot({ path: resolve(OUT, `developers-app-${tag}.png`) });
            shots++;
          }
        });
        await reach(`developers-assets-${tag}`, async () => {
          await page.goto(`/developers/applications/${appId}?tab=richpresence`);
          await check(
            page,
            page.locator('label.dev-file input[type=file]').first(),
            `developers-assets-${tag}`,
          );
        });
        if (sname === 'desktop') {
          await reach(`developers-home-${tag}`, async () => {
            await page.goto('/developers');
            await page.waitForTimeout(800);
            await page.screenshot({ path: resolve(OUT, `developers-home-${tag}.png`) });
            shots++;
          });
        }
      }

      // 7: an app command with an attachment option (the command list is faked).
      await reach(`command-form-${tag}`, async () => {
        await page.goto(`/channels/${guild.id}/${textChannel}`);
        const box = page.getByRole('textbox').last();
        await expect(box).toBeVisible({ timeout: 30_000 });
        await box.click();
        await page.keyboard.type('/snapshot');
        await page.waitForTimeout(800);
        await page.keyboard.press('Enter');
        await page.waitForTimeout(600);
        if ((await page.locator('.lng-appform-file').count()) === 0) {
          await page.keyboard.press('Enter');
          await page.waitForTimeout(600);
        }
        const file = page.locator('.lng-appform-file');
        await check(page, file, `command-form-${tag}`);
        if (first) {
          await file.setInputFiles({ name: 'pic.png', mimeType: 'image/png', buffer: PNG });
          await page.waitForTimeout(500);
          log(
            `after setInputFiles (command): ${(await page.locator('.lng-appform-field').first().innerText()).replace(/\s+/g, ' ').slice(0, 200)}`,
          );
          await page.screenshot({ path: resolve(OUT, `command-form-file-set-${tag}.png`) });
          shots++;
        }
      });

      // 8: the Rings composer, a photo ring.
      await reach(`ring-composer-${tag}`, async () => {
        await page.goto('/channels/@me');
        await page.getByRole('button', { name: 'Add a ring' }).click();
        await page.getByRole('radio', { name: 'Photo' }).check();
        await check(page, page.locator('[role=dialog] input[type=file]'), `ring-composer-${tag}`);
      });

      // The Entrance label (Admin > Entrance): the gap between the label text and the button.
      await reach(`admin-entrance-${tag}`, async () => {
        await page.goto('/admin/entrance');
        await check(page, page.locator('label.ss-field input[type=file]'), `admin-entrance-${tag}`);
      });

      await c.close();
    }
  }

  log(`SUMMARY probed=${probed} shots=${shots}`);
  await setStudySurfacesWith(o, false);
  await octx.close();
});
