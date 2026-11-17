import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { expect } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';
import { over, parseColor, ratio, textRatio, type Rgba } from '../support/contrast';

/**
 * Wave 91, kit: SEC-AUD-394 (D-904, the guestbook "Sign" button's states), SEC-AUD-395 (D-905, the kit's label-less
 * choice control; Members' `ss-check-solo` removed), SEC-AUD-396 (D-906, FileDrop's text as a prop).
 * NEEDS THE SWEEP'S SEEDED WORLD: the Owner of `e2e/.state/sweep/owner.json` and `world.json`, made on the same
 * database by `page-sweep.spec.ts -g "seed the sweep world"` (this spec never edits that spec). Chromium only.
 *
 * Three settings (375x812 touch Lounge Light, 375x812 touch Lounge Dark, 1280x800 Editorial; the account's main
 * theme by PATCH and localStorage, `data-theme` read back). Parts (`SR_P91_KIT_PART`, default all):
 *  - `sign`: `/u/owner`, a note typed and not sent: the "Sign" button at rest, hovered, pressed (mouse down, released
 *    off the button so nothing is sent) and with keyboard focus; ground and text ratio from `e2e/support/contrast.ts`
 *    (a colour with alpha is laid over its ground first), hover's and pressed's ground against rest's, the focus ring,
 *    the disabled button, and at 375 Lounge Light the signed-out `/~owner` form.
 *  - `members`: the seeded server's Members page: every checkbox's box and hit tests, the first row's height, the
 *    first column's width, the table's width, one row ticked (bulk bar), select all (1280), Space, the focus ring;
 *    and the labelled kit Checkbox and Switch of user Settings pages (box, row height, label's x).
 *  - `drop`: Admin's Entrance page and the server's Expressions page at 375/1280: the FileDrop's text and box.
 *
 * Local only. The commander's command, from `e2e/`, on a stack with the seeded world (API 8691, Vite 5691):
 *   SR_P91_KIT_ON=1 SR_P91_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-kit \
 *   SR_E2E_BASE_URL=http://localhost:5691 npx playwright test tests/p91-kit-shots.spec.ts \
 *   --project=chromium --workers=1 --retries=0 --reporter=line
 * (`SR_P91_KIT_BASE=1` instead of `_ON=1` is the base case, run on the code before the change; it tags the files
 * `base`, the other run `after`; `SR_P91_KIT_PART` = a comma list of sign, members, drop.)
 */
const ON = process.env.SR_P91_KIT_ON === '1' || process.env.SR_P91_KIT_BASE === '1';
const BASE = process.env.SR_P91_KIT_BASE === '1';
const OUT =
  process.env.SR_P91_KIT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-kit';
const TAG = BASE ? 'base' : 'after';
const PART = (process.env.SR_P91_KIT_PART ?? 'sign,members,drop').split(',');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const ROW_BOXES = "table.ss-table tbody input[type='checkbox']";
const HEAD_BOX = "table.ss-table thead input[type='checkbox']";

interface Setting {
  key: string;
  theme: string;
  size: { width: number; height: number };
  phone: boolean;
}
const SETTINGS: Setting[] = [
  { key: 'phone-light', theme: 'lounge-light', size: { width: 375, height: 812 }, phone: true },
  { key: 'phone-dark', theme: 'lounge-dark', size: { width: 375, height: 812 }, phone: true },
  {
    key: 'desk-editorial',
    theme: 'lounge-editorial',
    size: { width: 1280, height: 800 },
    phone: false,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};

async function newPage(browser: Browser, s: Setting, base: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: s.size,
    storageState: await base.storageState(),
    bypassCSP: true,
    ...(s.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((theme) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(10_000);
  return { ctx, page };
}

async function setTheme(o: BrowserContext['request'], theme: string) {
  await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
  const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(`REPORT account theme set ${theme}, read back: ${back.settings?.user_prefs?.theme}`);
}

// ---------------------------------------------------------------- sign (item 1)

interface Raw {
  text: string;
  color: string;
  layers: string[];
  opacity: number;
  outlineColor: string;
  outlineWidth: string;
  outlineStyle: string;
  box: number[];
  disabled: boolean;
}

function probeRaw(root: string): Record<string, unknown> {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const one = (e: Element | null) => {
    if (!e) return null;
    const cs = getComputedStyle(e);
    let op = 1;
    for (let a: Element | null = e; a; a = a.parentElement)
      op *= Number(getComputedStyle(a).opacity);
    const layers: string[] = [];
    for (let a: Element | null = e; a; a = a.parentElement) {
      const bg = getComputedStyle(a).backgroundColor;
      if (bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') {
        layers.push(bg);
        const m = bg.match(/\/\s*([\d.]+)\s*\)|,\s*([\d.]+)\s*\)$/);
        const al = m ? Number(m[1] ?? m[2]) : 1;
        if (al >= 1) break;
      }
    }
    const b = e.getBoundingClientRect();
    return {
      text: (e.textContent ?? '').trim(),
      color: cs.color,
      layers,
      opacity: op,
      outlineColor: cs.outlineColor,
      outlineWidth: cs.outlineWidth,
      outlineStyle: cs.outlineStyle,
      box: [r1(b.left), r1(b.top + window.scrollY), r1(b.width), r1(b.height)],
      disabled: (e as HTMLButtonElement).disabled,
    } satisfies Raw;
  };
  const form = document.querySelector(`${root} form.pp-sign`);
  if (!form) return { found: false };
  return {
    found: true,
    button: one(form.querySelector('button[type="submit"]')),
    panel: one(form.parentElement),
    theme: (document.querySelector('[data-theme]') as HTMLElement | null)?.dataset.theme ?? null,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  };
}

function ground(layers: string[]): Rgba {
  let g: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const l of [...layers].reverse()) g = over(parseColor(l), g);
  return g;
}

/** The button's ground (its opacity laid over white), the text's ratio on it, and the ring against the panel. */
function figures(raw: Record<string, unknown>) {
  const b = raw.button as Raw | null;
  const panel = raw.panel as Raw | null;
  if (!b) return { found: false };
  const g = ground(b.layers);
  const eff = b.opacity < 1 ? over({ ...g, a: b.opacity }, { r: 255, g: 255, b: 255, a: 1 }) : g;
  const text = parseColor(b.color);
  const t = over({ ...text, a: text.a * b.opacity }, eff);
  const out: Record<string, unknown> = {
    text: b.text,
    color: b.color,
    layers: b.layers,
    opacity: b.opacity,
    disabled: b.disabled,
    box: b.box,
    ground: [eff.r, eff.g, eff.b].map(Math.round),
    ratio: Math.round(ratio(t, eff) * 100) / 100,
    outline: `${b.outlineStyle} ${b.outlineWidth} ${b.outlineColor}`,
    theme: raw.theme,
    doc: raw.doc,
    effGround: eff,
  };
  if (panel) {
    const pg = ground(panel.layers);
    out.groundOnPanel = Math.round(ratio(eff, pg) * 100) / 100;
    if (b.outlineStyle !== 'none') {
      const ring = over(parseColor(b.outlineColor), pg);
      out.ringOnPanel = Math.round(ratio(ring, pg) * 100) / 100;
    }
  }
  return out;
}

const probe = async (page: Page, root = '.pp-root') =>
  figures((await page.evaluate(probeRaw, root)) as Record<string, unknown>);

test('the contrast helper reads wave 89 figures', () => {
  const label = textRatio('color(srgb 0.949 0.953 0.961 / 0.75)', 'rgb(43, 45, 49)');
  const g = over(parseColor('color(srgb 0.345 0.396 0.949 / 0.45)'), parseColor('rgb(43,45,49)'));
  const btn = ratio(parseColor('rgb(242, 243, 245)'), g);
  console.log(`REPORT helper self-check: ${label.toFixed(2)} (7.70), ${btn.toFixed(2)} (7.72)`);
  expect(Math.abs(label - 7.7)).toBeLessThanOrEqual(0.05);
  expect(Math.abs(btn - 7.72)).toBeLessThanOrEqual(0.05);
});

test('SEC-AUD-394: the guestbook Sign button in every state', async ({ browser }) => {
  test.skip(!PART.includes('sign'), 'part off');
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `${TAG}-sign-measure.json`);
  writeFileSync(jsonl, '');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    for (const s of SETTINGS) {
      await setTheme(ownerCtx.request, s.theme);
      const { ctx, page } = await newPage(browser, s, ownerCtx);
      const line: Record<string, unknown> = { setting: s.key, want: s.theme };
      await page.goto('/u/owner', { waitUntil: 'domcontentloaded', timeout: 20_000 });
      const form = page.locator('.pp-root form.pp-sign').first();
      await form.waitFor({ timeout: 15_000 });
      await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
      await form.scrollIntoViewIfNeeded();
      await page.waitForTimeout(500);
      line.disabled = await probe(page);
      await form.locator('textarea').fill('A note, not sent.');
      await page.waitForTimeout(300);
      line.rest = await probe(page);
      await form.screenshot({ path: resolve(OUT, `${TAG}-sign-rest-${s.key}.png`) });
      const btn = form.locator('button[type="submit"]');
      await btn.hover();
      await page.waitForTimeout(400);
      line.hover = await probe(page);
      if (s.key === 'desk-editorial')
        await form.screenshot({ path: resolve(OUT, `${TAG}-sign-hover-${s.key}.png`) });
      await page.mouse.down();
      await page.waitForTimeout(300);
      line.pressed = await probe(page);
      if (s.key === 'desk-editorial')
        await form.screenshot({ path: resolve(OUT, `${TAG}-sign-pressed-${s.key}.png`) });
      // Release off the button: nothing is sent.
      await page.mouse.move(2, 2);
      await page.mouse.up();
      await page.waitForTimeout(200);
      await form.locator('textarea').focus();
      await page.keyboard.press('Tab');
      await page.waitForTimeout(300);
      line.focus = await probe(page);
      line.focusIsButton = await page.evaluate(
        () => document.activeElement?.matches('form.pp-sign button[type="submit"]') ?? false,
      );
      const eg = (k: string) => (line[k] as { effGround?: Rgba }).effGround;
      line.hoverGroundVsRest =
        eg('hover') && eg('rest') ? Math.round(ratio(eg('hover')!, eg('rest')!) * 100) / 100 : null;
      line.pressedGroundVsRest =
        eg('pressed') && eg('rest')
          ? Math.round(ratio(eg('pressed')!, eg('rest')!) * 100) / 100
          : null;
      if (s.key === 'phone-light') {
        const anon = await browser.newContext({
          baseURL: ORIGIN,
          viewport: s.size,
          userAgent: PHONE_UA,
          isMobile: true,
          hasTouch: true,
        });
        const ap = await anon.newPage();
        ap.setDefaultTimeout(10_000);
        await ap.goto('/~owner', { waitUntil: 'domcontentloaded', timeout: 20_000 });
        const af = ap.locator('form.pp-sign').first();
        await af.waitFor({ timeout: 15_000 });
        await af.scrollIntoViewIfNeeded();
        await ap.waitForTimeout(600);
        line.signedOut = await probe(ap, 'body');
        await af.screenshot({ path: resolve(OUT, `${TAG}-sign-signedout-${s.key}.png`) });
        await anon.close();
      }
      // JSON: drop the object copy of the ground.
      const strip = JSON.stringify(line, (k, v) => (k === 'effGround' ? undefined : v));
      appendFileSync(jsonl, `${strip}\n`);
      const rr = (k: string) => (line[k] as { ratio?: number }).ratio;
      const gr = (k: string) => (line[k] as { ground?: number[] }).ground?.join(',');
      console.log(
        `REPORT ${TAG} ${s.key} Sign ratio: disabled ${rr('disabled')} rest ${rr('rest')} hover ${rr('hover')} pressed ${rr('pressed')} focus ${rr('focus')}; grounds rest ${gr('rest')} hover ${gr('hover')} pressed ${gr('pressed')}; hoverVsRest ${line.hoverGroundVsRest} pressedVsRest ${line.pressedGroundVsRest}; focus ring ${(line.focus as { outline?: string }).outline}`,
      );
      await ctx.close();
    }
  } finally {
    await ownerCtx.close();
  }
});

// ---------------------------------------------------------------- members + labelled controls (item 2)

function measureMembers() {
  const de = document.documentElement;
  const table = document.querySelector<HTMLElement>('table.ss-table');
  const themed = document.querySelector('[data-theme]');
  if (!table) return { found: false, theme: themed?.getAttribute('data-theme') ?? null };
  table.scrollIntoView({ block: 'start' });
  const hitOk = (el: HTMLElement, x: number, y: number) => {
    const h = document.elementFromPoint(x, y);
    if (!h) return false;
    if (h === el || el.contains(h)) return true;
    const lab =
      el.closest('label') ?? (el.id ? document.querySelector(`label[for="${el.id}"]`) : null);
    return !!lab && (h === lab || lab.contains(h));
  };
  const box = (e: HTMLElement) => {
    const r = e.getBoundingClientRect();
    return [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10);
  };
  const rows = Array.from(table.querySelectorAll<HTMLElement>('tr'));
  const out = rows.slice(0, 4).map((tr, i) => {
    const ctls = Array.from(
      tr.querySelectorAll<HTMLElement>('input[type=checkbox], button, a, select, [role=button]'),
    )
      .filter((e) => e.getBoundingClientRect().width > 0)
      .map((e) => {
        const r = e.getBoundingClientRect();
        const cx = r.left + r.width / 2;
        const cy = r.top + r.height / 2;
        return {
          name: (e.getAttribute('aria-label') || e.textContent || '').trim().slice(0, 28),
          box: box(e),
          hitTop: hitOk(e, cx, cy - 21),
          hitBottom: hitOk(e, cx, cy + 21),
          hitLeft: hitOk(e, cx - 21, cy),
          hitRight: hitOk(e, cx + 21, cy),
          isCheckbox: e instanceof HTMLInputElement,
        };
      });
    return { i, rowH: Math.round(tr.getBoundingClientRect().height * 10) / 10, ctls };
  });
  const firstRow = table.querySelector<HTMLElement>('tbody tr');
  const firstCell = firstRow?.querySelector<HTMLElement>('td');
  const headCell = table.querySelector<HTMLElement>('thead th');
  const bar = document.querySelector<HTMLElement>('[role=toolbar]');
  const hdr = table.querySelector<HTMLInputElement>('thead input[type=checkbox]');
  return {
    found: true,
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    tableBox: box(table),
    firstRowH: firstRow ? Math.round(firstRow.getBoundingClientRect().height * 10) / 10 : null,
    firstColW: firstCell ? box(firstCell)[2] : null,
    headColW: headCell ? box(headCell)[2] : null,
    bulkBar: bar ? box(bar) : null,
    header: hdr ? { checked: hdr.checked, indeterminate: hdr.indeterminate } : null,
    labelsInFirstCell: firstCell ? firstCell.querySelectorAll('label').length : null,
    rows: out,
  };
}

/** Every kit choice control of the page (a checkbox or switch input): box, its row's box, its label's x. */
function measureChoices() {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const de = document.documentElement;
  const list = Array.from(document.querySelectorAll<HTMLInputElement>('input[type=checkbox]'))
    .filter((e) => e.getBoundingClientRect().width > 0 && e.id)
    .map((e) => {
      const b = e.getBoundingClientRect();
      const row = e.parentElement!;
      const rb = row.getBoundingClientRect();
      const lab = document.querySelector<HTMLElement>(`label[for="${e.id}"]`);
      const lb = lab?.getBoundingClientRect();
      return {
        kind: e.getAttribute('role') === 'switch' ? 'switch' : 'checkbox',
        name: (lab?.textContent ?? e.getAttribute('aria-label') ?? '').trim().slice(0, 30),
        box: [r1(b.left), r1(b.top + window.scrollY), r1(b.width), r1(b.height)],
        row: [r1(rb.left), r1(rb.top + window.scrollY), r1(rb.width), r1(rb.height)],
        labelX: lb ? r1(lb.left) : null,
        labelW: lb ? r1(lb.width) : null,
      };
    });
  return { doc: de.scrollWidth - de.clientWidth, list };
}

test('SEC-AUD-395: Members and the labelled kit controls', async ({ browser }) => {
  test.skip(!PART.includes('members'), 'part off');
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `${TAG}-members-measure.json`);
  writeFileSync(jsonl, '');
  const w = world();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const log = (id: string, s: Setting, data: unknown) => {
    appendFileSync(jsonl, `${JSON.stringify({ id, tag: TAG, setting: s.key, data })}\n`);
    console.log(`REPORT ${TAG} ${s.key} ${id} ${JSON.stringify(data).slice(0, 2500)}`);
  };
  const shoot = (page: Page, name: string, s: Setting) =>
    page.screenshot({ path: resolve(OUT, `${TAG}-${name}-${s.key}.png`), timeout: 10_000 });
  try {
    for (const s of SETTINGS) {
      await setTheme(ownerCtx.request, s.theme);
      const { ctx, page } = await newPage(browser, s, ownerCtx);
      try {
        await page.goto(`/settings/server/${w.guildId}`, {
          waitUntil: 'domcontentloaded',
          timeout: 15_000,
        });
        await idle(page, 4000);
        const labels = await page
          .locator(NAV)
          .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
        const i = labels.findIndex((l) => /^Members$/i.test(l));
        if (i < 0) throw new Error(`no nav item Members: ${labels.join(' | ')}`);
        await page
          .locator(NAV)
          .nth(i)
          .evaluate((el) => (el as HTMLElement).click());
        await idle(page, 2500);
        await page.evaluate(() => document.fonts?.ready);
        log('members', s, await page.evaluate(measureMembers));
        await shoot(page, 'members', s);
        const boxes = page.locator(ROW_BOXES);
        if (s.phone) {
          const bb = await boxes.first().boundingBox();
          if (bb) await page.touchscreen.tap(bb.x + bb.width / 2, bb.y + bb.height / 2);
          await page.waitForTimeout(300);
          log('members-one-ticked', s, await page.evaluate(measureMembers));
        } else {
          await boxes.first().click();
          await page.waitForTimeout(250);
          log('members-one-ticked', s, await page.evaluate(measureMembers));
          await shoot(page, 'members-selected', s);
          await page.locator(HEAD_BOX).click();
          await page.waitForTimeout(250);
          const all = await boxes.evaluateAll((els) =>
            els.map((e) => (e as HTMLInputElement).checked),
          );
          log('select-all', s, { rows: all.length, allTicked: all.every(Boolean) });
          await page.getByRole('button', { name: /clear selection/i }).click();
          await page.waitForTimeout(250);
          await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
          let reached = false;
          for (let k = 0; k < 80 && !reached; k++) {
            await page.keyboard.press('Tab');
            reached = await page.evaluate(() =>
              /^Select /.test(document.activeElement?.getAttribute('aria-label') ?? ''),
            );
          }
          const ring = await page.evaluate(() => {
            const a = document.activeElement as HTMLInputElement | null;
            const cs = a ? getComputedStyle(a) : null;
            return {
              label: a?.getAttribute('aria-label'),
              outline: cs ? `${cs.outlineStyle} ${cs.outlineWidth} ${cs.outlineColor}` : null,
              focusVisible: a?.matches(':focus-visible'),
            };
          });
          await page.keyboard.press('Tab');
          await page.keyboard.press('Space');
          await page.waitForTimeout(250);
          const after = await page.evaluate(() => {
            const a = document.activeElement as HTMLInputElement | null;
            return { label: a?.getAttribute('aria-label'), checked: a?.checked };
          });
          log('keyboard', s, { reached, ring, afterSpace: after });
          await shoot(page, 'members-keyboard', s);
        }
        // Labelled kit controls on user Settings pages (both widths).
        for (const pg of ['accessibility', 'profile', 'privacy', 'notifications', 'text']) {
          await page.goto(`/settings/user/${pg}`, {
            waitUntil: 'domcontentloaded',
            timeout: 15_000,
          });
          await idle(page, 3000);
          const m = await page.evaluate(measureChoices);
          log(`choices-${pg}`, s, m);
          if (pg === 'accessibility' && s.phone) await shoot(page, 'labelled-controls', s);
        }
      } catch (e) {
        console.log(`REPORT FAILED ${s.key}: ${String(e).slice(0, 300)}`);
        throw e;
      } finally {
        await ctx.close().catch(() => undefined);
      }
    }
  } finally {
    await ownerCtx.close();
  }
});

// ---------------------------------------------------------------- FileDrop (item 3)

function measureDrop() {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const or = Array.from(document.querySelectorAll<HTMLElement>('span')).find((e) =>
    /^or drop (a file|files) here$/.test((e.textContent ?? '').trim()),
  );
  if (!or) return { found: false };
  or.scrollIntoView({ block: 'center' });
  const drop = or.parentElement!;
  const btn = drop.querySelector('button');
  const bx = (e: Element | null) => {
    if (!e) return null;
    const b = e.getBoundingClientRect();
    return [r1(b.left), r1(b.top), r1(b.width), r1(b.height)];
  };
  return {
    found: true,
    text: (or.textContent ?? '').trim(),
    dropBox: bx(drop),
    buttonBox: bx(btn),
    textBox: bx(or),
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  };
}

test('SEC-AUD-396: FileDrop on Admin Entrance and the server Expressions page', async ({
  browser,
}) => {
  test.skip(!PART.includes('drop'), 'part off');
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `${TAG}-drop-measure.json`);
  writeFileSync(jsonl, '');
  const w = world();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    for (const s of SETTINGS) {
      await setTheme(ownerCtx.request, s.theme);
      const { ctx, page } = await newPage(browser, s, ownerCtx);
      try {
        await page.goto('/admin/entrance', { waitUntil: 'domcontentloaded', timeout: 20_000 });
        await page.locator('#ent-hero-title').waitFor({ timeout: 30_000 });
        await page.waitForTimeout(1500);
        const a = await page.evaluate(measureDrop);
        appendFileSync(jsonl, `${JSON.stringify({ id: 'entrance', setting: s.key, a })}\n`);
        console.log(`REPORT ${TAG} ${s.key} entrance ${JSON.stringify(a)}`);
        await page.screenshot({ path: resolve(OUT, `${TAG}-drop-entrance-${s.key}.png`) });

        await page.goto(`/settings/server/${w.guildId}`, {
          waitUntil: 'domcontentloaded',
          timeout: 15_000,
        });
        await idle(page, 4000);
        const labels = await page
          .locator(NAV)
          .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
        const i = labels.findIndex((l) => /^Expressions$|^Emoji/i.test(l));
        if (i < 0) throw new Error(`no nav item Expressions: ${labels.join(' | ')}`);
        await page
          .locator(NAV)
          .nth(i)
          .evaluate((el) => (el as HTMLElement).click());
        await idle(page, 2500);
        const e = await page.evaluate(measureDrop);
        appendFileSync(jsonl, `${JSON.stringify({ id: 'expressions', setting: s.key, e })}\n`);
        console.log(`REPORT ${TAG} ${s.key} expressions ${JSON.stringify(e)}`);
        await page.screenshot({ path: resolve(OUT, `${TAG}-drop-expressions-${s.key}.png`) });
      } finally {
        await ctx.close().catch(() => undefined);
      }
    }
  } finally {
    await ownerCtx.close();
  }
});
