import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-341 (D-845), on the real stack: the Poll builder's and the Audio tool's checkboxes.
 * For each tool, theme (Lounge Dark, Lounge Light, Editorial) and size (1280x800, 375x812): the tool is opened from
 * the tools drawer by the option's own name; every checkbox's box, its label text's box (a Range), the gap between
 * them, the computed accent-colour (against `--ui-accent` resolved), and the outline after Tab (a keyboard focus).
 * The Poll form's controls (inputs, selects, buttons) are logged to `controls-<phase>-<tag>.json`; an `after` run
 * compares them with the `base` run's file. SR_P82_PHASE=base|after (the base run is made with rooms.css unchanged).
 * Local only (SR_P82_TOOLS=1). Shots and `measurements.txt` go to SR_P82_TOOLS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P82_TOOLS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-tools';
const PHASE = process.env.SR_P82_PHASE ?? 'after';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P82_TOOLS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

const note = (text: string) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(`REPORT ${text}`);
};

type Size = { width: number; height: number };
type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };
type Box = [string, number, number, number, number];

async function context(browser: Browser, base: BrowserContext, size: Size, prefs: Prefs) {
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

const prefsFor = (theme: string): Prefs => ({
  theme,
  zoom: 100,
  sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'lounge-editorial' },
});

const themeOf = (page: Page) =>
  page.evaluate(
    () =>
      document.querySelector<HTMLElement>('.lng-root')?.dataset.theme ??
      document.documentElement.dataset.theme ??
      '?',
  );

/** Open a tool from the drawer by the option's own accessible name; returns that name. */
async function openTool(page: Page, query: string, pick: RegExp): Promise<string> {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  const opt = page.getByRole('option', { name: pick }).first();
  const name = ((await opt.getAttribute('aria-label')) ?? (await opt.innerText())).replace(
    /\s+/g,
    ' ',
  );
  await opt.click();
  return name;
}

function toneWav(seconds: number): Buffer {
  const rate = 8000;
  const frames = seconds * rate;
  const b = Buffer.alloc(44 + frames * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + frames * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(rate, 24);
  b.writeUInt32LE(rate * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(frames * 2, 40);
  for (let i = 0; i < frames; i++)
    b.writeInt16LE(Math.round(8000 * Math.sin((2 * Math.PI * 440 * i) / rate)), 44 + i * 2);
  return b;
}

interface Row {
  kind: string;
  label: string;
  w: number;
  h: number;
  dy: number;
  gap: number;
  accent: string;
  accentWanted: string;
  outline: string;
  focused: boolean;
}

/** Every visible checkbox and radio of the open tool, measured (the outline is meaningful when it has focus). */
const checks = (page: Page): Promise<Row[]> =>
  page.evaluate(() => {
    const resolveColor = (v: string) => {
      const s = document.createElement('span');
      s.style.color = v;
      document.body.appendChild(s);
      const c = getComputedStyle(s).color;
      s.remove();
      return c;
    };
    return Array.from(
      document.querySelectorAll<HTMLInputElement>(
        '[role=dialog] input[type=checkbox], [role=dialog] input[type=radio]',
      ),
    )
      .filter((i) => i.getBoundingClientRect().width > 0)
      .map((i) => {
        const ib = i.getBoundingClientRect();
        const lab = i.closest('label');
        const span = lab?.querySelector('span');
        let tb: DOMRect | null = null;
        if (span) {
          const range = document.createRange();
          range.selectNodeContents(span);
          tb = Array.from(range.getClientRects()).filter((r) => r.width > 0)[0] ?? null;
        }
        const cs = getComputedStyle(i);
        const wanted = cs.getPropertyValue('--ui-accent').trim();
        return {
          kind: i.type,
          label: (lab?.textContent ?? '').trim().slice(0, 30),
          w: ib.width,
          h: ib.height,
          dy: tb ? ib.top + ib.height / 2 - (tb.top + tb.height / 2) : NaN,
          gap: tb ? tb.left - ib.right : NaN,
          accent: cs.accentColor === 'auto' ? 'auto' : resolveColor(cs.accentColor),
          accentWanted: wanted ? resolveColor(wanted) : '?',
          outline: `${cs.outlineStyle} ${cs.outlineWidth} ${resolveColor(cs.outlineColor)} offset ${cs.outlineOffset}`,
          focused: document.activeElement === i,
        };
      });
  });

/** Every input, select and button of the open tool: its box (for a before/after comparison). */
const controls = (page: Page): Promise<Box[]> =>
  page.evaluate(() =>
    Array.from(
      document.querySelectorAll<HTMLElement>(
        '[role=dialog] input, [role=dialog] select, [role=dialog] button, [role=dialog] textarea',
      ),
    )
      .filter((e) => e.getBoundingClientRect().width > 0)
      .map((e) => {
        const r = e.getBoundingClientRect();
        const label =
          e.getAttribute('aria-label') ?? e.closest('label')?.textContent ?? e.textContent ?? '';
        const type = (e as HTMLInputElement).type;
        return [
          `${e.tagName.toLowerCase()}${type ? ':' + type : ''} ${label.trim().slice(0, 24)}`,
          Math.round(r.left * 10) / 10,
          Math.round(r.top * 10) / 10,
          Math.round(r.width * 10) / 10,
          Math.round(r.height * 10) / 10,
        ] as Box;
      }),
  );

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

async function openPoll(page: Page, guildId: string, channelId: string, theme: string) {
  await page.goto(`/channels/${guildId}/${channelId}`);
  await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
  expect(await themeOf(page), 'the room wears the chosen theme').toBe(theme);
  const name = await openTool(page, 'poll', /poll/i);
  const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
  await page.waitForTimeout(900);
  await drawer.getByRole('button', { name: 'Start a new one' }).click();
  await expect(drawer.locator('.sr-room-check input')).toBeVisible({ timeout: 15_000 });
  return { name, drawer };
}

function reportRows(tag: string, rows: Row[]) {
  for (const r of rows)
    note(
      `${PHASE.toUpperCase()} ${tag} ${r.kind} "${r.label}": ${r.w.toFixed(1)} x ${r.h.toFixed(1)}, dy ${r.dy.toFixed(1)}, gap ${r.gap.toFixed(1)}, accent ${r.accent} (theme ${r.accentWanted}), outline [${r.outline}]${r.focused ? ' FOCUSED' : ''}`,
    );
}

const whole = (r: Row, needGap: boolean) =>
  r.w <= 20 &&
  r.h <= 20 &&
  Math.abs(r.dy) <= 3 &&
  (!needGap || r.gap >= 6) &&
  r.accent === r.accentWanted &&
  /solid 2px/.test(r.outline) &&
  r.focused;

const VARIANTS: Array<[string, string, Size]> = [
  ['lounge-dark', 'desktop', DESKTOP],
  ['lounge-dark', 'phone', PHONE],
  ['lounge-light', 'desktop', DESKTOP],
  ['lounge-light', 'phone', PHONE],
  ['lounge-editorial', 'desktop', DESKTOP],
  ['lounge-editorial', 'phone', PHONE],
];

test('the Poll builder, the Audio tool and a room tool: checkboxes measured and shot', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    octx.request,
    '/guilds',
    { name: `P82 tools ${Date.now().toString(36)}` },
  );
  const channelId = guild.channels.find((c) => c.type === 0)!.id;
  let shots = 0;
  const bad: string[] = [];
  const moved: string[] = [];

  for (const [theme, vp, size] of VARIANTS) {
    if (!new RegExp(process.env.SR_P82_ONLY ?? '.').test(`${theme}-${vp}`)) continue;
    const prefs = prefsFor(theme);
    await apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: prefs } });
    const c = await context(browser, octx, size, prefs);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    const errors: string[] = [];
    page.on('pageerror', (e) => errors.push(e.message));

    // ---- the Poll builder
    const ptag = `poll-${theme}-${vp}`;
    try {
      const { name, drawer } = await openPoll(page, guild.id, channelId, theme);
      note(`${ptag}: option picked by its name: "${name}"`);
      const ctl = await controls(page);
      writeFileSync(resolve(OUT, `controls-${PHASE}-${ptag}.json`), JSON.stringify(ctl));
      const baseFile = resolve(OUT, `controls-base-${ptag}.json`);
      const box = drawer.locator('.sr-room-check input');
      await box.click(); // tick with the pointer
      expect(await box.isChecked()).toBe(true);
      // keyboard focus: the select before it, then Tab
      await drawer.locator('.sr-room-inline select').focus();
      await page.keyboard.press('Tab');
      const rows = await checks(page);
      reportRows(`${ptag} ticked`, rows);
      if (PHASE === 'after')
        for (const r of rows) if (!whole(r, true)) bad.push(`${ptag} "${r.label}"`);
      note(`${ptag}: sideways overflow ${await overflow(page)}`);
      await box.scrollIntoViewIfNeeded();
      await page.screenshot({ path: resolve(OUT, `poll-${theme}-${vp}-${PHASE}.png`) });
      shots++;
      if (PHASE === 'after' && existsSync(baseFile)) {
        const before = JSON.parse(readFileSync(baseFile, 'utf8')) as Box[];
        const diff: string[] = [];
        ctl.forEach((a, i) => {
          const b = before[i];
          if (!b || JSON.stringify(a) !== JSON.stringify(b))
            diff.push(`${a[0]} now ${a.slice(1)} was ${b ? b.slice(1) : 'none'}`);
        });
        if (before.length !== ctl.length) diff.push(`count ${before.length} -> ${ctl.length}`);
        note(
          `CONTROLS ${ptag}: ${diff.length ? diff.join(' | ') : `all ${ctl.length} boxes equal to the base`}`,
        );
        if (diff.length) moved.push(ptag);
      }
      // click-through (Lounge Light, both sizes): pointer and Space, tick and untick (it starts ticked)
      if (theme === 'lounge-light') {
        await box.click();
        const a = await box.isChecked();
        await box.click();
        const b = await box.isChecked();
        await drawer.locator('.sr-room-inline select').focus();
        await page.keyboard.press('Tab');
        await page.keyboard.press('Space');
        const s1 = await box.isChecked();
        await page.keyboard.press('Space');
        const s2 = await box.isChecked();
        note(`click-through ${ptag}: pointer ->${a} ->${b}; Space ->${s1} ->${s2}`);
        if (a || !b || s1 || !s2) bad.push(`${ptag}: click-through`);
      }
    } catch (e) {
      note(`UNREACHED ${ptag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      bad.push(`${ptag}: unreached`);
    }

    // ---- the Audio tool
    const atag = `audio-${theme}-${vp}`;
    try {
      await page.goto(`/channels/${guild.id}/${channelId}`);
      await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
      const name = await openTool(page, 'audio', /audio/i);
      note(`${atag}: option picked by its name: "${name}"`);
      const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
      await page.waitForTimeout(900);
      await drawer
        .getByLabel('Choose an audio file')
        .setInputFiles({ name: 'tone.wav', mimeType: 'audio/wav', buffer: toneWav(12) });
      const box = drawer.locator('.utl-aud-check input');
      await expect(box).toBeVisible({ timeout: 15_000 });
      await box.click();
      await drawer.locator('.utl-aud-field input[type=number]').last().focus();
      await page.keyboard.press('Tab');
      const rows = await checks(page);
      reportRows(atag, rows);
      if (PHASE === 'after')
        for (const r of rows) if (!whole(r, false)) bad.push(`${atag} "${r.label}"`);
      note(`${atag}: sideways overflow ${await overflow(page)}`);
      await box.scrollIntoViewIfNeeded();
      await page.screenshot({ path: resolve(OUT, `audio-${theme}-${vp}-${PHASE}.png`) });
      shots++;
    } catch (e) {
      note(`UNREACHED ${atag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      bad.push(`${atag}: unreached`);
    }

    // ---- one other room tool (the tier list), Lounge Light
    if (theme === 'lounge-light') {
      try {
        await page.goto(`/channels/${guild.id}/${channelId}`);
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        const name = await openTool(page, 'tier', /tier list/i);
        const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
        await page.waitForTimeout(900);
        await drawer.getByRole('button', { name: 'Start a new one' }).click();
        await expect(drawer.getByLabel('Row name').first()).toBeVisible({ timeout: 15_000 });
        const n = await page.evaluate(
          () =>
            document.querySelectorAll('[role=dialog] .sr-room-check, [role=dialog] .utl-aud-check')
              .length,
        );
        note(
          `tierlist-${theme}-${vp}: option "${name}"; label-class users ${n}; overflow ${await overflow(page)}`,
        );
        await page.screenshot({ path: resolve(OUT, `tierlist-${theme}-${vp}-${PHASE}.png`) });
        shots++;
      } catch (e) {
        note(
          `UNREACHED tierlist-${theme}-${vp}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`,
        );
      }
    }
    if (errors.length) note(`page errors ${theme}-${vp}: ${errors.join('; ')}`);
    await c.close();
  }
  note(
    `${PHASE}: ${shots} shots; checks outside the target: ${bad.length ? bad.join('; ') : 'none'}; controls moved: ${moved.length ? moved.join(',') : 'none'}`,
  );
  expect(bad, 'every box is whole').toEqual([]);
  expect(moved, 'no other control moved').toEqual([]);
  await octx.close();
});
