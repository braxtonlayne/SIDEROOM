import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiDelete } from '../support/world';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 103, search (SEC-AUD-461, SEC-AUD-464; D-980, D-972): the Lounge search results' Sort select.
 *  - 461 (D-980): a select that re-runs a query on change keeps its focus and its value while the query runs. The
 *    keyboard sequences (ArrowDown / ArrowUp through every option; F4, ArrowUp, Enter) read, after each key
 *    and a settle, the select's value, whether it is the focused element, whether it is the SAME node (a marker set on
 *    it before the first key), and the first hit of the list (Newest and Oldest differ).
 *  - 464: `.lng-search-sort select` leaves msg.css; the select's box, the bar, its neighbours, the text clear of the
 *    chevron, and the hit test over the results panel, base then after.
 * Own data (D-942), named `p103s ...`, removed in a `finally`: two text channels and six messages posted one after the
 * other that all hold the word `p103sq<stamp>`. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; never edited). Nothing else is saved.
 *
 * Phases: `SR_P103_SEARCH_PHASE=base` records the base tree (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares.
 * Configs: 1024 Lounge Light (the keyboard reads, the shots), 1024 Lounge Dark, 375 touch Lounge Light (the phone's
 * search sheet; D-740). `data-theme` is read back before each shot.
 *
 * Local only (SR_P103_SEARCH_ON=1). SR_P103_SEARCH_OUT is the folder of the shots and the figures. From e2e/:
 *   SR_E2E_BASE_URL=http://localhost:5803 SR_P103_SEARCH_ON=1 SR_P103_SEARCH_PHASE=base \
 *   SR_P103_SEARCH_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-search \
 *   npx playwright test tests/p103-search-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *   (the same with SR_P103_SEARCH_PHASE=after)
 */
const ON = process.env.SR_P103_SEARCH_ON === '1';
const PHASE = process.env.SR_P103_SEARCH_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_SEARCH_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-search';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = Date.now().toString(36);
const WORD = `p103sq${STAMP}`;

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  main: string;
}
const CONFIGS: Cfg[] = [
  { tag: '1024-lounge-light', width: 1024, height: 768, touch: false, main: 'lounge-light' },
  { tag: '1024-lounge-dark', width: 1024, height: 768, touch: false, main: 'lounge-dark' },
  { tag: '375-lounge-light', width: 375, height: 812, touch: true, main: 'lounge-light' },
];

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Fig {
  select: Box | null;
  bar: Box | null;
  count: Box | null;
  textClear: boolean | null;
  textWidth: number | null;
  clientWidth: number | null;
  scroll: number[];
  theme: string | null;
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: { control: string; takers: string[] }[];
  keyboard?: Read[][];
}
interface Read {
  step: string;
  value: string | null;
  focused: boolean;
  sameNode: boolean;
  firstHit: string | null;
  status: string;
}

const goto = (page: Page, path: string) =>
  page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });

async function makeData(o: BrowserContext['request'], notes: string[]) {
  const w = world();
  const chans: string[] = [];
  for (const n of ['a', 'b']) {
    const c = await apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, {
      name: `p103s-${n}-${STAMP}`,
      type: 0,
    });
    chans.push(c.id);
  }
  // Six messages, one after the other: the newest is `m6`.
  for (let i = 1; i <= 6; i++) {
    for (let tries = 0; ; tries++) {
      try {
        await apiPost(o, `/channels/${chans[i % 2]}/messages`, {
          content: `${WORD} line m${i}`,
        });
        break;
      } catch (e) {
        if (tries > 8 || !String(e).includes('429')) throw e;
        await new Promise((r) => setTimeout(r, 1200));
      }
    }
    await new Promise((r) => setTimeout(r, 700));
  }
  notes.push(`channels ${chans.join(',')}`);
  return { guildId: w.guildId, home: chans[0]!, chans };
}

async function setTheme(owner: BrowserContext, c: Cfg) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: c.main, zoom: 100 } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(c.main);
}

/** The results panel with the Sort select in it, after a real search for the word. */
async function openSearch(page: Page, c: Cfg, ids: { guildId: string; home: string }) {
  await goto(page, `/channels/${ids.guildId}/${ids.home}`);
  if (c.touch) {
    await page.getByRole('button', { name: 'Search this conversation' }).click({ timeout: 40_000 });
  }
  const sb = page.getByRole('searchbox').first();
  await sb.waitFor({ timeout: 40_000 });
  const panel = page.getByRole('region', { name: 'Search results' });
  let count = '';
  for (let i = 0; i < 12; i++) {
    await sb.fill(WORD);
    await sb.press('Enter');
    await panel.getByRole('combobox', { name: 'Sort by' }).waitFor({ timeout: 25_000 });
    count = ((await panel.locator('.lng-search-count').first().textContent()) ?? '').trim();
    if (/^6 /.test(count)) break;
    await page.waitForTimeout(1000);
  }
  expect(count, 'the index holds the six messages').toMatch(/^6 /);
  await page.waitForTimeout(900);
  return panel;
}

const box = async (l: Locator): Promise<Box | null> => {
  if (!(await l.count())) return null;
  const b = await l.first().boundingBox();
  return b ? { x: b.x, y: b.y, w: b.width, h: b.height } : null;
};

async function read(page: Page, _panel: Locator, step: string): Promise<Read> {
  return page.evaluate(
    ({ step }) => {
      const sel = document.querySelector<HTMLSelectElement>('[aria-label="Search results"] select');
      const mark = document.querySelector('[data-p103-mark]');
      const hit = document.querySelector('.lng-search-hit');
      const st = document.querySelector('[aria-label="Search results"] [role=status]');
      return {
        step,
        value: sel ? sel.value : null,
        focused: !!sel && document.activeElement === sel,
        sameNode: !!sel && sel === mark,
        firstHit: hit ? (hit.textContent ?? '').replace(/\s+/g, ' ').slice(0, 60) : null,
        status: st ? (st.textContent ?? '').trim() : '',
      };
    },
    { step },
  );
}

const SETTLE = 2200;
const CFG = process.env.SR_P103_SEARCH_CFG ? new RegExp(process.env.SR_P103_SEARCH_CFG) : null;

async function keyboardReads(page: Page, panel: Locator): Promise<Read[][]> {
  const sel = panel.getByRole('combobox', { name: 'Sort by' });
  const out: Read[][] = [];
  // 1: the arrow keys, from Newest.
  await sel.evaluate((el) => el.setAttribute('data-p103-mark', '1'));
  await sel.focus();
  const seq1: Read[] = [await read(page, panel, 'start (Newest)')];
  for (const key of ['ArrowDown', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowUp', 'ArrowUp']) {
    await page.keyboard.press(key);
    await page.waitForTimeout(SETTLE);
    seq1.push(await read(page, panel, key));
    say(`seq1 ${JSON.stringify(seq1[seq1.length - 1])} panel ${await panel.isVisible()}`);
  }
  out.push(seq1);
  // back to Newest on the select if it is still the same node (a put-back for the second sequence)
  const sel2 = panel.getByRole('combobox', { name: 'Sort by' });
  await sel2.selectOption('new');
  await page.waitForTimeout(SETTLE);
  // 2: the native way: F4 opens the list (Alt+ArrowDown is the Lounge's next-channel shortcut), ArrowUp, Enter.
  await sel2.evaluate((el) => {
    document
      .querySelectorAll('[data-p103-mark]')
      .forEach((n) => n.removeAttribute('data-p103-mark'));
    el.setAttribute('data-p103-mark', '1');
  });
  await sel2.focus();
  const seq2: Read[] = [await read(page, panel, 'start (Newest)')];
  const alt = async (keys: string[], label: string) => {
    await page.keyboard.press('F4');
    await page.waitForTimeout(250);
    for (const k of keys) await page.keyboard.press(k);
    await page.waitForTimeout(SETTLE);
    seq2.push(await read(page, panel, label));
    say(`seq2 ${JSON.stringify(seq2[seq2.length - 1])} panel ${await panel.isVisible()}`);
  };
  await alt(['ArrowDown', 'Enter'], 'F4, ArrowDown, Enter');
  await alt(['ArrowUp', 'Enter'], 'F4, ArrowUp, Enter');
  await alt(['ArrowDown', 'ArrowDown', 'Enter'], 'F4, ArrowDown x2, Enter');
  await alt(['ArrowUp', 'ArrowUp', 'Enter'], 'F4, ArrowUp x2, Enter');
  out.push(seq2);
  // put it back for the shots
  await panel.getByRole('combobox', { name: 'Sort by' }).selectOption('new');
  await page.waitForTimeout(SETTLE);
  return out;
}

async function measure(page: Page, panel: Locator): Promise<Omit<Fig, 'hit' | 'hitFails'>> {
  const sel = panel.getByRole('combobox', { name: 'Sort by' });
  await sel.scrollIntoViewIfNeeded();
  const m = await sel.evaluate((el: HTMLSelectElement) => {
    const cs = getComputedStyle(el);
    const c = document.createElement('canvas').getContext('2d')!;
    c.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
    const text = el.selectedOptions[0]?.textContent ?? '';
    const textWidth = c.measureText(text).width;
    const padStart = parseFloat(cs.paddingInlineStart);
    const padEnd = parseFloat(cs.paddingInlineEnd);
    return {
      textWidth,
      clientWidth: el.clientWidth,
      textClear: textWidth + padStart + padEnd <= el.clientWidth + 0.5,
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    };
  });
  return {
    select: await box(sel),
    bar: await box(panel.locator('.lng-search-bar')),
    count: await box(panel.locator('.lng-search-count').first()),
    textClear: m.textClear,
    textWidth: m.textWidth,
    clientWidth: m.clientWidth,
    scroll: await page.evaluate(() => [
      document.documentElement.scrollWidth,
      document.documentElement.clientWidth,
    ]),
    theme: m.theme,
  };
}

async function hit(page: Page, panel: Locator) {
  await openCollapsed(page, panel);
  const rows: ControlRow[] = await hitTestAll(page, panel);
  const n = (s: string) => rows.filter((r) => r.status === s).length;
  return {
    hit: { pass: n('pass'), fail: n('fail'), notReached: n('not-reached'), skipped: n('skipped') },
    hitFails: rows
      .filter((r) => r.status === 'fail')
      .map((r) => ({
        control: `${r.name || r.tag} (${r.path})`,
        takers: (r.result?.points ?? []).flatMap((p) => (p.taker ? [JSON.stringify(p.taker)] : [])),
      })),
  };
}

test(`p103 search: the Sort select keeps focus and value; its box rule leaves msg.css, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const notes: string[] = [];
  const figs: Record<string, Fig> = {};
  const ids = await makeData(owner.request, notes);
  try {
    for (const c of CONFIGS) {
      if (CFG && !CFG.test(c.tag)) continue;
      await setTheme(owner, c);
      const ctx = await browser.newContext({
        baseURL: ORIGIN,
        viewport: { width: c.width, height: c.height },
        storageState: await owner.storageState(),
        bypassCSP: true,
        ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
      });
      await ctx.addInitScript(
        (p) => {
          try {
            localStorage.setItem(
              'sr.user.prefs',
              JSON.stringify({ state: { prefs: p }, version: 2 }),
            );
          } catch {
            /* no storage */
          }
        },
        { theme: c.main, zoom: 100 },
      );
      try {
        const page = await ctx.newPage();
        page.setDefaultTimeout(15_000);
        const panel = await openSearch(page, c, ids);
        const fig = {
          ...(await measure(page, panel)),
          ...(await hit(page, panel)),
        } as Fig;
        if (!c.touch && c.tag.includes('light')) fig.keyboard = await keyboardReads(page, panel);
        figs[c.tag] = fig;
        // the select reads as a whole
        expect(fig.theme, `data-theme of ${c.tag}`).toBe(c.main);
        if (fig.scroll[0]! > fig.scroll[1]!)
          problems.push(`${c.tag}: page scrolls sideways ${fig.scroll}`);
        if (fig.hit.notReached)
          problems.push(`${c.tag}: ${fig.hit.notReached} hit rows not reached`);
        await panel.getByRole('combobox', { name: 'Sort by' }).scrollIntoViewIfNeeded();
        await page.screenshot({ path: resolve(OUT, `${shotName(`search-${c.tag}`)}.png`) });
        if (c.tag === '1024-lounge-light') {
          // the select focused after an ArrowUp (the base loses focus; after keeps it)
          const sel = panel.getByRole('combobox', { name: 'Sort by' });
          await sel.focus();
          await page.keyboard.press('ArrowDown');
          await page.waitForTimeout(SETTLE);
          await page.keyboard.press('ArrowUp');
          await page.waitForTimeout(SETTLE);
          await page.screenshot({ path: resolve(OUT, `${shotName('search-1024-focus')}.png`) });
          fig.keyboard = fig.keyboard ?? [];
          const last = await read(page, panel, 'shot: ArrowDown, ArrowUp');
          say(`focus shot read ${JSON.stringify(last)}`);
          await panel.getByRole('combobox', { name: 'Sort by' }).selectOption('new');
          // Alt+ArrowDown on the focused select: the Lounge's next-channel shortcut takes it (recorded, not asserted).
          await panel.getByRole('combobox', { name: 'Sort by' }).focus();
          const before = new URL(page.url()).pathname;
          await page.keyboard.press('Alt+ArrowDown');
          await page.waitForTimeout(1200);
          say(
            `Alt+ArrowDown on the select: path ${before} -> ${new URL(page.url()).pathname}, panel visible ${await panel.isVisible()}`,
          );
        }
        say(`${c.tag} ${JSON.stringify({ ...fig, keyboard: undefined })}`);
        if (fig.keyboard) say(`${c.tag} keyboard ${JSON.stringify(fig.keyboard)}`);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).split('\n')[0]!.slice(0, 400)}`);
      } finally {
        await ctx.close();
      }
    }
  } finally {
    for (const ch of ids.chans)
      await apiDelete(owner.request, `channels/${ch}`).catch(() => undefined);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Fig>;
    for (const k of Object.keys(figs)) {
      const a = figs[k]!;
      const b = base[k];
      if (!b) continue;
      const phone = k.startsWith('375');
      const minH = phone ? 44 : 40;
      if (a.select && a.select.h < minH - 0.5)
        problems.push(`${k}: select ${a.select.h} high, under ${minH}`);
      if (a.select && b.select && a.select.w < b.select.w - 0.5)
        problems.push(`${k}: select ${a.select.w} wide, base ${b.select.w}`);
      if (a.textClear === false) problems.push(`${k}: select text not clear of the chevron`);
      if (a.bar && b.bar) {
        const grow = (a.select?.h ?? 0) - (b.select?.h ?? 0);
        if (a.bar.h > b.bar.h + Math.max(0, grow) + 0.5)
          problems.push(`${k}: bar ${a.bar.h} high, base ${b.bar.h}, select grew ${grow}`);
      }
      if (a.hit.pass + a.hit.fail !== b.hit.pass + b.hit.fail || a.hit.fail > b.hit.fail)
        problems.push(
          `${k}: hit test ${JSON.stringify(a.hit)} against base ${JSON.stringify(b.hit)}`,
        );
    }
    // the keyboard: after, every key keeps focus, value and node; the options are all reached
    const kb = figs['1024-lounge-light']?.keyboard;
    if (kb) {
      for (const seq of kb)
        for (const r of seq.slice(1)) {
          if (!r.focused || !r.sameNode)
            problems.push(`keyboard "${r.step}": focused ${r.focused}, same node ${r.sameNode}`);
        }
      const seen = new Set(kb[0]!.map((r) => r.value));
      for (const v of ['new', 'old', 'relevant'])
        if (!seen.has(v)) problems.push(`arrow keys never reached "${v}"`);
      const seen2 = new Set(kb[1]!.map((r) => r.value));
      for (const v of ['new', 'old', 'relevant'])
        if (!seen2.has(v)) problems.push(`F4 + arrow keys never reached "${v}"`);
      const first = (v: string) => kb[0]!.find((r) => r.value === v)?.firstHit ?? null;
      if (first('new') && first('old') && first('new') === first('old'))
        problems.push(`the first hit is the same for Newest and Oldest: ${first('new')}`);
    }
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  await owner.close();
  say(`notes ${JSON.stringify(notes)}`);
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'after') expect(problems).toEqual([]);
});
