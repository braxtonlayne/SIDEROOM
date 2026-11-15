import { mkdirSync, appendFileSync } from 'node:fs';
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

/**
 * SEC-AUD-306 (wave 78; D-806), on the real stack: every column of the emoji picker's grid is whole. The grid was
 * `repeat(7, 1fr)`, whose columns do not shrink below their button's content, so the last column was cut. The
 * measurement: every `.lng-emoji-btn`'s right edge is at or inside the right edge of `.lng-emoji-scroll`'s client box,
 * `.lng-emoji-scroll.scrollWidth <= clientWidth`. SR_P78_BASE=1 measures only and asserts the fault (run it with the
 * CSS held back). Local only (SR_P78_EMOJI=1); shots go to SR_P78_EMOJI_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P78_EMOJI_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-emoji';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const BASE = process.env.SR_P78_BASE === '1';
const POPOVERS = process.env.SR_P78_EMOJI_POPOVERS === '1';

test.skip(process.env.SR_P78_EMOJI !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
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
const note = (text: string) => {
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(text);
};

async function world(browser: Browser) {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Emoji Room' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const starter = await apiPost<{ id: string }>(a, `/channels/${channel}/messages`, {
    content: 'Something to react to',
  });
  await apiPut(
    a,
    `/channels/${channel}/messages/${starter.id}/reactions/${encodeURIComponent('\u{1F44D}')}/@me`,
  );
  const th = await apiPost<{ id: string }>(
    a,
    `/channels/${channel}/messages/${starter.id}/threads`,
    { name: 'Emoji thread' },
  );
  await apiPost(a, `/channels/${th.id}/messages`, { content: 'In the thread' });
  return { octx, a, guild, channel };
}

interface Grid {
  clientWidth: number;
  scrollWidth: number;
  clientLeft: number;
  clientRight: number;
  buttons: number;
  maxRight: number;
  minLeft: number;
  cell: number;
  cut: number;
  firstGridCols: number;
  panelLeft: number;
  panelRight: number;
  pickerLeft: number;
  pickerRight: number;
}

/** The numbers D-806 names, read from the open picker. */
async function grid(dialog: Locator): Promise<Grid> {
  return dialog.evaluate((root) => {
    const scroll = root.querySelector<HTMLElement>('.lng-emoji-scroll')!;
    const sr = scroll.getBoundingClientRect();
    const clientLeft = sr.left + scroll.clientLeft;
    const clientRight = clientLeft + scroll.clientWidth;
    const btns = Array.from(root.querySelectorAll<HTMLElement>('.lng-emoji-btn'));
    const rects = btns.map((b) => b.getBoundingClientRect());
    const firstGrid = root.querySelector<HTMLElement>('.lng-emoji-grid');
    const cols = firstGrid ? getComputedStyle(firstGrid).gridTemplateColumns.split(' ').length : 0;
    const panel = root.getBoundingClientRect();
    const picker = root.querySelector<HTMLElement>('.lng-emoji') ?? (root as HTMLElement);
    const pr = picker.getBoundingClientRect();
    return {
      clientWidth: scroll.clientWidth,
      scrollWidth: scroll.scrollWidth,
      clientLeft,
      clientRight,
      buttons: btns.length,
      maxRight: Math.max(...rects.map((r) => r.right)),
      minLeft: Math.min(...rects.map((r) => r.left)),
      cell: rects[0]?.width ?? 0,
      cut: rects.filter((r) => r.right > clientRight + 0.5).length,
      firstGridCols: cols,
      panelLeft: panel.left,
      panelRight: panel.right,
      pickerLeft: pr.left,
      pickerRight: pr.right,
    };
  });
}

async function openPicker(page: Page, opener: string, panel: string, within?: Locator) {
  await (within ?? wrap(page)).getByRole('button', { name: opener }).first().click();
  const dialog = page.getByRole('dialog', { name: panel }).last();
  await expect(dialog).toBeVisible();
  await page.waitForTimeout(500);
  return dialog;
}

function line(where: string, g: Grid) {
  return (
    `${where}: scroll client ${g.clientWidth} scrollW ${g.scrollWidth} clientBox ${g.clientLeft.toFixed(1)}..${g.clientRight.toFixed(1)} ` +
    `buttons ${g.buttons} cell ${g.cell.toFixed(1)} cols ${g.firstGridCols} minLeft ${g.minLeft.toFixed(1)} maxRight ${g.maxRight.toFixed(1)} ` +
    `cut ${g.cut} picker ${g.pickerLeft.toFixed(1)}..${g.pickerRight.toFixed(1)} panel ${g.panelLeft.toFixed(1)}..${g.panelRight.toFixed(1)}`
  );
}

/** Every column whole: nothing past the scroll area's client box, no sideways scroll, the picker inside its panel. */
async function expectWhole(page: Page, dialog: Locator, where: string, vpWidth: number) {
  const g = await grid(dialog);
  note(line(where, g));
  expect(g.buttons, `${where} has emoji`).toBeGreaterThan(0);
  expect(g.cut, `${where}: buttons cut at the end edge`).toBe(0);
  expect(g.maxRight, `${where} last column inside`).toBeLessThanOrEqual(g.clientRight + 0.5);
  expect(g.minLeft, `${where} first column inside`).toBeGreaterThanOrEqual(g.clientLeft - 0.5);
  expect(g.scrollWidth, `${where} no sideways scroll`).toBeLessThanOrEqual(g.clientWidth);
  expect(g.pickerRight, `${where} picker inside its panel`).toBeLessThanOrEqual(g.panelRight + 0.5);
  expect(g.panelLeft, `${where} panel on screen`).toBeGreaterThanOrEqual(0);
  expect(g.panelRight, `${where} panel on screen`).toBeLessThanOrEqual(vpWidth);
  const over = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(over, `${where} page overflow`).toBeLessThanOrEqual(0);
  return g;
}

test('the base: the last column of the emoji grid is cut', async ({ browser }) => {
  test.skip(!BASE, 'base measurement only (SR_P78_BASE=1)');
  const { octx, a, guild, channel } = await world(browser);
  await apiPatch(a, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
  const seen: Record<string, Grid> = {};
  for (const [vp, size] of Object.entries(VIEWPORTS)) {
    const c = await context(browser, octx, size, 'lounge-editorial');
    const page = await c.newPage();
    await page.goto(`/channels/${guild.id}/${channel}`);
    await expect(wrap(page).locator('textarea')).toBeVisible();
    const dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
    const g = await grid(dialog);
    seen[vp] = g;
    note(line(`BASE ${vp}`, g));
    await page.screenshot({ path: resolve(OUT, `base-emoji-${vp}.png`) });
    await c.close();
  }
  await octx.close();
  // The fault: on the phone something is cut or scrolls sideways. The desktop is only reported.
  const p = seen.phone!;
  expect(p.cut > 0 || p.scrollWidth > p.clientWidth, 'base phone: a column is cut').toBe(true);
});

test('every column of the emoji picker is whole', async ({ browser }) => {
  test.skip(BASE || POPOVERS, 'not in the base or popover run');
  const { octx, a, guild, channel } = await world(browser);
  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(wrap(page).locator('textarea')).toBeVisible();

      // 1. Top of the list.
      let dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
      const g = await expectWhole(page, dialog, `${tag} top`, size.width);
      // The same gap either side of the grid.
      expect(
        Math.abs(g.minLeft - g.clientLeft - (g.clientRight - g.maxRight)),
        `${tag} gaps either side`,
      ).toBeLessThanOrEqual(4);
      await page.screenshot({ path: resolve(OUT, `emoji-top-${tag}.png`) });

      // 2. A later category: press each category button; shoot after the third from last.
      const cats = dialog.locator('.lng-emoji-cat');
      const n = await cats.count();
      expect(n, `${tag} categories`).toBeGreaterThan(3);
      for (let i = 0; i < n; i++) {
        await cats.nth(i).click();
        await page.waitForTimeout(250);
        if (i === n - 3) {
          await page.waitForTimeout(600);
          await expectWhole(page, dialog, `${tag} category ${i}`, size.width);
          await page.screenshot({ path: resolve(OUT, `emoji-later-${tag}.png`) });
        }
      }
      await expectWhole(page, dialog, `${tag} after all categories`, size.width);

      // 3. Click through: pick the last emoji of a row (the seventh).
      await cats.first().click();
      await page.waitForTimeout(500);
      const ta = wrap(page).locator('textarea');
      await ta.fill('');
      const row = dialog.locator('.lng-emoji-grid').first().locator('.lng-emoji-btn');
      const seventh = row.nth(Math.min(6, (await row.count()) - 1));
      const sb = (await seventh.boundingBox())!;
      const scb = (await dialog.locator('.lng-emoji-scroll').boundingBox())!;
      expect(sb.x + sb.width, `${tag} seventh inside`).toBeLessThanOrEqual(scb.x + scb.width + 0.5);
      await seventh.click();
      await expect
        .poll(async () => (await ta.inputValue()).length, { message: `${tag} picked` })
        .toBeGreaterThan(0);

      // 4. Favourite: a right click on one marks it (the picker stays open after a pick).
      const target = dialog.locator('.lng-emoji-btn').nth(3);
      const label = (await target.getAttribute('aria-label'))!;
      await target.click({ button: 'right' });
      await expect(
        dialog.getByRole('button', { name: label, exact: true }).first(),
        `${tag} favourite marked`,
      ).toHaveAttribute('data-favorite', /.*/);
      await expectWhole(page, dialog, `${tag} with favourites`, size.width);
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);

      if (vp === 'phone') {
        // 5. The search field typed into.
        if (theme === 'lounge-dark') {
          dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
          await dialog.locator('.lng-emoji-search').fill('smile');
          await page.waitForTimeout(400);
          await expectWhole(page, dialog, `${tag} search`, size.width);
          await page.screenshot({ path: resolve(OUT, `emoji-search-${tag}.png`) });
          await page.keyboard.press('Escape');
          await expect(dialog).toHaveCount(0);
        }

        // 6. The reaction picker on a message (the "+" beside the existing reaction).
        if (theme !== 'lounge-editorial') {
          const reactions = page.locator('.lng-reactions').first();
          await expect(reactions).toBeVisible();
          dialog = await openPicker(
            page,
            'Add reaction to this message',
            'Add reaction',
            reactions,
          );
          await expectWhole(page, dialog, `${tag} reaction picker`, size.width);
          await page.screenshot({ path: resolve(OUT, `reaction-${tag}.png`) });
          await page.keyboard.press('Escape');
          await expect(dialog).toHaveCount(0);
        }

        // 7. The thread pane, one theme.
        if (theme === 'lounge-light') {
          await page.locator('.lng-thread-chip', { hasText: 'Emoji thread' }).click();
          await page.waitForTimeout(800);
          dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
          await expectWhole(page, dialog, `${tag} thread`, size.width);
          await page.screenshot({ path: resolve(OUT, `thread-emoji-${tag}.png`) });
        }
      }
      await c.close();
    }
  }
  await octx.close();
});

// ---- Tail (D-806): the Lounge's other popovers on a phone -----------------------------------------------------

const PNG_1PX =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';
const POP_THEMES = (process.env.SR_P78_EMOJI_THEMES ?? 'lounge-dark').split(',');

interface PopMeasure {
  panelLeft: number;
  panelRight: number;
  panelTop: number;
  panelBottom: number;
  scrollW: number;
  clientW: number;
  cut: string[];
}

/** The open popover's box against the screen and anything inside it that sticks out past its sides. */
async function measurePopover(dialog: Locator): Promise<PopMeasure> {
  return dialog.evaluate((root) => {
    const r = root.getBoundingClientRect();
    const cut: string[] = [];
    for (const el of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
      const b = el.getBoundingClientRect();
      if (b.width === 0 || b.height === 0) continue;
      if (b.right > r.right + 0.5 || b.left < r.left - 0.5) {
        const cls = typeof el.className === 'string' ? el.className.split(' ')[0] : '';
        cut.push(`${el.tagName.toLowerCase()}.${cls} ${b.left.toFixed(0)}..${b.right.toFixed(0)}`);
      }
    }
    // The widest scroll area inside the panel (or the panel itself).
    let scrollW = root.scrollWidth;
    let clientW = root.clientWidth;
    for (const el of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
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

test('the other popovers on a phone', async ({ browser }) => {
  test.skip(!POPOVERS, 'popover look-over only (SR_P78_EMOJI_POPOVERS=1)');
  const { octx, a, guild, channel } = await world(browser);
  // A pinned message, a custom emoji in a message.
  const pinned = await apiPost<{ id: string }>(a, `/channels/${channel}/messages`, {
    content:
      'A pinned message with a fairly long line of text so that it has to wrap inside the narrow panel',
  });
  await apiPut(a, `/channels/${channel}/pins/${pinned.id}`).catch(() => undefined);
  const emoji = await apiPost<{ id: string }>(a, `/guilds/${guild.id}/emojis`, {
    name: 'tinydot',
    image: PNG_1PX,
  });
  await apiPost(a, `/channels/${channel}/messages`, {
    content: `custom emoji <:tinydot:${emoji.id}> in a message`,
  });
  const size = VIEWPORTS.phone;
  for (const theme of POP_THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const c = await context(browser, octx, size, theme);
    const page = await c.newPage();
    page.setDefaultTimeout(15_000);
    await page.goto(`/channels/${guild.id}/${channel}`);
    await expect(wrap(page).locator('textarea')).toBeVisible();
    await page.waitForTimeout(800);

    const cases: [string, () => Promise<Locator | null>][] = [
      ['threads', async () => page.getByRole('button', { name: 'Threads', exact: true })],
      ['pins', async () => page.getByRole('button', { name: 'Pinned Messages', exact: true })],
      ['saved', async () => page.getByRole('button', { name: 'Saved items', exact: true })],
      ['inbox', async () => page.getByRole('button', { name: 'Inbox', exact: true })],
      ['custom-emoji', async () => page.getByRole('button', { name: /Emoji :tinydot:/ }).first()],
      [
        'device-input',
        async () => page.getByRole('button', { name: 'Input device and volume', exact: true }),
      ],
      [
        'device-output',
        async () => page.getByRole('button', { name: 'Output device and volume', exact: true }),
      ],
    ];
    for (const [name, trigger] of cases) {
      const tag = `popover-${name}-${theme}`;
      const t = await trigger();
      if (
        !t ||
        !(await t
          .first()
          .isVisible()
          .catch(() => false))
      ) {
        note(`${tag}: NOT REACHED (trigger not on the phone page)`);
        continue;
      }
      await t.first().click();
      await page.waitForTimeout(700);
      const dialog = page.getByRole('dialog').last();
      if (!(await dialog.isVisible().catch(() => false))) {
        note(`${tag}: NOT REACHED (no panel opened)`);
        continue;
      }
      const m = await measurePopover(dialog);
      const bad =
        m.panelLeft < -0.5 ||
        m.panelRight > size.width + 0.5 ||
        m.scrollW > m.clientW + 0.5 ||
        m.cut.length > 0;
      note(
        `${tag}: panel ${m.panelLeft.toFixed(1)}..${m.panelRight.toFixed(1)} x ${m.panelTop.toFixed(1)}..${m.panelBottom.toFixed(1)} ` +
          `scroll ${m.scrollW}/${m.clientW} cut [${m.cut.join('; ')}] => ${bad ? 'CUT' : 'whole'}`,
      );
      await page.screenshot({ path: resolve(OUT, `${tag}-phone.png`) });
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
    }

    // Thread members: in the thread pane.
    {
      const tag = `popover-thread-members-${theme}`;
      await page.locator('.lng-thread-chip', { hasText: 'Emoji thread' }).first().click();
      await page.waitForTimeout(900);
      const t = page.getByRole('button', { name: /Thread members/ }).first();
      if (await t.isVisible().catch(() => false)) {
        await t.click();
        await page.waitForTimeout(700);
        const dialog = page.getByRole('dialog').last();
        const m = await measurePopover(dialog);
        const bad =
          m.panelLeft < -0.5 ||
          m.panelRight > size.width + 0.5 ||
          m.scrollW > m.clientW + 0.5 ||
          m.cut.length > 0;
        note(
          `${tag}: panel ${m.panelLeft.toFixed(1)}..${m.panelRight.toFixed(1)} scroll ${m.scrollW}/${m.clientW} cut [${m.cut.join('; ')}] => ${bad ? 'CUT' : 'whole'}`,
        );
        await page.screenshot({ path: resolve(OUT, `${tag}-phone.png`) });
      } else {
        note(`${tag}: NOT REACHED (trigger not visible)`);
      }
    }
    note(
      `popover-soundboard/privacy-codes/volume-${theme}: NOT REACHED (they need a live voice call)`,
    );
    await c.close();
  }
  await octx.close();
});
