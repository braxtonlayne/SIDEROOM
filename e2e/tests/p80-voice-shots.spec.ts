import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-325 / SEC-AUD-326 (wave 80; D-827, D-828), on the real stack with a real one-member call (the server runs
 * with SR_VOICE__ENABLED=true, Chromium's fake microphone and camera come from the config's launch arguments).
 *  - Item 1: after "Back to channels" and after going forward again (three tries) every pane's left is a multiple of
 *    the viewport width, the visible one 0, and nothing of the root, `html`, `body` or an ancestor is scrolled.
 *    A trap logs who called `focus()` and `scrollIntoView()`.
 *  - Item 2: the call bar's "Microphone options" and "Camera options": the control's box, what is at its centre, a
 *    touch tap (a forced click when the tap did nothing), then the open menu's figures, a device chosen.
 *  - SR_P80_BASE=1 logs and shoots without asserting item 1 or the 44 px target (the base case).
 * Local only: SR_P80_VOICE=1. Shots and `measurements.txt` go to SR_P80_VOICE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P80_VOICE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-voice';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };
const VOICE = process.env.SR_P80_VOICE === '1';
const BASE = process.env.SR_P80_BASE === '1';
const THEMES = (
  process.env.SR_P80_VOICE_THEMES ?? 'lounge-dark,lounge-light,lounge-editorial'
).split(',');

test.skip(!VOICE, 'local visual check only (SR_P80_VOICE=1)');
test.setTimeout(1_500_000);

const note = (text: string) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(text);
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
      // The trap: who focused or scrolled something into view.
      const w = window as unknown as { __trap: string[] };
      w.__trap = [];
      const desc = (el: Element | null) =>
        el
          ? `${el.tagName.toLowerCase()}.${String((el as HTMLElement).className).split(' ')[0]}`
          : '-';
      const origFocus = HTMLElement.prototype.focus;
      HTMLElement.prototype.focus = function (this: HTMLElement, ...a: unknown[]) {
        w.__trap.push(
          `focus ${desc(this)} opts=${JSON.stringify(a[0] ?? {})} at ${(new Error().stack ?? '').split('\n')[2]?.trim().slice(0, 110)}`,
        );
        return (origFocus as (...x: unknown[]) => void).apply(this, a);
      };
      const origSiv = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = function (this: Element, ...a: unknown[]) {
        w.__trap.push(
          `scrollIntoView ${desc(this)} at ${(new Error().stack ?? '').split('\n')[2]?.trim().slice(0, 110)}`,
        );
        return (origSiv as (...x: unknown[]) => void).apply(this, a);
      };
    },
    { theme, zoom: 100 },
  );
  return c;
}

const themeOf = (page: Page) =>
  page.evaluate(() =>
    ['.lng-root', 'html', 'body']
      .map((q) => {
        const el = document.querySelector<HTMLElement>(q);
        return `${q}[${Array.from(el?.attributes ?? [])
          .filter((x) => /theme/.test(x.name))
          .map((x) => `${x.name}=${x.value}`)
          .join(',')}]`;
      })
      .join(' '),
  );

interface Pos {
  line: string;
  bad: string[];
}

/** Scroll offsets of the document, body, root and every ancestor of `.lng-main`; each pane's left. */
async function snap(page: Page, label: string, vw: number): Promise<Pos> {
  const r = await page.evaluate(() => {
    const w = window as unknown as { __trap: string[] };
    const scrolled: string[] = [];
    const se = document.scrollingElement as HTMLElement;
    const list: [string, Element | null][] = [
      ['scrollingElement', se],
      ['html', document.documentElement],
      ['body', document.body],
      ['.lng-root', document.querySelector('.lng-root')],
    ];
    const main = document.querySelector('.lng-main');
    for (let e = main?.parentElement ?? null; e; e = e.parentElement) {
      list.push([`anc ${e.tagName.toLowerCase()}.${String(e.className).split(' ')[0]}`, e]);
    }
    const seen = new Set<Element>();
    for (const [n, el] of list) {
      if (!el || seen.has(el)) continue;
      seen.add(el);
      const h = el as HTMLElement;
      scrolled.push(`${n}:${h.scrollLeft}/${h.scrollTop}`);
    }
    const lefts: Record<string, number> = {};
    for (const s of ['.lng-left', '.lng-main', '.lng-right']) {
      const el = document.querySelector(s);
      lefts[s] = el ? el.getBoundingClientRect().left : NaN;
    }
    const ae = document.activeElement as HTMLElement | null;
    const trap = w.__trap.splice(0).slice(-6);
    return {
      scrolled,
      lefts,
      active: ae
        ? `${ae.tagName.toLowerCase()}.${String(ae.className).split(' ')[0]} "${(ae.getAttribute('aria-label') ?? ae.textContent ?? '').slice(0, 30)}"`
        : '-',
      pane: document.querySelector<HTMLElement>('.lng-root')?.dataset.pane ?? '?',
      trap,
      over: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
  });
  const bad: string[] = [];
  for (const [s, left] of Object.entries(r.lefts)) {
    if (Number.isNaN(left)) continue;
    const k = left / vw;
    if (Math.abs(k - Math.round(k)) * vw > 0.5) bad.push(`${s} left ${left.toFixed(1)}`);
  }
  const visible = { list: '.lng-left', chat: '.lng-main', members: '.lng-right' }[
    r.pane as 'list' | 'chat' | 'members'
  ];
  if (visible && Math.abs(r.lefts[visible] ?? 0) > 0.5) {
    bad.push(`visible pane ${visible} left ${r.lefts[visible]?.toFixed(1)}`);
  }
  for (const s of r.scrolled) {
    const [n, v] = s.split(':');
    if (n && /^(scrollingElement|html|body|\.lng-root)/.test(n) && v?.split('/')[0] !== '0') {
      bad.push(`scrollLeft ${s}`);
    }
  }
  const nonZero = r.scrolled.filter((s) => s.split(':')[1]?.split('/')[0] !== '0');
  const line =
    `${label}: pane=${r.pane} lefts ${Object.entries(r.lefts)
      .map(([k, v]) => `${k}=${v.toFixed(1)}`)
      .join(' ')} scrollLeft!=0 [${nonZero.join('; ')}] active ${r.active} pageOver ${r.over} ` +
    `trap [${r.trap.join(' || ')}] => ${bad.length ? `SHIFTED ${bad.join(', ')}` : 'ok'}`;
  note(line);
  return { line, bad };
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
  if (!ok) {
    const state = await page
      .locator('.vc-state')
      .first()
      .innerText()
      .catch(() => '(no .vc-state)');
    note(`${tag}: the page shows "${state}"`);
  }
  return ok;
}

async function hangUp(page: Page) {
  const leave = page.getByRole('button', { name: 'Disconnect' }).first();
  if (await leave.isVisible().catch(() => false)) await leave.click().catch(() => undefined);
  await page.waitForTimeout(800);
}

test('the phone call page stays at 0, the call bar menus open and are whole', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Voice Room' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const voiceChannel =
    guild.channels.find((c) => c.type === 2)?.id ??
    (await apiPost<{ id: string }>(a, `/guilds/${guild.id}/channels`, { name: 'Calls', type: 2 }))
      .id;
  await apiPost(a, `/channels/${channel}/messages`, { content: 'Something to look at' });
  const faults: string[] = [];

  for (const theme of THEMES) {
    const editorial = theme === 'lounge-editorial';
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const c = await context(browser, octx, PHONE, theme, true);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    await page.goto(`/channels/${guild.id}/${voiceChannel}`);
    await page.waitForTimeout(1500);
    note(`== ${theme} phone: room wears data-theme=${await themeOf(page)}`);
    if (!(await join(page, `phone ${theme}`))) {
      await page.screenshot({ path: resolve(OUT, `not-connected-${theme}.png`) });
      await c.close();
      note('NOT CONNECTED: stopping, both items listed again');
      break;
    }
    await page.waitForTimeout(1500);
    await snap(page, `${theme} call page`, PHONE.width);
    await page.screenshot({ path: resolve(OUT, `call-${theme}.png`) });

    // ---- Item 1: back and forward, three tries --------------------------------------------------
    for (let i = 1; i <= 3; i++) {
      await page.getByRole('button', { name: 'Back to channels' }).first().click();
      await page.waitForTimeout(700);
      const p = await snap(page, `${theme} try ${i} after Back to channels`, PHONE.width);
      if (i === 1) await page.screenshot({ path: resolve(OUT, `back-${theme}.png`) });
      if (p.bad.length) faults.push(`${theme} back ${i}: ${p.bad.join(', ')}`);
      const voiceBtn = page.getByRole('button', { name: /voice channel, connected/ }).first();
      await voiceBtn.click();
      await page.waitForTimeout(500);
      if ((await page.locator('.lng-root').getAttribute('data-pane')) !== 'chat') {
        // The route is already the call's: go through the text channel, then back to the call.
        await page.locator('.lng-left').getByText('general', { exact: true }).first().click();
        await page.waitForTimeout(500);
        await page.getByRole('button', { name: 'Back to channels' }).first().click();
        await page.waitForTimeout(500);
        await voiceBtn.click();
      }
      await page.waitForTimeout(700);
      const q = await snap(page, `${theme} try ${i} forward again`, PHONE.width);
      if (i === 1) await page.screenshot({ path: resolve(OUT, `forward-${theme}.png`) });
      if (q.bad.length) faults.push(`${theme} forward ${i}: ${q.bad.join(', ')}`);
    }
    // Provoked: from the channel list, script scrolls into view / focuses a control of the off-screen call page (what
    // Playwright's own click does, and what a focus() without preventScroll does).
    await page.getByRole('button', { name: 'Back to channels' }).first().click();
    await page.waitForTimeout(700);
    const far = page.locator('.lng-main .vc-ctl-arrow, .lng-main .vc-ctl').first();
    await far
      .evaluate((el) => el.scrollIntoView({ block: 'nearest', inline: 'center' }))
      .catch(() => undefined);
    await page.waitForTimeout(400);
    const pv1 = await snap(page, `${theme} provoked scrollIntoView`, PHONE.width);
    if (pv1.bad.length) faults.push(`${theme} provoked scrollIntoView: ${pv1.bad.join(', ')}`);
    await page.screenshot({ path: resolve(OUT, `provoked-${theme}.png`) });
    await far.evaluate((el) => (el as HTMLElement).focus()).catch(() => undefined);
    await page.waitForTimeout(400);
    const pv2 = await snap(page, `${theme} provoked focus`, PHONE.width);
    if (pv2.bad.length) faults.push(`${theme} provoked focus: ${pv2.bad.join(', ')}`);
    await page
      .getByRole('button', { name: /voice channel, connected/ })
      .first()
      .click()
      .catch(() => undefined);
    await page.waitForTimeout(500);
    if ((await page.locator('.lng-root').getAttribute('data-pane')) !== 'chat') {
      await page.locator('.lng-left').getByText('general', { exact: true }).first().click();
      await page.waitForTimeout(500);
      await page.getByRole('button', { name: 'Back to channels' }).first().click();
      await page.waitForTimeout(500);
      await page
        .getByRole('button', { name: /voice channel, connected/ })
        .first()
        .click();
      await page.waitForTimeout(500);
    }
    await snap(page, `${theme} after the provoked scroll, forward again`, PHONE.width);
    if (editorial) {
      await hangUp(page);
      await c.close();
      continue;
    }

    // ---- Item 2: the call bar's chevrons --------------------------------------------------------
    // Camera on first (the fake camera), so that its menu lists a device.
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
      const chev = page.getByRole('button', { name: label, exact: true });
      const tag = `${theme} ${name}-chevron`;
      const n = await chev.count();
      if (!n) {
        note(`${tag}: NO such button`);
        faults.push(`${tag} missing`);
        continue;
      }
      const info = await chev.first().evaluate((el) => {
        const b = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        const cx = b.left + b.width / 2;
        const cy = b.top + b.height / 2;
        const at = document.elementFromPoint(cx, cy);
        let inert = false;
        for (let e: Element | null = el; e; e = e.parentElement) {
          if ((e as HTMLElement).inert) inert = true;
        }
        return {
          box: `${b.left.toFixed(1)},${b.top.toFixed(1)} ${b.width.toFixed(1)}x${b.height.toFixed(1)}`,
          w: b.width,
          h: b.height,
          pe: cs.pointerEvents,
          vis: cs.visibility,
          inert,
          cx,
          cy,
          self: at === el || el.contains(at),
          at: at
            ? `${at.tagName.toLowerCase()}.${String((at as HTMLElement).className).split(' ')[0]}`
            : 'null',
        };
      });
      note(
        `${tag}: count ${n} box ${info.box} pointer-events ${info.pe} visibility ${info.vis} inert ${info.inert} ` +
          `elementFromPoint ${info.self ? 'the control' : info.at}`,
      );
      await page.screenshot({ path: resolve(OUT, `bar-${name}-${theme}.png`) });
      let how = 'tap';
      await page.touchscreen.tap(info.cx, info.cy);
      await page.waitForTimeout(700);
      let open = await page
        .locator('[role="menu"]')
        .last()
        .isVisible()
        .catch(() => false);
      if (!open) {
        how = 'forced click';
        await chev
          .first()
          .click({ force: true, timeout: 5000 })
          .catch(() => undefined);
        await page.waitForTimeout(700);
        open = await page
          .locator('[role="menu"]')
          .last()
          .isVisible()
          .catch(() => false);
      }
      note(
        `${tag}: opened by ${open ? how : 'NOTHING'} (a tap ${how === 'tap' && open ? 'worked' : 'did not'})`,
      );
      if (!open) {
        faults.push(`${tag} did not open`);
        continue;
      }
      const m = await measureMenu(page);
      note(
        `${tag} menu: panel ${m.left.toFixed(1)}..${m.right.toFixed(1)} x ${m.top.toFixed(1)}..${m.bottom.toFixed(1)} ` +
          `scroll ${m.scrollW}/${m.clientW} past [${m.past.join('; ')}] items [${m.items.join(' | ')}]`,
      );
      await page.screenshot({ path: resolve(OUT, `menu-${name}-${theme}.png`) });
      const whole =
        m.left >= -0.5 &&
        m.right <= PHONE.width + 0.5 &&
        m.top >= -0.5 &&
        m.bottom <= PHONE.height + 0.5 &&
        m.scrollW <= m.clientW + 0.5 &&
        m.past.length === 0;
      note(
        `${tag} menu => ${whole ? 'whole' : 'NOT WHOLE'}; chevron ${info.w.toFixed(1)}x${info.h.toFixed(1)}`,
      );
      if (!whole) faults.push(`${tag} menu not whole`);
      if (info.w < 24 || info.h < 24) faults.push(`${tag} chevron under 24 px`);
      if (info.w < 44 || info.h < 44)
        note(
          `${tag}: touch target ${info.w.toFixed(0)}x${info.h.toFixed(0)} (44 x 44 is the target, 24 x 24 the floor)`,
        );
      const dev = page.locator('[role="menu"]').last().locator('[role="menuitemradio"]');
      const count = await dev.count();
      if (count < 1) faults.push(`${tag} no device listed`);
      else {
        await dev.nth(count > 1 ? 1 : 0).click();
        await page.waitForTimeout(500);
        const still = await page
          .locator('[role="menu"]')
          .last()
          .isVisible()
          .catch(() => false);
        note(
          `${tag}: pressed device ${count > 1 ? 1 : 0} of ${count}, menu ${still ? 'STILL OPEN' : 'closed'}`,
        );
        if (still) {
          await page.keyboard.press('Escape');
          await page.waitForTimeout(300);
        }
      }
      const pos = await snap(page, `${tag} after the menu`, PHONE.width);
      if (pos.bad.length) faults.push(`${tag} after menu: ${pos.bad.join(', ')}`);
    }
    await hangUp(page);
    await c.close();

    // ---- Desktop: the user panel's device menu, nothing moved ----------------------------------
    const dc = await context(browser, octx, DESKTOP, theme, false);
    const dp = await dc.newPage();
    dp.setDefaultTimeout(20_000);
    await dp.goto(`/channels/${guild.id}/${voiceChannel}`);
    await dp.waitForTimeout(1500);
    note(`== ${theme} desktop: room wears data-theme=${await themeOf(dp)}`);
    if (await join(dp, `desktop ${theme}`)) {
      await dp.waitForTimeout(1500);
      const btn = dp.getByRole('button', { name: 'Input device and volume', exact: true }).first();
      if (await btn.isVisible().catch(() => false)) {
        await btn.click();
        await dp.waitForTimeout(700);
        const dlg = dp.getByRole('dialog').last();
        const b = await dlg.boundingBox().catch(() => null);
        note(
          `desktop ${theme} device menu: panel ${b ? `${b.x.toFixed(0)}..${(b.x + b.width).toFixed(0)}` : 'none'}`,
        );
        await dp.screenshot({ path: resolve(OUT, `desktop-device-${theme}.png`) });
        await dp.keyboard.press('Escape');
      } else note(`desktop ${theme}: no Input device button`);
      await dp.screenshot({ path: resolve(OUT, `desktop-call-${theme}.png`) });
      await hangUp(dp);
    }
    await dc.close();
  }
  await octx.close();
  note(`FAULTS: ${faults.length ? faults.join(' ;; ') : 'none'}`);
  if (!BASE) expect(faults, 'the phone call page and its menus are whole').toEqual([]);
});
