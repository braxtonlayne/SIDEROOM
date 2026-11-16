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
 * SEC-AUD-331 / SEC-AUD-332 (wave 81; D-830, D-833), on the real stack with a real call (the server runs with
 * SR_VOICE__ENABLED=true, Chromium's fake microphone and camera): a second member (Bob) joins first and stays, then the
 * owner joins, per theme, once on a phone (375x812, touch) and once on a desktop (1280x800).
 *  - 331: the call bar's "Microphone options" / "Camera options": box, every control's box, the bar's scrollWidth and
 *    clientWidth, at 375 and (logged only) 360; both menus opened by `touchscreen.tap`, whole inside the screen, a
 *    device chosen; the desktop chevrons' box.
 *  - 332: the privacy codes popover: the verify button's computed look, text contrast, the pressed border against the
 *    popover, the three boxes of the row, not pressed and pressed (and the hover's background on desktop).
 *  - SR_P81_BASE=1 logs and shoots without asserting (the base case, the two CSS files held back).
 * Local only: SR_P81_VOICE=1. Shots and `measurements.txt` go to SR_P81_VOICE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P81_VOICE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-voice';
const PHONE = { width: 375, height: 812 };
const SMALL = { width: 360, height: 800 };
const DESKTOP = { width: 1280, height: 800 };
const BASE = process.env.SR_P81_BASE === '1';
const THEMES = (
  process.env.SR_P81_VOICE_THEMES ?? 'lounge-dark,lounge-light,lounge-editorial'
).split(',');

test.skip(process.env.SR_P81_VOICE !== '1', 'local visual check only (SR_P81_VOICE=1)');
test.setTimeout(1_500_000);

const note = (text: string) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(`REPORT ${text}`);
};

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
  phone: boolean,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    permissions: ['microphone', 'camera'],
    ...(phone ? { hasTouch: true, isMobile: true } : {}),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const themeOf = (page: Page) =>
  page.evaluate(() => document.querySelector<HTMLElement>('.lng-root')?.dataset.theme ?? '?');

async function join(page: Page, tag: string): Promise<boolean> {
  const j = page.getByRole('button', { name: 'Join Voice' }).first();
  await expect(j).toBeVisible();
  const t0 = Date.now();
  await j.click();
  const ok = await page
    .locator('.vc-state[data-ok]')
    .first()
    .waitFor({ state: 'attached', timeout: 90_000 })
    .then(() => true)
    .catch(() => false);
  note(`${tag}: connected ${ok} in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
  return ok;
}

async function hangUp(page: Page) {
  const leave = page.getByRole('button', { name: 'Disconnect' }).first();
  if (await leave.isVisible().catch(() => false)) await leave.click().catch(() => undefined);
  await page.waitForTimeout(800);
}

/** Every control of the call bar: box, and the bar's own scroll. */
async function barBoxes(page: Page, tag: string, vw: number): Promise<string[]> {
  const r = await page.evaluate((width) => {
    const bar = Array.from(document.querySelectorAll<HTMLElement>('.vc-controls')).find(
      (e) => e.getBoundingClientRect().width > 0,
    );
    if (!bar) return null;
    const ctl = Array.from(bar.querySelectorAll<HTMLElement>('button')).filter(
      (e) => e.getBoundingClientRect().width > 0,
    );
    const cut: string[] = [];
    const boxes = ctl.map((e) => {
      const b = e.getBoundingClientRect();
      const n = (e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 24);
      if (b.left < -0.5 || b.right > width + 0.5) cut.push(n);
      return `${n} ${b.left.toFixed(0)}..${b.right.toFixed(0)} ${b.width.toFixed(0)}x${b.height.toFixed(0)}`;
    });
    const gap = getComputedStyle(bar).columnGap;
    return { boxes, cut, scrollW: bar.scrollWidth, clientW: bar.clientWidth, gap };
  }, vw);
  if (!r) {
    note(`${tag}: no call bar`);
    return ['no call bar'];
  }
  note(
    `${tag}: bar scroll ${r.scrollW}/${r.clientW} (hidden ${r.scrollW - r.clientW}) gap ${r.gap}; cut at load [${r.cut.join(', ')}]; ${r.boxes.join(' | ')}`,
  );
  return r.cut;
}

async function chevrons(page: Page, tag: string) {
  const out: Record<string, { w: number; h: number }> = {};
  for (const label of ['Microphone options', 'Camera options']) {
    const el = page.getByRole('button', { name: label, exact: true }).first();
    const b = await el.boundingBox().catch(() => null);
    note(
      `${tag} ${label}: ${b ? `${b.x.toFixed(1)},${b.y.toFixed(1)} ${b.width}x${b.height}` : 'none'}`,
    );
    if (b) out[label] = { w: b.width, h: b.height };
  }
  return out;
}

interface MenuFig {
  left: number;
  right: number;
  top: number;
  bottom: number;
  scrollW: number;
  clientW: number;
  past: string[];
  items: string[];
}

async function measureMenu(page: Page): Promise<MenuFig> {
  return page
    .locator('[role="menu"]')
    .last()
    .evaluate((root) => {
      const r = root.getBoundingClientRect();
      const past: string[] = [];
      for (const el of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
        const b = el.getBoundingClientRect();
        if (b.width === 0 || b.height === 0) continue;
        if (b.right > r.right + 0.5 || b.left < r.left - 0.5) {
          past.push(`${el.tagName.toLowerCase()} ${b.left.toFixed(0)}..${b.right.toFixed(0)}`);
        }
      }
      return {
        left: r.left,
        right: r.right,
        top: r.top,
        bottom: r.bottom,
        scrollW: root.scrollWidth,
        clientW: root.clientWidth,
        past: past.slice(0, 6),
        items: Array.from(root.querySelectorAll('[role^="menuitem"]')).map(
          (e) => `${e.getAttribute('role')}:${(e.textContent ?? '').trim().slice(0, 40)}`,
        ),
      };
    });
}

/** The verify button: computed look, contrast, boxes of the row. */
async function verifyFigures(page: Page, tag: string): Promise<{ text: number; border: number }> {
  const r = await page
    .getByRole('dialog')
    .last()
    .evaluate((dlg) => {
      const btn = dlg.querySelector<HTMLElement>('.vc-e2ee-verify');
      if (!btn) return null;
      const parse = (c: string): [number, number, number, number] => {
        const m = c.match(/color\(srgb ([\d.e-]+) ([\d.e-]+) ([\d.e-]+)(?: \/ ([\d.e-]+))?\)/);
        if (m) return [+m[1]! * 255, +m[2]! * 255, +m[3]! * 255, m[4] === undefined ? 1 : +m[4]];
        const n = c.match(/[\d.]+/g)?.map(Number) ?? [0, 0, 0];
        return [n[0] ?? 0, n[1] ?? 0, n[2] ?? 0, n[3] ?? 1];
      };
      const over = (top: number[], under: number[]) => {
        const a = top[3] ?? 1;
        return [0, 1, 2].map((i) => (top[i] ?? 0) * a + (under[i] ?? 0) * (1 - a));
      };
      const lum = (c: number[]) => {
        const f = (v: number) => {
          const s = v / 255;
          return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
        };
        return 0.2126 * f(c[0] ?? 0) + 0.7152 * f(c[1] ?? 0) + 0.0722 * f(c[2] ?? 0);
      };
      const ratio = (a: number[], b: number[]) => {
        const x = lum(a);
        const y = lum(b);
        return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
      };
      // The ground of the popover: the first opaque background walking up from the dialog.
      let ground = [255, 255, 255];
      const chain: Element[] = [];
      for (let e: Element | null = dlg; e; e = e.parentElement) chain.push(e);
      for (const e of chain) {
        const bg = parse(getComputedStyle(e).backgroundColor);
        if (bg[3] > 0) {
          ground = over(bg, ground);
          if (bg[3] >= 0.99) break;
        }
      }
      const cs = getComputedStyle(btn);
      const bg = over(parse(cs.backgroundColor), ground);
      const fg = over(parse(cs.color), bg);
      const bd = over(parse(cs.borderTopColor), ground);
      const box = (el: Element | null) => {
        const b = el?.getBoundingClientRect();
        return b
          ? `${b.left.toFixed(0)},${b.top.toFixed(0)} ${b.width.toFixed(0)}x${b.height.toFixed(0)}`
          : '-';
      };
      const row = btn.closest('.vc-e2ee-row');
      const pop = getComputedStyle(dlg);
      return {
        pressed: btn.getAttribute('aria-pressed'),
        label: (btn.textContent ?? '').trim(),
        bg: cs.backgroundColor,
        color: cs.color,
        border: `${cs.borderTopWidth} ${cs.borderTopStyle} ${cs.borderTopColor}`,
        radius: cs.borderTopLeftRadius,
        font: `${cs.fontFamily.slice(0, 40)} ${cs.fontSize} ${cs.fontWeight}`,
        popFont: `${pop.fontFamily.slice(0, 40)} ${pop.fontSize}`,
        ground: `rgb(${ground.map((v) => Math.round(v)).join(',')})`,
        text: ratio(fg, bg),
        borderRatio: ratio(bd, ground),
        boxes: `name ${box(row?.querySelector('.vc-e2ee-name') ?? null)} | code ${box(row?.querySelector('.vc-e2ee-code') ?? null)} | button ${box(btn)} | row ${box(row ?? null)}`,
        shield: getComputedStyle(document.querySelector('.vc-e2ee-shield') ?? document.body).color,
      };
    });
  if (!r) {
    note(`${tag}: NO verify button`);
    return { text: 0, border: 0 };
  }
  note(
    `${tag} [${r.label}, pressed=${r.pressed}]: bg ${r.bg} colour ${r.color} border ${r.border} radius ${r.radius} font ${r.font} (popover ${r.popFont}) ` +
      `ground ${r.ground} text ${r.text.toFixed(2)}:1 border-vs-ground ${r.borderRatio.toFixed(2)}:1 shield ${r.shield}; boxes ${r.boxes}`,
  );
  return { text: r.text, border: r.borderRatio };
}

test('the phone call bar chevrons and the privacy codes button', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Voice Room' },
  );
  const text = guild.channels.find((c) => c.type === 0)!.id;
  const voice =
    guild.channels.find((c) => c.type === 2)?.id ??
    (await apiPost<{ id: string }>(a, `/guilds/${guild.id}/channels`, { name: 'Calls', type: 2 }))
      .id;
  const invite = await apiPost<{ code: string }>(a, `/channels/${text}/invites`, {});
  const bobCtx = await browser.newContext({
    baseURL: ORIGIN,
    permissions: ['microphone', 'camera'],
  });
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
  await bobPage.goto(`/channels/${guild.id}/${voice}`);
  const faults: string[] = [];
  note(`bob joining (one try)`);
  const bobIn = await join(bobPage, 'bob');
  if (!bobIn)
    note('Bob did not connect: the privacy codes are shot with one member if they list the owner');

  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const tShort = theme.replace('lounge-', '');
    for (const phone of [true, false]) {
      const size = phone ? PHONE : DESKTOP;
      const vp = phone ? 'phone' : 'desktop';
      const tag = `${theme} ${vp}`;
      const c = await context(browser, octx, size, theme, phone);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${voice}`);
      await page.waitForTimeout(1500);
      note(`== ${tag}: room wears data-theme=${await themeOf(page)}`);
      if (!(await join(page, tag))) {
        await page.screenshot({ path: resolve(OUT, `not-connected-${tShort}-${vp}.png`) });
        faults.push(`${tag} not connected`);
        await c.close();
        continue;
      }
      await page.waitForTimeout(2500);

      // ---- 331: the bar --------------------------------------------------------------------------
      const cut = await barBoxes(page, `${tag} bar`, size.width);
      const chev = await chevrons(page, `${tag} chevron`);
      const roomOver = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      note(`${tag}: page over ${roomOver}`);
      if (phone) {
        const pl = await page.locator('.lng-main').evaluate((e) => e.getBoundingClientRect().left);
        note(`${tag}: pane left ${pl.toFixed(1)}`);
        if (Math.abs(pl) > 0.5) faults.push(`${tag} pane left ${pl}`);
        for (const [k, v] of Object.entries(chev)) {
          if (v.w < 44 || v.h < 44) {
            note(`${tag} ${k}: ${v.w}x${v.h} is under the 44 x 44 target`);
            if (!BASE) faults.push(`${tag} ${k} ${v.w}x${v.h}`);
          }
        }
        if (cut.length) note(`${tag}: CUT AT LOAD ${cut.join(', ')}`);
        if (cut.length && !BASE) faults.push(`${tag} cut at load ${cut.join(', ')}`);
        if (theme !== 'lounge-editorial' || BASE) {
          await page.screenshot({ path: resolve(OUT, `bar-${tShort}-phone.png`) });
        }
        await page.setViewportSize(SMALL);
        await page.waitForTimeout(500);
        await barBoxes(page, `${tag} bar at 360x800 (logged only)`, SMALL.width);
        await chevrons(page, `${tag} chevron at 360`);
        await page.setViewportSize(PHONE);
        await page.waitForTimeout(500);
      } else {
        note(`${tag}: chevrons expected 20x28`);
        for (const [k, v] of Object.entries(chev)) {
          if ((v.w !== 20 || v.h !== 28) && !BASE) faults.push(`${tag} ${k} ${v.w}x${v.h}`);
        }
        if (tShort === 'dark')
          await page.screenshot({ path: resolve(OUT, `bar-dark-desktop.png`) });
      }

      // The chevrons' menus: by touch on the phone, Dark and Light.
      if (phone && theme !== 'lounge-editorial') {
        await page
          .getByRole('button', { name: 'Turn on camera', exact: true })
          .first()
          .tap()
          .catch(() => undefined);
        await page.waitForTimeout(2000);
        for (const [name, label] of [
          ['mic', 'Microphone options'],
          ['camera', 'Camera options'],
        ] as const) {
          const t2 = `${tag} ${name}-chevron`;
          const box = await page
            .getByRole('button', { name: label, exact: true })
            .first()
            .boundingBox();
          if (!box) {
            faults.push(`${t2} missing`);
            continue;
          }
          await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
          await page.waitForTimeout(700);
          const open = await page
            .locator('[role="menu"]')
            .last()
            .isVisible()
            .catch(() => false);
          note(`${t2}: opened by tap ${open}`);
          if (!open) {
            faults.push(`${t2} did not open`);
            continue;
          }
          const m = await measureMenu(page);
          note(
            `${t2} menu: panel ${m.left.toFixed(1)}..${m.right.toFixed(1)} x ${m.top.toFixed(1)}..${m.bottom.toFixed(1)} scroll ${m.scrollW}/${m.clientW} past [${m.past.join('; ')}] items [${m.items.join(' | ')}]`,
          );
          await page.screenshot({ path: resolve(OUT, `menu-${name}-${tShort}.png`) });
          const whole =
            m.left >= -0.5 &&
            m.right <= PHONE.width + 0.5 &&
            m.top >= -0.5 &&
            m.bottom <= PHONE.height + 0.5 &&
            m.scrollW <= m.clientW + 0.5 &&
            m.past.length === 0;
          note(`${t2} menu => ${whole ? 'whole' : 'NOT WHOLE'}`);
          if (!whole) faults.push(`${t2} menu not whole`);
          const dev = page.locator('[role="menu"]').last().locator('[role="menuitemradio"]');
          const count = await dev.count();
          if (count < 1) faults.push(`${t2} no device listed`);
          else {
            await dev.nth(count > 1 ? 1 : 0).click();
            await page.waitForTimeout(500);
            const still = await page
              .locator('[role="menu"]')
              .last()
              .isVisible()
              .catch(() => false);
            note(
              `${t2}: device ${count > 1 ? 1 : 0} of ${count} chosen, menu ${still ? 'STILL OPEN' : 'closed'}`,
            );
            if (still) {
              await page.keyboard.press('Escape');
              await page.waitForTimeout(300);
            }
          }
          const pl2 = await page
            .locator('.lng-main')
            .evaluate((e) => e.getBoundingClientRect().left);
          if (Math.abs(pl2) > 0.5) faults.push(`${t2} pane left ${pl2} after menu`);
        }
      }

      // ---- 332: the privacy codes ---------------------------------------------------------------
      const shield = page.locator('.vc-e2ee-shield:visible').first();
      if (phone) await shield.tap().catch(() => shield.click());
      else await shield.click();
      await page.waitForTimeout(800);
      const dialog = page.getByRole('dialog').last();
      const verify = dialog.locator('.vc-e2ee-verify').first();
      if (!(await verify.isVisible().catch(() => false))) {
        note(`${tag}: NO verify button in the popover (second member in: ${bobIn})`);
        await page.screenshot({ path: resolve(OUT, `privacy-none-${tShort}-${vp}.png`) });
        faults.push(`${tag} no verify button`);
      } else {
        const before = await verifyFigures(page, `${tag} privacy unpressed`);
        if (!phone) {
          await verify.hover();
          const hv = await verify.evaluate((e) => getComputedStyle(e).backgroundColor);
          const rest = await verify.evaluate((e) => {
            const s = e.style.cssText;
            return s;
          });
          await page.mouse.move(2, 2);
          const rb = await verify.evaluate((e) => getComputedStyle(e).backgroundColor);
          note(
            `${tag} hover: background ${hv} vs resting ${rb}${rest ? '' : ''} => ${hv === rb ? 'SAME' : 'differs'}`,
          );
          if (hv === rb && !BASE) faults.push(`${tag} hover equals resting`);
        }
        await page.screenshot({ path: resolve(OUT, `privacy-unpressed-${tShort}-${vp}.png`) });
        if (phone) await verify.tap();
        else await verify.click();
        await page.waitForTimeout(500);
        if (!phone) await page.mouse.move(2, 2);
        const after = await verifyFigures(page, `${tag} privacy pressed`);
        await page.screenshot({ path: resolve(OUT, `privacy-pressed-${tShort}-${vp}.png`) });
        if (!BASE) {
          if (before.text < 4.5) faults.push(`${tag} unpressed text ${before.text.toFixed(2)}`);
          if (after.text < 4.5) faults.push(`${tag} pressed text ${after.text.toFixed(2)}`);
          if (after.border < 3) faults.push(`${tag} pressed border ${after.border.toFixed(2)}`);
        }
        if (theme === 'lounge-light' && phone) {
          // Click-through: press it again (back to "Mark as verified").
          if (phone) await verify.tap();
          await page.waitForTimeout(500);
          await verifyFigures(page, `${tag} privacy pressed again`);
          await page.screenshot({ path: resolve(OUT, `privacy-again-${tShort}-${vp}.png`) });
        }
      }
      await hangUp(page);
      await c.close();
    }
  }
  await hangUp(bobPage);
  await bobCtx.close();
  await octx.close();
  note(`FAULTS: ${faults.length ? faults.join(' ;; ') : 'none'}`);
  if (!BASE) expect(faults, 'the call bar and the privacy codes button').toEqual([]);
});
