import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { deflateSync } from 'node:zlib';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-343 (wave 83; D-849), on the real stack: on a phone an asset row stacks (the kind under the name). Rich Presence at 375
 * (a dashed 20-letter name, a 32-letter name, a short gone-name) and Emojis (at rest and with one row renaming), the name's lines,
 * the kind's box against the name's, the action, the row; boxes of each list and of the OAuth2 redirect rows at both sizes are
 * written to OUT/boxes-<phase>.json and compared with the base run's. SR_P83_BASE=1: the base case (developers.css held back):
 * logged, nothing asserted. Local only (SR_P83_DEVELOPERS=1); shots go to SR_P83_DEVELOPERS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const BASE = process.env.SR_P83_BASE === '1';
const OUT =
  process.env.SR_P83_DEVELOPERS_OUT ??
  `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-developers${BASE ? '-base' : ''}`;
const BASE_BOXES =
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-developers-base/boxes-base.json';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const THEMES = ['lounge-dark', 'lounge-light'] as const;

test.skip(process.env.SR_P83_DEVELOPERS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number };
const log = (line: string) => console.log(`REPORT ${line}`);
let shots = 0;
const boxLog: Record<string, unknown> = {};

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (b: Buffer) => {
  let c = 0xffffffff;
  for (const x of b) c = crcTable[(c ^ x) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};

/** A solid-colour PNG, built by hand (no dependency). */
function png(size: number, rgb: [number, number, number]): Buffer {
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, sum]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const row = Buffer.concat([Buffer.from([0]), Buffer.from(Array(size).fill(rgb).flat())]);
  const raw = Buffer.concat(Array(size).fill(row));
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
  touch = false,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    bypassCSP: true,
    hasTouch: touch,
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

async function reach(name: string, run: () => Promise<void>) {
  try {
    await run();
  } catch (e) {
    log(`UNREACHED ${name}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
  }
}

async function shoot(page: Page, name: string) {
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${name}.png`) });
  shots++;
}

const themeOf = (page: Page) =>
  page.evaluate(() => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null);

interface Off {
  node: string;
  l: number;
  r: number;
  w: number;
}
/** The document's sideways overflow and, when over 0, every element whose right edge is past the viewport (deepest first). */
async function width(page: Page, tag: string, assert = true): Promise<number> {
  const res = await page.evaluate(() => {
    const de = document.documentElement;
    const over = de.scrollWidth - de.clientWidth;
    const offenders: Off[] = [];
    if (over > 0) {
      const vw = de.clientWidth;
      const depth = (e: Element) => {
        let d = 0;
        for (let p: Element | null = e; p; p = p.parentElement) d++;
        return d;
      };
      const all = Array.from(document.body.querySelectorAll<HTMLElement>('*'))
        .map((e) => ({ e, b: e.getBoundingClientRect() }))
        .filter(({ b }) => b.width > 0 && b.right > vw + 0.5)
        // inside its own scroller (a code block, the phone nav): contained, not the page's overflow
        .filter(({ e }) => {
          for (let p = e.parentElement; p && p !== document.body; p = p.parentElement) {
            const ox = getComputedStyle(p).overflowX;
            if (ox !== 'visible' && p.getBoundingClientRect().right <= vw + 0.5) return false;
          }
          return true;
        })
        .sort((a, b) => depth(b.e) - depth(a.e));
      for (const { e, b } of all.slice(0, 14)) {
        const cls = typeof e.className === 'string' ? e.className.trim().replace(/\s+/g, '.') : '';
        offenders.push({
          node: `${e.tagName.toLowerCase()}${cls ? `.${cls.slice(0, 60)}` : ''} < ${e.parentElement?.tagName.toLowerCase()}.${String(e.parentElement?.className ?? '').slice(0, 40)} "${(e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 44)}"`,
          l: Math.round(b.left),
          r: Math.round(b.right),
          w: Math.round(b.width),
        });
      }
    }
    if (over > 0 && offenders.length === 0) {
      // Text that spills past its own box: a Range per text node.
      const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      for (let n = w.nextNode(); n; n = w.nextNode()) {
        const range = document.createRange();
        range.selectNodeContents(n);
        const rb = range.getBoundingClientRect();
        const p = n.parentElement!;
        let clipped = false;
        for (let q: HTMLElement | null = p; q && q !== document.body; q = q.parentElement) {
          if (
            getComputedStyle(q).overflowX !== 'visible' &&
            q.getBoundingClientRect().right <= de.clientWidth + 0.5
          )
            clipped = true;
        }
        if (!clipped && rb.right > de.clientWidth + 0.5)
          offenders.push({
            node: `TEXT in ${p.tagName.toLowerCase()}.${String(p.className).slice(0, 40)} < ${p.parentElement?.tagName.toLowerCase()}.${String(p.parentElement?.className).slice(0, 40)} "${(n.textContent ?? '').trim().slice(0, 40)}"`,
            l: Math.round(rb.left),
            r: Math.round(rb.right),
            w: Math.round(rb.width),
          });
      }
    }
    return { over, offenders };
  });
  log(`width ${tag}: ${res.over}`);
  if (res.over > 0) for (const o of res.offenders) log(`  over ${tag}: ${JSON.stringify(o)}`);
  if (assert && !BASE)
    expect.soft(res.over, `${tag}: document scrolls sideways`).toBeLessThanOrEqual(0);
  return res.over;
}

/** Every box under the Activities card, keyed by an index and its tag/class (for the 1280 before/after diff). */
async function boxes(page: Page, sel: string) {
  return page.evaluate((s) => {
    const r = (n: number) => Math.round(n * 10) / 10;
    const root = document.querySelector(s);
    if (!root) return [];
    const rb = root.getBoundingClientRect();
    return Array.from(root.querySelectorAll<HTMLElement>('*'))
      .filter((e) => e.getBoundingClientRect().width > 0)
      .map((e, i) => {
        const b = e.getBoundingClientRect();
        const cls = typeof e.className === 'string' ? e.className.trim().replace(/\s+/g, '.') : '';
        return {
          k: `${i}:${e.tagName.toLowerCase()}${cls ? `.${cls.slice(0, 40)}` : ''}`,
          x: r(b.left - rb.left),
          y: r(b.top - rb.top),
          w: r(b.width),
          h: r(b.height),
        };
      });
  }, sel);
}

const baseBoxes: Record<string, { k: string; x: number; y: number; w: number; h: number }[]> =
  !BASE && existsSync(BASE_BOXES) ? JSON.parse(readFileSync(BASE_BOXES, 'utf8')) : {};

function diffBoxes(key: string, now: { k: string; x: number; y: number; w: number; h: number }[]) {
  boxLog[key] = now;
  const before = baseBoxes[key];
  if (BASE || !before) return;
  if (before.length !== now.length) {
    log(`boxes ${key}: count ${before.length} -> ${now.length}`);
  }
  let worst = 0;
  let which = '';
  for (let i = 0; i < Math.min(before.length, now.length); i++) {
    const a = before[i]!;
    const b = now[i]!;
    const d = Math.max(
      Math.abs(a.x - b.x),
      Math.abs(a.y - b.y),
      Math.abs(a.w - b.w),
      Math.abs(a.h - b.h),
    );
    if (d > worst) {
      worst = d;
      which = `${a.k} ${JSON.stringify(a)} -> ${JSON.stringify(b)}`;
    }
  }
  log(`boxes ${key}: worst move ${worst}px ${which}`);
  if (!(key.startsWith('list-richpresence') && key.includes('phone')))
    expect.soft(worst, `${key}: a box moved by more than 2px`).toBeLessThanOrEqual(2);
}

/** One list row: the name's lines and where it breaks, the kind label's lines, the action's box, all relative to the row. */
async function listRows(page: Page, tag: string) {
  const rows = await page.evaluate(() => {
    const r = (n: number) => Math.round(n * 10) / 10;
    const linesOf = (el: Element) => {
      const range = document.createRange();
      const tn: Text[] = [];
      const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      for (let n = w.nextNode(); n; n = w.nextNode()) tn.push(n as Text);
      const lines: string[] = [];
      let top = -1;
      let cur = '';
      for (const t of tn) {
        for (let i = 0; i < t.length; i++) {
          range.setStart(t, i);
          range.setEnd(t, i + 1);
          const rc = range.getClientRects()[0];
          if (!rc) continue;
          if (top >= 0 && Math.abs(rc.top - top) > 4) {
            lines.push(cur);
            cur = '';
          }
          top = rc.top;
          cur += t.data[i];
        }
      }
      if (cur) lines.push(cur);
      return lines;
    };
    return Array.from(document.querySelectorAll<HTMLElement>('.dev-main .dev-uri-list li'))
      .filter((li) => li.querySelector('.dev-media-row'))
      .map((li) => {
        const lb = li.getBoundingClientRect();
        const rel = (e: Element) => {
          const b = e.getBoundingClientRect();
          return {
            x: r(b.left - lb.left),
            r: r(lb.right - b.right),
            y: r(b.top - lb.top),
            w: r(b.width),
            h: r(b.height),
          };
        };
        const code = li.querySelector('code')!;
        const kind = li.querySelector('.dev-media-row .dev-muted');
        const last = li.lastElementChild!;
        const media = li.querySelector('.dev-media-row')!;
        const action = last === media ? null : last;
        const mb = code.getBoundingClientRect();
        return {
          li: r(lb.width),
          liH: r(lb.height),
          media: rel(media),
          code: rel(code),
          nameLines: linesOf(code),
          kindLines: kind ? linesOf(kind) : null,
          kind: kind ? rel(kind) : null,
          action: action ? rel(action) : null,
          actionSameLineAsName: action
            ? Math.abs(action.getBoundingClientRect().top - mb.top) < 24
            : null,
          overCard: r(
            Math.max(
              0,
              ...Array.from(li.querySelectorAll('*')).map((e) => e.getBoundingClientRect().right),
            ) - li.closest('.dev-card')!.getBoundingClientRect().right,
          ),
        };
      });
  });
  for (const row of rows) log(`listrow ${tag} ${JSON.stringify(row)}`);
  return rows;
}

test('A phone asset row: the kind under the name', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });
  const stamp = Date.now().toString(36);
  const mk = async (name: string) =>
    (
      await apiPost<{ application: { id: string } }>(o, '/applications', {
        name: `${name} ${stamp}`,
      })
    ).application.id;
  const emptyApp = await mk('Empty app');
  const fullApp = await mk('Full app');
  const img = (n: number, c: [number, number, number]) =>
    `data:image/png;base64,${png(n, c).toString('base64')}`;
  const letters = 'qwrtpsdfghjklzxcvbnmqwrtpsdfghjk'.slice(0, 32);
  const dashed = `art-desktop-${'mwjftvwy'}`;
  const short = `gone-${stamp.slice(0, 8).padEnd(8, 'x')}`;
  await apiPost(o, `/applications/${fullApp}/assets`, {
    name: dashed,
    type: 2,
    image: img(64, [60, 120, 200]),
  });
  await apiPost(o, `/applications/${fullApp}/assets`, {
    name: letters,
    type: 1,
    image: img(64, [200, 120, 60]),
  });
  await apiPost(o, `/applications/${fullApp}/assets`, {
    name: short,
    type: 2,
    image: img(64, [90, 90, 90]),
  });
  for (const n of [`seed_${stamp}`, `second_emoji_${stamp}`.slice(0, 32)])
    await apiPost(o, `/applications/${fullApp}/emojis`, { name: n, image: img(64, [200, 80, 40]) });
  log(`world empty=${emptyApp} full=${fullApp} base=${BASE} names=${dashed},${letters},${short}`);

  const phase = BASE ? 'base' : 'after';
  const prefsList: Prefs[] = THEMES.map((theme) => ({ theme, zoom: 100 }));
  for (const prefs of prefsList) {
    const th = prefs.theme.replace('lounge-', '');
    await setPrefs(prefs);
    for (const [sname, size] of Object.entries(SIZES)) {
      const phone = sname === 'phone';
      const tag = `${th}-${sname}`;
      const c = await context(browser, octx, size, prefs, phone);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);

      await reach(`richpresence-${tag}`, async () => {
        await page.goto(`/developers/applications/${fullApp}?tab=richpresence`);
        await expect(page.locator('.dev-main .dev-uri-list li').first()).toBeVisible();
        await page.waitForTimeout(400);
        log(`theme ${tag} ${await themeOf(page)}`);
        await width(page, `richpresence-${tag}`);
        const rows = await listRows(page, `richpresence-${tag}`);
        diffBoxes(`list-richpresence-${tag}`, await boxes(page, '.dev-main .dev-card'));
        const muted = await page.evaluate(
          () => getComputedStyle(document.querySelector('.dev-media-row .dev-muted')!).color,
        );
        const textc = await page.evaluate(
          () => getComputedStyle(document.querySelector('.dev-media-row code')!).color,
        );
        log(`colours ${tag} kind=${muted} name=${textc}`);
        if (phone && !BASE) {
          for (const r of rows) {
            const isLong = r.nameLines.join('') === letters;
            expect.soft(r.kindLines?.length, `${tag}: kind on one line`).toBe(1);
            expect.soft(r.overCard, `${tag}: nothing past the card`).toBeLessThanOrEqual(0);
            expect
              .soft(Math.abs(r.kind!.x - r.code.x), `${tag}: kind's left edge at the name's`)
              .toBeLessThanOrEqual(1);
            expect.soft(r.kind!.y, `${tag}: kind under the name`).toBeGreaterThan(r.code.y);
            if (isLong)
              expect.soft(r.nameLines.length, `${tag}: long name breaks`).toBeGreaterThan(1);
            else {
              expect.soft(r.nameLines.length, `${tag}: name on one line`).toBe(1);
              expect.soft(r.actionSameLineAsName, `${tag}: action on the name's line`).toBe(true);
              expect.soft(r.action!.r, `${tag}: action at the row's end`).toBeLessThanOrEqual(2);
            }
          }
        }
        await page.locator('.dev-main .dev-uri-list').first().scrollIntoViewIfNeeded();
        await shoot(page, `${phase}-richpresence-${tag}`);
      });

      await reach(`emojis-${tag}`, async () => {
        await page.goto(`/developers/applications/${fullApp}?tab=emojis`);
        await expect(page.locator('.dev-main .dev-uri-list li').first()).toBeVisible();
        await page.waitForTimeout(400);
        await width(page, `emojis-${tag}`);
        await listRows(page, `emojis-${tag}`);
        diffBoxes(`list-emojis-${tag}`, await boxes(page, '.dev-main .dev-card'));
        await page
          .getByRole('button', { name: /^Rename/ })
          .first()
          .click();
        await page.waitForTimeout(300);
        await width(page, `emojis-renaming-${tag}`);
        diffBoxes(`list-emojis-renaming-${tag}`, await boxes(page, '.dev-main .dev-card'));
        await page.locator('.dev-main .dev-uri-list').first().scrollIntoViewIfNeeded();
        await shoot(page, `${phase}-emojis-renaming-${tag}`);
      });

      await reach(`oauth2-${tag}`, async () => {
        await page.goto(`/developers/applications/${emptyApp}?tab=oauth2`);
        await expect(page.locator('.dev-main .dev-card').first()).toBeVisible();
        await page.waitForTimeout(900);
        diffBoxes(`oauth2-${tag}`, await boxes(page, '.dev-main .dev-card'));
      });

      if (phone && th === 'light' && !BASE) {
        await reach(`clickthrough-${tag}`, async () => {
          await page.goto(`/developers/applications/${fullApp}?tab=emojis`);
          const items = page.locator('.dev-main .dev-uri-list li');
          await expect(items.first()).toBeVisible();
          await page
            .getByRole('button', { name: /^Rename/ })
            .first()
            .click();
          const field = items.first().locator('input');
          await field.fill(`renamed_${stamp}`.slice(0, 32));
          await page.getByRole('button', { name: /^Save/ }).first().click();
          await expect(page.locator('.dev-main .dev-uri-list code').first()).toContainText(
            'renamed_',
          );
          log('click: emoji renamed and saved');
          await shoot(page, `click-emoji-renamed-${tag}`);
          await page.goto(`/developers/applications/${fullApp}?tab=richpresence`);
          const names = page.locator('.dev-main .dev-uri-list li');
          await expect(names.first()).toBeVisible();
          const before = await names.count();
          await page.getByRole('button', { name: new RegExp(short) }).click();
          await expect(names).toHaveCount(before - 1);
          log(`click: asset ${short} deleted (${before} -> ${before - 1})`);
          await shoot(page, `click-asset-deleted-${tag}`);
        });
      }
      await c.close();
    }
  }
  writeFileSync(resolve(OUT, `boxes-${phase}.json`), JSON.stringify(boxLog));
  log(`SUMMARY shots=${shots}`);
  await octx.close();
});
