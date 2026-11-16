import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { deflateSync } from 'node:zlib';
import type { Browser, BrowserContext, Page } from '@playwright/test';
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
 * SEC-AUD-337 / 338 (wave 82; D-841, D-842), on the real stack.
 *  - 337: no Developers page scrolls sideways at 375 or 1280. A helper measures `scrollWidth - clientWidth` of the document
 *    and, when over 0, lists the elements whose right edge is past the viewport (deepest first). Every page, empty and with
 *    content (the Activities tab with two mapping rows, one with a 60-character target, and a bundle listed).
 *  - 338: a phone list row reads. Rich Presence (a dashed 20-character name, a 32-letter name) and Emojis at 375: the name's
 *    lines (one Range per line) and where it breaks, the kind label's lines, the action's box.
 *  - Finding: the top navigation at 375 (scrollWidth, touch scroll, mask, the selected item in view).
 * SR_P82_BASE=1: the base case (developers.css unchanged): everything is logged, nothing asserted. Without it the rules
 * are asserted. Boxes of the Activities tab and of the list rows are written to OUT/boxes-<phase>.json and, on the "after"
 * run, compared with OUT/../p82-developers-base/boxes-base.json (1280: at most 2 px).
 * Local only (SR_P82_DEVELOPERS=1); shots go to SR_P82_DEVELOPERS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const BASE = process.env.SR_P82_BASE === '1';
const OUT =
  process.env.SR_P82_DEVELOPERS_OUT ??
  `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-developers${BASE ? '-base' : ''}`;
const BASE_BOXES =
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-developers-base/boxes-base.json';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const THEMES = ['lounge-dark', 'lounge-light'] as const;

test.skip(process.env.SR_P82_DEVELOPERS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number };
const log = (line: string) => console.log(`REPORT ${line}`);
let shots = 0;
let opened = 0;
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

/** A stored (uncompressed) zip with the given files. */
function zip(files: Record<string, string>): Buffer {
  const locals: Buffer[] = [];
  const centrals: Buffer[] = [];
  let off = 0;
  for (const [name, text] of Object.entries(files)) {
    const nm = Buffer.from(name);
    const data = Buffer.from(text);
    const crc = crc32(data);
    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0);
    lh.writeUInt16LE(20, 4);
    lh.writeUInt32LE(crc, 14);
    lh.writeUInt32LE(data.length, 18);
    lh.writeUInt32LE(data.length, 22);
    lh.writeUInt16LE(nm.length, 26);
    locals.push(lh, nm, data);
    const ch = Buffer.alloc(46);
    ch.writeUInt32LE(0x02014b50, 0);
    ch.writeUInt16LE(20, 4);
    ch.writeUInt16LE(20, 6);
    ch.writeUInt32LE(crc, 16);
    ch.writeUInt32LE(data.length, 20);
    ch.writeUInt32LE(data.length, 24);
    ch.writeUInt16LE(nm.length, 28);
    ch.writeUInt32LE(off, 42);
    centrals.push(ch, nm);
    off += 30 + nm.length + data.length;
  }
  const cd = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(Object.keys(files).length, 8);
  end.writeUInt16LE(Object.keys(files).length, 10);
  end.writeUInt32LE(cd.length, 12);
  end.writeUInt32LE(off, 16);
  return Buffer.concat([...locals, cd, end]);
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
  if (BASE || !before || !key.includes('desktop')) return;
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

/** The top navigation: strip, mask, the selected item, and (touch) a real scroll gesture. */
async function navLook(page: Page, tag: string, gesture: boolean) {
  const look = () =>
    page.evaluate(() => {
      const nav = document.querySelector<HTMLElement>('.dev-nav')!;
      const nb = nav.getBoundingClientRect();
      const cur = nav.querySelector<HTMLElement>("[aria-current='page']");
      const cb = cur?.getBoundingClientRect();
      const links = Array.from(nav.querySelectorAll<HTMLElement>('a')).map((a) => {
        const b = a.getBoundingClientRect();
        return `${(a.textContent ?? '').trim()}:${Math.round(b.left)}..${Math.round(b.right)}`;
      });
      return {
        scrollWidth: nav.scrollWidth,
        clientWidth: nav.clientWidth,
        scrollLeft: Math.round(nav.scrollLeft),
        attr: nav.getAttribute('data-overflow'),
        mask: getComputedStyle(nav).maskImage.slice(0, 60),
        navLeft: Math.round(nb.left),
        navRight: Math.round(nb.right),
        current: cur?.textContent?.trim() ?? null,
        curLeft: cb ? Math.round(cb.left) : null,
        curRight: cb ? Math.round(cb.right) : null,
        links,
      };
    });
  log(`nav ${tag} load ${JSON.stringify(await look())}`);
  if (gesture) {
    const cdp = await page.context().newCDPSession(page);
    const nb = await page.locator('.dev-nav').boundingBox();
    if (nb) {
      const y = nb.y + nb.height / 2;
      const x0 = nb.x + nb.width - 20;
      await cdp.send('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: [{ x: x0, y }],
      });
      for (let i = 1; i <= 10; i++) {
        await cdp.send('Input.dispatchTouchEvent', {
          type: 'touchMove',
          touchPoints: [{ x: x0 - i * 20, y }],
        });
        await page.waitForTimeout(16);
      }
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await page.waitForTimeout(500);
      log(`nav ${tag} after touch swipe ${JSON.stringify(await look())}`);
    }
  }
}

const APP_TABS = [
  'general',
  'oauth2',
  'bot',
  'installation',
  'commands',
  'emojis',
  'richpresence',
  'activities',
  'review',
];
const DOCS = [
  'getting-started',
  'authentication',
  'gateway',
  'rest',
  'rate-limits',
  'interactions',
  'api-reference',
  'gateway-events',
  'packages',
  'sdks',
  'porting',
];

test('Developers pages hold their width and their list rows read', async ({ browser }) => {
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
  await reach('assets', async () => {
    await apiPost(o, `/applications/${fullApp}/assets`, {
      name: 'art-desktop-mwjftvwy',
      type: 2,
      image: img(64, [60, 120, 200]),
    });
    await apiPost(o, `/applications/${fullApp}/assets`, {
      name: letters,
      type: 1,
      image: img(64, [200, 120, 60]),
    });
    await apiPost(o, `/applications/${fullApp}/assets`, {
      name: `gone-${stamp}`,
      type: 1,
      image: img(64, [90, 90, 90]),
    });
    for (const n of [`seed_${stamp}`, `second_emoji_${stamp}`.slice(0, 32)])
      await apiPost(o, `/applications/${fullApp}/emojis`, {
        name: n,
        image: img(64, [200, 80, 40]),
      });
  });
  await reach('mappings', async () => {
    await apiPut(o, `/applications/${fullApp}/activity`, {
      dev_entry_url: null,
      url_mappings: [
        { prefix: '/api', target: 'https://example.com/v1' },
        { prefix: '/proxy', target: `https://${'a'.repeat(44)}.example.com/some/path` },
      ],
    });
  });
  log(`world empty=${emptyApp} full=${fullApp} base=${BASE}`);
  const longPath = `assets/${'deeply-nested-folder-name/'.repeat(3)}bundle-${'x'.repeat(40)}.js`;
  const bundleZip = zip({
    'index.html': '<!doctype html><title>x</title><p>hi</p>',
    [longPath]: 'console.log(1);',
  });

  const prefsList: Prefs[] = THEMES.map((theme) => ({ theme, zoom: 100 }));
  const widths: Record<string, number> = {};
  for (const prefs of prefsList) {
    const short = prefs.theme.replace('lounge-', '');
    await setPrefs(prefs);
    for (const [sname, size] of Object.entries(SIZES)) {
      const phone = sname === 'phone';
      const tag = `${short}-${sname}`;
      const c = await context(browser, octx, size, prefs, phone);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const wide = async (name: string) => {
        widths[`${name}-${tag}`] = await width(page, `${name}-${tag}`);
      };

      // Every page, empty (Dark both sizes; Light too, they are cheap).
      const pages: [string, string][] = [
        ['overview', '/developers'],
        ['applications', '/developers/applications'],
        ['packages', '/developers/packages'],
        ...APP_TABS.map(
          (t) =>
            [`empty-${t}`, `/developers/applications/${emptyApp}?tab=${t}`] as [string, string],
        ),
        ...(prefs.theme === 'lounge-dark'
          ? DOCS.map((d) => [`doc-${d}`, `/developers/docs/${d}`] as [string, string])
          : []),
      ];
      for (const [name, url] of pages) {
        await reach(`${name}-${tag}`, async () => {
          await page.goto(url);
          await page.waitForTimeout(500);
          await wide(name);
        });
      }

      // The Activities tab with content: two mapping rows, then a bundle (a real upload of a stored zip).
      await reach(`activities-full-${tag}`, async () => {
        await page.goto(`/developers/applications/${fullApp}?tab=activities`);
        await expect(page.locator('.dev-uri-list li').first()).toBeVisible();
        log(`theme ${tag} ${await themeOf(page)}`);
        await wide('activities-mappings');
        const card = '.dev-main .dev-card';
        diffBoxes(`activities-mappings-${tag}`, await boxes(page, card));
        await page.locator('.dev-uri-list').first().scrollIntoViewIfNeeded();
        const lis = await page.evaluate(() =>
          Array.from(document.querySelectorAll('.dev-main .dev-uri-list li')).map((li) => {
            const lb = li.getBoundingClientRect();
            return Array.from(li.children).map((k) => {
              const b = k.getBoundingClientRect();
              return `${k.tagName.toLowerCase()} w=${Math.round(b.width)} x=${Math.round(b.left - lb.left)} right-gap=${Math.round(lb.right - b.right)}`;
            });
          }),
        );
        log(`mapping rows ${tag} ${JSON.stringify(lis)}`);
        if (!BASE && phone) {
          for (const row of lis)
            for (const k of row.slice(0, 2)) {
              const w = Number(/w=(\d+)/.exec(k)![1]);
              expect
                .soft(w, `${tag}: a mapping field is at least 160px wide`)
                .toBeGreaterThanOrEqual(160);
            }
        }
        if (!(await page.locator('.dev-bundle').count())) {
          await page.locator('input[data-testid=bundle-input]').setInputFiles({
            name: 'game.zip',
            mimeType: 'application/zip',
            buffer: bundleZip,
          });
          await expect(page.locator('.dev-bundle').first()).toBeVisible({ timeout: 20_000 });
        }
        await page.waitForTimeout(300);
        await wide('activities-bundle');
        if (await page.getByRole('button', { name: 'Show files' }).count())
          await page.getByRole('button', { name: 'Show files' }).first().click();
        await page.waitForTimeout(500);
        await wide('activities-bundle-files');
        diffBoxes(`activities-bundle-${tag}`, await boxes(page, '.dev-main .dev-card'));
        await page.locator('.dev-bundle-list').scrollIntoViewIfNeeded();
        await shoot(page, `${BASE ? 'base' : 'after'}-activities-bundle-${tag}`);
        await page.locator('.dev-uri-list').first().scrollIntoViewIfNeeded();
        await shoot(page, `${BASE ? 'base' : 'after'}-activities-mappings-${tag}`);
      });

      // The list rows (D-842).
      for (const [name, tab] of [
        ['richpresence', 'richpresence'],
        ['emojis', 'emojis'],
      ] as const) {
        await reach(`list-${name}-${tag}`, async () => {
          await page.goto(`/developers/applications/${fullApp}?tab=${tab}`);
          await expect(page.locator('.dev-main .dev-uri-list li').first()).toBeVisible();
          await page.waitForTimeout(400);
          await wide(`list-${name}`);
          const rows = await listRows(page, `${name}-${tag}`);
          diffBoxes(`list-${name}-${tag}`, await boxes(page, '.dev-main .dev-card'));
          if (phone && !BASE) {
            for (const r of rows) {
              if (r.kindLines)
                expect.soft(r.kindLines.length, `${tag}: kind label on one line`).toBe(1);
              if (r.action)
                expect.soft(r.action.r, `${tag}: action at the row's end`).toBeLessThanOrEqual(2);
              expect.soft(r.overCard, `${tag}: nothing past the card`).toBeLessThanOrEqual(0);
            }
          }
          await page.locator('.dev-main .dev-uri-list').first().scrollIntoViewIfNeeded();
          await shoot(page, `${BASE ? 'base' : 'after'}-list-${name}-${tag}`);
        });
      }

      // The redirect URI rows (unchanged) and the top navigation.
      await reach(`oauth2-rows-${tag}`, async () => {
        await page.goto(`/developers/applications/${emptyApp}?tab=oauth2`);
        await page.waitForTimeout(400);
        diffBoxes(`oauth2-${tag}`, await boxes(page, '.dev-main .dev-card'));
      });
      if (phone && prefs.theme === 'lounge-dark') {
        for (const [name, url] of [
          ['overview', '/developers'],
          ['applications', '/developers/applications'],
          ['packages', '/developers/packages'],
          ['docs', '/developers/docs/getting-started'],
          ['app', `/developers/applications/${fullApp}?tab=general`],
        ] as const) {
          await reach(`nav-${name}`, async () => {
            await page.goto(url);
            await page.waitForTimeout(600);
            await navLook(page, name, name === 'overview' || name === 'docs');
            if (name === 'overview' || name === 'docs')
              await page.screenshot({
                path: resolve(OUT, `nav-${name}-after-swipe-${tag}.png`),
                clip: { x: 0, y: 0, width: 375, height: 160 },
              });
            await page.goto(url);
            await page.waitForTimeout(600);
            if (name === 'overview' || name === 'docs')
              await page.screenshot({
                path: resolve(OUT, `nav-${name}-load-${tag}.png`),
                clip: { x: 0, y: 0, width: 375, height: 160 },
              });
            shots += name === 'overview' || name === 'docs' ? 2 : 0;
          });
        }
      }

      // Click-through (Light, 375): add a mapping, type in both fields, remove it; delete an asset.
      if (phone && prefs.theme === 'lounge-light' && !BASE) {
        await reach(`clickthrough-${tag}`, async () => {
          await page.goto(`/developers/applications/${fullApp}?tab=activities`);
          const items = page.locator('.dev-main .dev-uri-list li');
          await expect(items.first()).toBeVisible();
          const n0 = await items.count();
          await page
            .getByRole('button', { name: 'Add a mapping' })
            .click()
            .catch(async () => {
              await page.getByRole('button', { name: /^Add/ }).first().click();
            });
          await expect(items).toHaveCount(n0 + 1);
          const row = items.last();
          await row.locator('input').nth(0).fill('/new');
          await row
            .locator('input')
            .nth(1)
            .fill(`https://${'b'.repeat(46)}.example.org/x`);
          await row.scrollIntoViewIfNeeded();
          await shoot(page, `click-mapping-added-${tag}`);
          const rb = await row.evaluate((li) => {
            const lb = li.getBoundingClientRect();
            return Array.from(li.children).map((k) => {
              const b = k.getBoundingClientRect();
              return { w: Math.round(b.width), rightGap: Math.round(lb.right - b.right) };
            });
          });
          log(`click mapping row ${JSON.stringify(rb)}`);
          for (const k of rb.slice(0, 2)) expect.soft(k.w).toBeGreaterThanOrEqual(160);
          await row.getByRole('button', { name: /^Remove/ }).click();
          await expect(items).toHaveCount(n0);
          log(`click: mapping added, typed in both fields, removed (${n0} rows)`);
          await width(page, `click-after-remove-${tag}`);

          await page.goto(`/developers/applications/${fullApp}?tab=richpresence`);
          const names = page.locator('.dev-main .dev-uri-list li');
          await expect(names.first()).toBeVisible();
          const before = await names.count();
          await page.getByRole('button', { name: new RegExp(`gone-${stamp}`) }).click();
          await expect(names).toHaveCount(before - 1);
          log(`click: asset gone-${stamp} deleted (${before} -> ${before - 1})`);
          await shoot(page, `click-asset-deleted-${tag}`);
        });
      }
      await c.close();
    }
  }

  writeFileSync(resolve(OUT, `boxes-${BASE ? 'base' : 'after'}.json`), JSON.stringify(boxLog));
  writeFileSync(
    resolve(OUT, `widths-${BASE ? 'base' : 'after'}.json`),
    JSON.stringify(widths, null, 1),
  );
  log(`SUMMARY shots=${shots} opened=${opened}`);
  await octx.close();
});
