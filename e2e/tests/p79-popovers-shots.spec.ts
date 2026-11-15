import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { deflateSync } from 'node:zlib';
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
 * SEC-AUD-316 (wave 79; D-818), on the real stack: the Lounge's popovers carry their own width limits, and the
 * sticker grid, the GIF grids and the voice popovers are whole on a phone.
 *  - SR_P79_BASE=1: Pins and Saved items at 375x812 with `header.css` / `features.css` held back and the
 *    `lounge.css` rule removed: measures and asserts the fault (the panel is wider than its slot).
 *  - SR_P79_POPOVERS=1: part 1 (Pins, Saved items) and part 2 (sticker grid, GIF categories, GIF results), both
 *    sizes, three themes where the plan says so.
 *  - SR_P79_VOICE=1: part 3, a real one-member (plus one) call with Chromium's fake microphone (the server runs
 *    with SR_VOICE__ENABLED=true), the soundboard, the privacy codes, a tile's volume, the two device menus.
 * Local only; shots and `measurements.txt` go to SR_P79_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P79_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-popovers';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };
const BASE = process.env.SR_P79_BASE === '1';
const POPOVERS = process.env.SR_P79_POPOVERS === '1';
const VOICE = process.env.SR_P79_VOICE === '1';
const VOICE_THEMES = (process.env.SR_P79_VOICE_THEMES ?? 'lounge-dark,lounge-light').split(',');

test.skip(!(BASE || POPOVERS || VOICE), 'local visual check only');
test.setTimeout(1_800_000);

const note = (text: string) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(text);
};

// ---- A solid-colour PNG made here (stickers and the faked GIF source) -----------------------------------------

function crc32(buf: Buffer): number {
  let c = ~0;
  for (const b of buf) {
    c ^= b;
    for (let k = 0; k < 8; k++) c = c & 1 ? (c >>> 1) ^ 0xedb88320 : c >>> 1;
  }
  return ~c >>> 0;
}
function chunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
/** A w x h PNG: a diagonal gradient of one hue, so a tile is not a flat square. */
function png(w: number, h: number, hue: number): Buffer {
  const rows: Buffer[] = [];
  for (let y = 0; y < h; y++) {
    const row = Buffer.alloc(1 + w * 3);
    for (let x = 0; x < w; x++) {
      const t = (x + y) / (w + h);
      row[1 + x * 3] = Math.round(60 + 160 * t * (hue % 3 === 0 ? 1 : 0.4));
      row[2 + x * 3] = Math.round(60 + 160 * t * (hue % 3 === 1 ? 1 : 0.4));
      row[3 + x * 3] = Math.round(60 + 160 * t * (hue % 3 === 2 ? 1 : 0.4));
    }
    rows.push(row);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(Buffer.concat(rows))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}
const dataUri = (b: Buffer) => `data:image/png;base64,${b.toString('base64')}`;

// ---- World ----------------------------------------------------------------------------------------------------

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
  extra: { voice?: boolean } = {},
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    ...(extra.voice ? { permissions: ['microphone'] } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const wrap = (page: Page) => page.locator('.lng-composer-wrap').last();

async function setTheme(a: BrowserContext['request'], theme: string) {
  await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
}

/** The theme the page is really wearing, read from the room's root (and the document as a second place). */
async function themeOf(page: Page): Promise<string> {
  return page.evaluate(
    () =>
      document.querySelector<HTMLElement>('.lng-root')?.dataset.theme ??
      document.documentElement.dataset.theme ??
      '?',
  );
}

async function world(browser: Browser) {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Popover Room' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const voiceChannel =
    guild.channels.find((c) => c.type === 2)?.id ??
    (await apiPost<{ id: string }>(a, `/guilds/${guild.id}/channels`, { name: 'Calls', type: 2 }))
      .id;
  // A pinned message with a long line, so Pins has something to wrap.
  const pinned = await apiPost<{ id: string }>(a, `/channels/${channel}/messages`, {
    content:
      'A pinned message with a fairly long line of text so that it has to wrap inside the narrow panel',
  });
  await apiPut(a, `/channels/${channel}/pins/${pinned.id}`).catch(() => undefined);
  await apiPost(a, `/channels/${channel}/messages`, { content: 'Something to look at' });
  return { octx, a, guild, channel, voiceChannel };
}

// ---- Measurements ---------------------------------------------------------------------------------------------

interface PopMeasure {
  panelLeft: number;
  panelRight: number;
  panelTop: number;
  panelBottom: number;
  scrollW: number;
  clientW: number;
  cut: string[];
}

/** The open popover's box against the screen and anything inside it that sticks out past its sides. The kit's tab
 *  strip (a `role=tablist` that scrolls on its own, kit's item this wave) is left out of the cut list. */
async function measurePopover(dialog: Locator): Promise<PopMeasure> {
  return dialog.evaluate((root) => {
    const r = root.getBoundingClientRect();
    const cut: string[] = [];
    const inTabs = (el: Element) => !!el.closest('[role="tablist"]');
    for (const el of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
      if (inTabs(el)) continue;
      const b = el.getBoundingClientRect();
      if (b.width === 0 || b.height === 0) continue;
      if (b.right > r.right + 0.5 || b.left < r.left - 0.5) {
        const cls = typeof el.className === 'string' ? el.className.split(' ')[0] : '';
        cut.push(`${el.tagName.toLowerCase()}.${cls} ${b.left.toFixed(0)}..${b.right.toFixed(0)}`);
      }
    }
    let scrollW = root.scrollWidth;
    let clientW = root.clientWidth;
    for (const el of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
      if (inTabs(el)) continue;
      const oy = getComputedStyle(el).overflowY;
      if ((oy === 'auto' || oy === 'scroll') && el.scrollWidth > el.clientWidth + 0.5) {
        scrollW = el.scrollWidth;
        clientW = el.clientWidth;
      }
    }
    return {
      panelLeft: r.left,
      panelRight: r.right,
      panelTop: r.top,
      panelBottom: r.bottom,
      scrollW,
      clientW,
      cut: cut.slice(0, 6),
    };
  });
}

function popLine(tag: string, m: PopMeasure, vp: number) {
  const bad =
    m.panelLeft < -0.5 ||
    m.panelRight > vp + 0.5 ||
    m.scrollW > m.clientW + 0.5 ||
    m.cut.length > 0;
  note(
    `${tag}: panel ${m.panelLeft.toFixed(1)}..${m.panelRight.toFixed(1)} x ${m.panelTop.toFixed(1)}..${m.panelBottom.toFixed(1)} ` +
      `scroll ${m.scrollW}/${m.clientW} cut [${m.cut.join('; ')}] => ${bad ? 'CUT' : 'whole'}`,
  );
  return bad;
}

interface Cells {
  cells: number;
  clientWidth: number;
  scrollWidth: number;
  clientLeft: number;
  clientRight: number;
  minLeft: number;
  maxRight: number;
  cut: number;
  pickerLeft: number;
  pickerRight: number;
  panelLeft: number;
  panelRight: number;
  cellWidth: number;
}

/** Every cell of a grid inside the scroll area's client box, no sideways scroll, the picker inside its panel. */
async function cells(dialog: Locator, selector: string): Promise<Cells> {
  return dialog.evaluate((root, sel) => {
    const scroll = root.querySelector<HTMLElement>('.lng-emoji-scroll')!;
    const sr = scroll.getBoundingClientRect();
    const clientLeft = sr.left + scroll.clientLeft;
    const clientRight = clientLeft + scroll.clientWidth;
    const rects = Array.from(root.querySelectorAll<HTMLElement>(sel)).map((b) =>
      b.getBoundingClientRect(),
    );
    const panel = root.getBoundingClientRect();
    const picker = root.querySelector<HTMLElement>('.lng-emoji') ?? (root as HTMLElement);
    const pr = picker.getBoundingClientRect();
    return {
      cells: rects.length,
      clientWidth: scroll.clientWidth,
      scrollWidth: scroll.scrollWidth,
      clientLeft,
      clientRight,
      minLeft: Math.min(...rects.map((r) => r.left)),
      maxRight: Math.max(...rects.map((r) => r.right)),
      cut: rects.filter((r) => r.right > clientRight + 0.5 || r.left < clientLeft - 0.5).length,
      pickerLeft: pr.left,
      pickerRight: pr.right,
      panelLeft: panel.left,
      panelRight: panel.right,
      cellWidth: rects[0]?.width ?? 0,
    };
  }, selector);
}

async function expectWholeCells(
  page: Page,
  dialog: Locator,
  selector: string,
  where: string,
  vp: number,
) {
  const g = await cells(dialog, selector);
  note(
    `${where}: ${g.cells} cells (${g.cellWidth.toFixed(1)} wide) in ${g.clientLeft.toFixed(1)}..${g.clientRight.toFixed(1)}, ` +
      `min ${g.minLeft.toFixed(1)} max ${g.maxRight.toFixed(1)} cut ${g.cut} scroll ${g.scrollWidth}/${g.clientWidth} ` +
      `picker ${g.pickerLeft.toFixed(1)}..${g.pickerRight.toFixed(1)} panel ${g.panelLeft.toFixed(1)}..${g.panelRight.toFixed(1)}`,
  );
  expect(g.cells, `${where} has cells`).toBeGreaterThan(0);
  expect(g.cut, `${where}: cells outside the scroll area's client box`).toBe(0);
  expect(g.scrollWidth, `${where} no sideways scroll`).toBeLessThanOrEqual(g.clientWidth);
  expect(g.pickerRight, `${where} picker inside its panel`).toBeLessThanOrEqual(g.panelRight + 0.5);
  expect(g.panelLeft, `${where} panel on screen`).toBeGreaterThanOrEqual(0);
  expect(g.panelRight, `${where} panel on screen`).toBeLessThanOrEqual(vp);
  const over = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(over, `${where} page overflow`).toBeLessThanOrEqual(0);
  return g;
}

async function openPicker(page: Page, opener: string, panel: string) {
  await wrap(page).getByRole('button', { name: opener }).first().click();
  const dialog = page.getByRole('dialog', { name: panel }).last();
  await expect(dialog).toBeVisible();
  await page.waitForTimeout(600);
  return dialog;
}

// ---- Part 1 base: the fault, with header.css / features.css held back -----------------------------------------

test('base: Pins and Saved items are wider than their slot on a phone', async ({ browser }) => {
  test.skip(!BASE, 'base measurement only (SR_P79_BASE=1)');
  const { octx, a, guild, channel } = await world(browser);
  await setTheme(a, 'lounge-dark');
  const c = await context(browser, octx, PHONE, 'lounge-dark');
  const page = await c.newPage();
  page.setDefaultTimeout(15_000);
  await page.goto(`/channels/${guild.id}/${channel}`);
  await expect(wrap(page).locator('textarea')).toBeVisible();
  await page.waitForTimeout(800);
  let faults = 0;
  for (const [name, label] of [
    ['pins', 'Pinned Messages'],
    ['saved', 'Saved items'],
  ] as const) {
    await page.getByRole('button', { name: label, exact: true }).click();
    await page.waitForTimeout(700);
    const dialog = page.getByRole('dialog').last();
    const m = await measurePopover(dialog);
    const box = await dialog.locator('.lng-pins-pop, .lng-saved-pop').first().boundingBox();
    note(`BASE ${name}: popover box width ${box?.width.toFixed(1)}`);
    if (popLine(`BASE ${name}`, m, PHONE.width)) faults++;
    await page.screenshot({ path: resolve(OUT, `base-${name}-phone.png`) });
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);
  }
  await c.close();
  await octx.close();
  expect(faults, 'base: Pins or Saved items is cut on the phone').toBeGreaterThan(0);
});

// ---- Part 1 and 2 ---------------------------------------------------------------------------------------------

const GIF_COLORS = [0, 1, 2, 3, 4, 5, 6, 7];

/** The GIF source, faked: config, categories, a page of results, trending; the images are routed PNGs. */
async function fakeGifs(c: BrowserContext) {
  const img = png(160, 120, 1);
  await c.route('**/fake-gif/**', (route) =>
    route.fulfill({ status: 200, contentType: 'image/png', body: img }),
  );
  await c.route('**/api/v1/gifs/**', (route) => {
    const u = new URL(route.request().url());
    const p = u.pathname;
    const json = (body: unknown) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
    if (p.endsWith('/gifs/config')) return json({ enabled: true });
    if (p.endsWith('/gifs/categories')) {
      return json({
        categories: ['Happy', 'Sad', 'Excited', 'Confused', 'Applause', 'Facepalm'].map((n, i) => ({
          name: n,
          search_term: n.toLowerCase(),
          image_url: `${ORIGIN}/fake-gif/cat-${i}.png`,
        })),
      });
    }
    // search and trending: eight results
    return json({
      results: GIF_COLORS.map((i) => ({
        id: `g${i}`,
        title: `Fake gif ${i}`,
        url: `${ORIGIN}/fake-gif/g${i}.png`,
        preview_url: `${ORIGIN}/fake-gif/g${i}.png`,
        width: 160,
        height: 60 + (i % 3) * 30,
      })),
      next: null,
    });
  });
}

test('the popovers, the sticker grid and the GIF grids on both sizes', async ({ browser }) => {
  test.skip(!POPOVERS, 'popover run only (SR_P79_POPOVERS=1)');
  const { octx, a, guild, channel } = await world(browser);
  // Stickers: nine, added through the API (a 96x96 PNG each).
  for (let i = 0; i < 9; i++) {
    await apiPost(a, `/guilds/${guild.id}/stickers`, {
      name: `sticker${i}`,
      tags: 'smile',
      file: dataUri(png(96, 96, i)),
    });
  }
  const msgs = await a.get(`/api/v1/guilds/${guild.id}/stickers`);
  note(`stickers added: ${((await msgs.json()) as unknown[]).length}`);

  for (const theme of THEMES) {
    await setTheme(a, theme);
    for (const [vp, size] of [
      ['phone', PHONE],
      ['desktop', DESKTOP],
    ] as const) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      await fakeGifs(c);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(wrap(page).locator('textarea')).toBeVisible();
      await page.waitForTimeout(800);
      note(`${tag}: room wears data-theme=${await themeOf(page)}`);

      // Part 1: Pins and Saved items (phone only; Dark and Light are the shots, Editorial is measured).
      if (vp === 'phone') {
        for (const [name, label] of [
          ['pins', 'Pinned Messages'],
          ['saved', 'Saved items'],
        ] as const) {
          await page.getByRole('button', { name: label, exact: true }).click();
          await page.waitForTimeout(700);
          const dialog = page.getByRole('dialog').last();
          const m = await measurePopover(dialog);
          const bad = popLine(`${name}-${tag}`, m, size.width);
          expect(bad, `${name} ${tag} is whole`).toBe(false);
          if (theme !== 'lounge-editorial') {
            await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
          }
          await page.keyboard.press('Escape');
          await page.waitForTimeout(300);
        }
      }

      // Part 2a: the sticker grid.
      let dialog = await openPicker(page, 'Open sticker picker', 'Sticker picker');
      await expectWholeCells(page, dialog, '.lng-sticker-tile', `sticker ${tag}`, size.width);
      if (theme !== 'lounge-editorial') {
        await page.screenshot({ path: resolve(OUT, `stickers-${tag}.png`) });
      }
      if (theme === 'lounge-dark') {
        // Pick one: it is sent as a message.
        await dialog.locator('.lng-sticker-tile').first().click();
        await expect
          .poll(
            async () => {
              const r = await a.get(`/api/v1/channels/${channel}/messages?limit=3`);
              return JSON.stringify(await r.json()).includes('sticker_items');
            },
            { message: `${tag} a sticker was sent` },
          )
          .toBe(true);
        note(`${tag}: picked a sticker, the message carries sticker_items`);
      }
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);

      // Part 2b: the GIF picker, categories then results.
      dialog = await openPicker(page, 'Open GIF picker', 'GIF picker');
      await expect(dialog.locator('.lng-gif-cat').first()).toBeVisible();
      await page.waitForTimeout(500);
      await expectWholeCells(page, dialog, '.lng-gif-cat', `gif-cats ${tag}`, size.width);
      if (theme !== 'lounge-editorial') {
        await page.screenshot({ path: resolve(OUT, `gif-cats-${tag}.png`) });
      }
      await dialog.locator('.lng-gif-cat').first().click();
      await expect(dialog.locator('.lng-gif-cell').first()).toBeVisible();
      await page.waitForTimeout(700);
      await expectWholeCells(page, dialog, '.lng-gif-cell', `gif-results ${tag}`, size.width);
      if (theme !== 'lounge-editorial') {
        await page.screenshot({ path: resolve(OUT, `gif-results-${tag}.png`) });
      }
      await c.close();
    }
  }
  await octx.close();
});

// ---- Part 3: a real call --------------------------------------------------------------------------------------

test('the voice popovers and the device menus on a phone', async ({ browser }) => {
  test.skip(!VOICE, 'voice run only (SR_P79_VOICE=1, server with SR_VOICE__ENABLED=true)');
  const { octx, a, guild, channel, voiceChannel } = await world(browser);
  const size = PHONE;
  const invite = await apiPost<{ code: string }>(a, `/channels/${channel}/invites`, {});
  // A second member in the call, so that a tile with a volume control exists.
  const bobCtx = await browser.newContext({ baseURL: ORIGIN, permissions: ['microphone'] });
  const bob = await registerUser(bobCtx.request, 'bobby');
  await loginApi(bobCtx.request, bob);
  await apiPost(bobCtx.request, `/invites/${invite.code}`, {});
  const bobPage = await bobCtx.newPage();
  const bobTheme = { theme: 'lounge-dark', zoom: 100 };
  await bobPage.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, bobTheme);

  for (const theme of VOICE_THEMES) {
    await setTheme(a, theme);
    const c = await context(browser, octx, size, theme, { voice: true });
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    const logs: string[] = [];
    page.on('console', (m) => {
      if (/voice|sfu|ice|webrtc|error/i.test(m.text())) logs.push(m.text().slice(0, 160));
    });
    await page.goto(`/channels/${guild.id}/${voiceChannel}`);
    await page.waitForTimeout(1500);
    note(`voice ${theme}: room wears data-theme=${await themeOf(page)}`);
    await page.screenshot({ path: resolve(OUT, `voice-join-${theme}.png`) });

    const join = page.getByRole('button', { name: 'Join Voice' }).first();
    await expect(join).toBeVisible();
    await join.click();
    const connected = await page
      .locator('.vc-state[data-ok]')
      .first()
      .waitFor({ state: 'attached', timeout: 60_000 })
      .then(() => true)
      .catch(() => false);
    note(`voice ${theme}: connected ${connected}`);
    if (!connected) {
      const state = await page
        .locator('.vc-state')
        .first()
        .innerText()
        .catch(() => '(no .vc-state)');
      note(`voice ${theme}: the page shows "${state}"; console: ${logs.slice(-8).join(' | ')}`);
      await page.screenshot({ path: resolve(OUT, `voice-not-connected-${theme}.png`) });
      await c.close();
      break;
    }

    if (theme === VOICE_THEMES[0]) {
      await bobPage.goto(`/channels/${guild.id}/${voiceChannel}`);
      await bobPage.getByRole('button', { name: 'Join Voice' }).first().click();
      await bobPage
        .locator('.vc-state[data-ok]')
        .first()
        .waitFor({ state: 'attached', timeout: 60_000 });
    }
    await page.waitForTimeout(1500);
    await page.screenshot({ path: resolve(OUT, `voice-call-${theme}.png`) });
    const names = await page.getByRole('button').evaluateAll((els) =>
      els
        .filter((e) => (e as HTMLElement).offsetParent !== null)
        .map((e) => e.getAttribute('aria-label') ?? e.textContent?.trim() ?? '')
        .filter(Boolean)
        .slice(0, 60),
    );
    note(`voice ${theme}: visible buttons: ${names.join(' | ')}`);

    const cases: [string, Locator][] = [
      ['volume', page.getByRole('button', { name: /^Volume for / })],
      ['privacy-codes', page.locator('.vc-e2ee-shield')],
      ['mic-options', page.getByRole('button', { name: 'Microphone options', exact: true })],
      ['camera-options', page.getByRole('button', { name: 'Camera options', exact: true })],
      ['soundboard', page.getByRole('button', { name: 'Soundboard', exact: true })],
      ['device-input', page.getByRole('button', { name: 'Input device and volume', exact: true })],
      [
        'device-output',
        page.getByRole('button', { name: 'Output device and volume', exact: true }),
      ],
    ];
    for (const [name, trigger] of cases) {
      const tag = `voice-${name}-${theme}`;
      // The first match can be a copy off the phone's screen (the channel list is beside the call): take the one
      // whose box lies inside the viewport, in the call view first, then in the channel list.
      const onScreen = async (): Promise<Locator | null> => {
        const n = await trigger.count();
        for (let i = 0; i < n; i++) {
          const el = trigger.nth(i);
          const bb = await el.boundingBox().catch(() => null);
          if (
            bb &&
            bb.x >= 0 &&
            bb.x + bb.width <= size.width &&
            bb.y >= 0 &&
            bb.y + bb.height <= size.height
          )
            return el;
        }
        return null;
      };
      let t = await onScreen();
      let where = 'call view';
      if (!t) {
        const back = page.getByRole('button', { name: 'Back to channels' }).first();
        if (await back.isVisible().catch(() => false)) {
          await back.click().catch(() => undefined);
          await page.waitForTimeout(800);
          t = await onScreen();
          where = 'channel list';
          if (!t) {
            await page.screenshot({ path: resolve(OUT, `voice-list-${name}-${theme}.png`) });
          }
        }
      }
      if (!t) {
        note(`${tag}: NOT REACHED (no copy of the trigger on the phone's screen)`);
        continue;
      }
      const clicked = await t
        .click({ timeout: 5000 })
        .then(() => true)
        .catch(() => false);
      if (!clicked) {
        note(`${tag}: NOT REACHED (trigger could not be pressed)`);
        continue;
      }
      note(`${tag}: opened from the ${where}`);
      await page.waitForTimeout(800);
      const dialog = page.getByRole('dialog').last();
      if (!(await dialog.isVisible().catch(() => false))) {
        note(`${tag}: NOT REACHED (no panel opened)`);
        continue;
      }
      const m = await measurePopover(dialog);
      popLine(tag, m, size.width);
      await page.screenshot({ path: resolve(OUT, `${tag}.png`) });
      if (name === 'soundboard') {
        const sound = dialog.locator('button').filter({ hasText: /\S/ }).nth(2);
        if (await sound.isVisible().catch(() => false)) {
          await sound.click({ timeout: 3000 }).catch(() => undefined);
          note(`${tag}: pressed a control inside the soundboard`);
        }
      }
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
      if (where === 'channel list') {
        await page
          .getByRole('button', { name: /voice channel, connected/ })
          .first()
          .click()
          .catch(() => undefined);
        await page.waitForTimeout(800);
      }
    }
    const leave = page.getByRole('button', { name: 'Disconnect' }).first();
    if (await leave.isVisible().catch(() => false)) await leave.click().catch(() => undefined);
    await page.waitForTimeout(800);
    await c.close();
  }
  await bobCtx.close();
  await octx.close();
});
