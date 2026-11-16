import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-333 (D-837) and SEC-AUD-334 (D-840), on the real stack.
 *  1. A checkbox or radio in a tool is checkbox-sized and beside its label: for every tool of wave 79's list, each
 *     checkbox and radio's box and its label text's box, BASE (the new rule deleted from the live style sheet) and AFTER.
 *     Wave 80's height log of every `.ud-field` is re-run, base and after: equal.
 *  2. The naming table on the Cinema's submit page (three names: a long one without a space, a short one, one with a
 *     warning line): the "Read as" cell's width and lines, the table's and the wrapper's scroll and client widths, base
 *     (the `.rv-naming` rules deleted from the live style sheet) and after, 375 and 1280, Light and shipped.
 *  3. Findings: the JSON tool by its own name; Admin > ingest sources' guide at 375.
 * Local only (SR_P81_TOOLS=1). Shots and `measurements.txt` go to SR_P81_TOOLS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P81_TOOLS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-tools';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P81_TOOLS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

const note = (text: string) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(`REPORT ${text}`);
};

type Size = { width: number; height: number };
type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };

async function context(browser: Browser, base: BrowserContext, size: Size, prefs: Prefs) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

const setPrefs = (a: BrowserContext['request'], prefs: Prefs) =>
  apiPatch(a, '/users/@me/settings', { settings: { user_prefs: prefs } });

const prefsFor = (theme: string): Prefs => ({
  theme,
  zoom: 100,
  sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'sideroom' },
});

const themeOf = (page: Page) =>
  page.evaluate(
    () =>
      document.querySelector<HTMLElement>('.lng-root')?.dataset.theme ??
      document.documentElement.dataset.theme ??
      '?',
  );

async function openTool(page: Page, query: string, ready: string) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.keyboard.press('Enter');
  await expect(page.locator(ready).first()).toBeVisible({ timeout: 15_000 });
}

/** Delete the live rules whose selector matches: the page then wears the base CSS. Returns how many were deleted. */
const holdBack = (page: Page, source: string) =>
  page.evaluate((src) => {
    const re = new RegExp(src);
    let n = 0;
    type Container = { cssRules: CSSRuleList; deleteRule(i: number): void };
    const walk = (owner: Container) => {
      for (let i = owner.cssRules.length - 1; i >= 0; i--) {
        const r = owner.cssRules[i] as CSSStyleRule & Partial<Container>;
        if (r.selectorText && re.test(r.selectorText)) {
          owner.deleteRule(i);
          n++;
        } else if (r.cssRules && !r.selectorText) {
          walk(r as unknown as Container);
        }
      }
    };
    for (const sheet of Array.from(document.styleSheets)) {
      try {
        walk(sheet);
      } catch {
        /* cross-origin */
      }
    }
    return n;
  }, source);

interface CheckRow {
  kind: string;
  label: string;
  w: number;
  h: number;
  dy: number;
  inlineStartOK: boolean;
  stacked: boolean;
  wide: number;
}

/** Every checkbox and radio on the page: its box, its label text's first line box, and where they sit. */
const checks = (page: Page): Promise<CheckRow[]> =>
  page.evaluate(() =>
    Array.from(
      document.querySelectorAll<HTMLInputElement>('input[type=checkbox], input[type=radio]'),
    )
      .filter((i) => i.getBoundingClientRect().width > 0 || i.getBoundingClientRect().height > 0)
      .map((i) => {
        const lab = i.closest('label');
        const ib = i.getBoundingClientRect();
        let tb: DOMRect | null = null;
        if (lab) {
          const range = document.createRange();
          range.selectNodeContents(lab);
          const rects = Array.from(range.getClientRects()).filter(
            (r) => r.width > 0 && r.height > 0 && !(r.left === ib.left && r.top === ib.top),
          );
          // the first text line that is not the input itself
          const textRects = rects.filter(
            (r) => !(Math.abs(r.width - ib.width) < 0.5 && Math.abs(r.height - ib.height) < 0.5),
          );
          tb = textRects[0] ?? null;
        }
        const ic = ib.top + ib.height / 2;
        const tc = tb ? tb.top + tb.height / 2 : ic;
        const parent = i.parentElement!.getBoundingClientRect();
        return {
          kind: i.type,
          label: (lab?.textContent ?? i.getAttribute('aria-label') ?? '').trim().slice(0, 28),
          w: ib.width,
          h: ib.height,
          dy: ic - tc,
          inlineStartOK: tb ? ib.left <= tb.left + 0.5 : true,
          stacked: tb ? tb.top > ib.bottom - 1 : false,
          wide: ib.width / Math.max(parent.width, 1),
        };
      }),
  );

interface FieldRow {
  caption: string;
  tag: string;
  label: number;
  control: number;
}
const fields = (page: Page): Promise<FieldRow[]> =>
  page.evaluate(() =>
    Array.from(
      document.querySelectorAll<HTMLElement>('.ud-root > .ud-field, .ud-timer > .ud-field'),
    ).map((f) => {
      const span = f.querySelector<HTMLElement>(':scope > span');
      const ctl = f.querySelector<HTMLElement>(
        ':scope > input, :scope > textarea, :scope > select',
      );
      return {
        caption: (span?.textContent ?? '').trim().slice(0, 24),
        tag: ctl?.tagName.toLowerCase() ?? '-',
        label: f.getBoundingClientRect().height,
        control: ctl?.getBoundingClientRect().height ?? 0,
      };
    }),
  );

type Tool = { slug: string; query: string; ready?: string; act?: (page: Page) => Promise<void> };
const fillFirst = (text: string) => async (page: Page) => {
  const ta = page
    .locator('.ud-root textarea, .ud-root input[type=text], .ud-root input:not([type])')
    .first();
  if (await ta.count()) await ta.fill(text);
};
const TOOLS: Tool[] = [
  { slug: 'json', query: 'json', act: fillFirst('{"b":1,"a":[1,2,3]}') },
  {
    slug: 'regex',
    query: 'regex tester',
    act: async (p) => {
      await p.locator('.ud-root input').first().fill('a+b');
      await p.locator('.ud-root textarea').first().fill('aab ab b aaab');
    },
  },
  { slug: 'base64', query: 'base64', act: fillFirst('hello world?>>') },
  { slug: 'url', query: 'url encode', act: fillFirst('a b&c=d/é') },
  { slug: 'jwt', query: 'jwt decoder', act: fillFirst('eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig') },
  { slug: 'mdtable', query: 'markdown table', act: fillFirst('a,b\n1,2') },
  { slug: 'counter', query: 'word counter', act: fillFirst('one two three four') },
  {
    slug: 'hash',
    query: 'hashes',
    act: async (page) => {
      await page
        .locator('input[type=file]')
        .first()
        .setInputFiles({ name: 'a.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
      await page.waitForTimeout(600);
    },
  },
  {
    slug: 'diff',
    query: 'diff',
    act: async (p) => {
      const tas = p.locator('.ud-root textarea');
      await tas.nth(0).fill('one\ntwo\nthree');
      await tas.nth(1).fill('one\n2\nthree\nfour');
    },
  },
  { slug: 'dice', query: 'dice' },
  { slug: 'coin', query: 'coin flip' },
  { slug: 'wheel', query: 'wheel' },
  { slug: 'loot', query: 'loot tables' },
  { slug: 'notes', query: 'private notes' },
  { slug: 'pomodoro', query: 'pomodoro', ready: '.ud-timer' },
  { slug: 'stopwatch', query: 'stopwatch', ready: '.ud-timer' },
  { slug: 'countdown', query: 'countdown', ready: '.ud-timer' },
];
// Other tools of the drawer that hold a checkbox or radio (not under `.ud-root`); best effort.
const EXTRA: Tool[] = [
  { slug: 'programmer', query: 'programmer' },
  { slug: 'units', query: 'unit' },
  { slug: 'timezone', query: 'time zone' },
  { slug: 'poll', query: 'poll' },
  { slug: 'audio', query: 'audio' },
];

test('a checkbox in a tool is checkbox-sized and beside its label', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  let shots = 0;
  const bad: string[] = [];
  const heightsEqual: string[] = [];

  const variants: Array<[string, string, Size]> = [
    ['lounge-light', 'desktop', DESKTOP],
    ['lounge-dark', 'desktop', DESKTOP],
    ['lounge-light', 'phone', PHONE],
    ['lounge-dark', 'phone', PHONE],
  ];
  for (const [theme, vp, size] of variants) {
    const prefs = prefsFor(theme);
    await setPrefs(octx.request, prefs);
    const c = await context(browser, octx, size, prefs);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    const main = theme === 'lounge-light' && vp === 'desktop';
    const list = main
      ? [...TOOLS, ...EXTRA]
      : TOOLS.filter((t) => ['base64', 'mdtable'].includes(t.slug));
    for (const tool of list) {
      const tag = `${tool.slug}-${theme}-${vp}`;
      try {
        await page.goto('/channels/@me');
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        expect(await themeOf(page), 'the room wears the chosen theme').toBe(theme);
        await openTool(page, tool.query, tool.ready ?? '.ud-root, .ud-timer, [role=dialog] input');
        if (tool.act) await tool.act(page);
        await page.waitForTimeout(300);
        const over = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(over, `${tag}: no sideways scroll`).toBeLessThanOrEqual(0);
        const after = await checks(page);
        const afterFields = await fields(page);
        for (const r of after) {
          note(
            `AFTER ${tag} ${r.kind} "${r.label}": ${r.w.toFixed(1)} x ${r.h.toFixed(1)}, dy ${r.dy.toFixed(1)}, startSide ${r.inlineStartOK}, stacked ${r.stacked}, share of parent ${(r.wide * 100).toFixed(0)}%`,
          );
          if (r.w > 20 || r.h > 20 || Math.abs(r.dy) > 4 || !r.inlineStartOK || r.stacked)
            bad.push(`${tag} "${r.label}"`);
        }
        // Click-through: tick the checkbox of the two named tools and see the output change.
        if (main && ['base64', 'mdtable', 'json'].includes(tool.slug)) {
          const box = page.locator('.ud-root input[type=checkbox]').first();
          const out = page.locator('.ud-out').first();
          const before = await out.innerText();
          const was = await box.isChecked();
          await box.click();
          await page.waitForTimeout(250);
          const now = await out.innerText();
          note(
            `click-through ${tool.slug}: checked ${was} -> ${await box.isChecked()}; output changed ${before !== now}`,
          );
          await box.click();
        }
        await page.screenshot({ path: resolve(OUT, `${tag}-after.png`) });
        shots++;
        if (main) {
          const n = await holdBack(page, '\\.ud-(root|timer) input\\[type=.(checkbox|radio)');
          await page.waitForTimeout(300);
          const base = await checks(page);
          note(`BASE ${tag}: ${n} rule(s) held back`);
          for (const r of base) {
            note(
              `BASE ${tag} ${r.kind} "${r.label}": ${r.w.toFixed(1)} x ${r.h.toFixed(1)}, dy ${r.dy.toFixed(1)}, startSide ${r.inlineStartOK}, stacked ${r.stacked}, share of parent ${(r.wide * 100).toFixed(0)}%`,
            );
          }
          const baseFields = await fields(page);
          const eq =
            JSON.stringify(
              afterFields.map((f) => [f.tag, Math.round(f.label), Math.round(f.control)]),
            ) ===
            JSON.stringify(
              baseFields.map((f) => [f.tag, Math.round(f.label), Math.round(f.control)]),
            );
          heightsEqual.push(`${tool.slug}:${eq}`);
          for (const f of afterFields)
            note(
              `HEIGHT ${tool.slug} "${f.caption}" <${f.tag}> label ${f.label.toFixed(1)} control ${f.control.toFixed(1)}`,
            );
          if (['base64', 'mdtable'].includes(tool.slug)) {
            expect(n, 'the new rule was found and held back').toBeGreaterThan(0);
            await page.screenshot({ path: resolve(OUT, `${tool.slug}-${theme}-${vp}-base.png`) });
            shots++;
          }
        }
      } catch (e) {
        note(`UNREACHED ${tag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      }
    }
    await c.close();
  }
  note(`field heights base == after: ${heightsEqual.join(' ')}`);
  note(`checks outside the target: ${bad.length ? bad.join('; ') : 'none'}`);
  note(`tools: ${shots} shots`);
  expect(bad, 'every checkbox and radio is small, centred on its label and beside it').toEqual([]);
  expect(
    heightsEqual.filter((h) => h.endsWith('false')),
    'no field changed height',
  ).toEqual([]);
  await octx.close();
});

// ---- the naming table --------------------------------------------------------------------------------------------

const NAMES = [
  'The.Long.Corridor.2001.Remastered.Directors.Cut.1080p.BluRay.x264-GROUP.mkv',
  'Heat (1995).mkv',
  'Some.Movie.Title.mkv',
].join('\n');

interface TableRow {
  nameLeft: number;
  nameWidth: number;
  readLeft: number;
  readWidth: number;
  lines: number;
}
interface TableM {
  rows: TableRow[];
  headLefts: number[];
  headWidths: number[];
  tableSW: number;
  tableCW: number;
  wrapSW: number;
  wrapCW: number;
  docSW: number;
  docCW: number;
  warnings: number;
}
const measureTable = (page: Page): Promise<TableM> =>
  page.evaluate(() => {
    const table = document.querySelector<HTMLElement>('.rv-naming, table[aria-label] ')!;
    const t = (document.querySelector('.rv-table-wrap .rv-table') ?? table) as HTMLElement;
    const wrap = t.closest<HTMLElement>('.rv-table-wrap')!;
    const rows = Array.from(t.querySelectorAll('tbody tr')).map((tr) => {
      const [a, b] = Array.from(tr.querySelectorAll('td')) as HTMLElement[];
      const ar = a!.getBoundingClientRect();
      const br = b!.getBoundingClientRect();
      const range = document.createRange();
      range.selectNodeContents(b!);
      const tops = new Set(
        Array.from(range.getClientRects())
          .filter((r) => r.width > 0)
          .map((r) => Math.round(r.top / 4)),
      );
      return {
        nameLeft: ar.left,
        nameWidth: ar.width,
        readLeft: br.left,
        readWidth: br.width,
        lines: tops.size,
      };
    });
    const ths = Array.from(t.querySelectorAll('thead th')).map((h) => h.getBoundingClientRect());
    return {
      rows,
      headLefts: ths.map((h) => h.left),
      headWidths: ths.map((h) => h.width),
      tableSW: t.scrollWidth,
      tableCW: t.clientWidth,
      wrapSW: wrap.scrollWidth,
      wrapCW: wrap.clientWidth,
      docSW: document.documentElement.scrollWidth,
      docCW: document.documentElement.clientWidth,
      warnings: t.querySelectorAll('td .rv-muted').length,
    };
  });

async function typeAndParse(page: Page) {
  const guide = page.getByRole('button', { name: /naming/i }).first();
  await expect(guide).toBeVisible({ timeout: 30_000 });
  if ((await guide.getAttribute('aria-expanded')) === 'false') await guide.click();
  const ta = page.locator('textarea').first();
  await ta.fill(NAMES);
  await page
    .getByRole('button', { name: /test|parse/i })
    .first()
    .click();
  await expect(page.locator('.rv-table-wrap .rv-table tbody tr').first()).toBeVisible({
    timeout: 20_000,
  });
}

const fmtT = (m: TableM) =>
  `name col ${m.rows.map((r) => `${r.nameLeft.toFixed(0)}+${r.nameWidth.toFixed(0)}`).join('/')}; read col ${m.rows.map((r) => `${r.readLeft.toFixed(0)}+${r.readWidth.toFixed(0)} (${r.lines} lines)`).join('/')}; head lefts ${m.headLefts.map((x) => x.toFixed(0)).join(',')} widths ${m.headWidths.map((x) => x.toFixed(0)).join(',')}; table sw/cw ${m.tableSW}/${m.tableCW}; wrap sw/cw ${m.wrapSW}/${m.wrapCW}; doc sw/cw ${m.docSW}/${m.docCW}; warning lines ${m.warnings}`;

test('the naming table reads in phrases on a phone', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, bootstrapState().owner);
  let shots = 0;
  for (const [variant, section] of [
    ['light', 'lounge-light'],
    ['shipped', 'sideroom'],
  ] as const) {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: section, library: 'sideroom', arcade: 'sideroom' },
    };
    await setPrefs(octx.request, prefs);
    for (const [vp, size] of [
      ['phone', PHONE],
      ['desktop', DESKTOP],
    ] as const) {
      const c = await context(browser, octx, size, prefs);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      await page.goto('/cinema/submit');
      await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', section, {
        timeout: 60_000,
      });
      await typeAndParse(page);
      await page.waitForTimeout(500);
      const tag = `naming-${variant}-${vp}`;
      const after = await measureTable(page);
      note(`AFTER ${tag}: ${fmtT(after)}`);
      await page.locator('.rv-table-wrap').scrollIntoViewIfNeeded();
      await page.screenshot({ path: resolve(OUT, `${tag}-after.png`) });
      shots++;
      expect(after.docSW, `${tag}: the document does not scroll sideways`).toBeLessThanOrEqual(
        after.docCW,
      );
      expect(after.warnings, `${tag}: one name produced a warning line`).toBeGreaterThan(0);
      if (vp === 'phone') {
        for (const r of after.rows)
          expect
            .soft(r.readWidth, `${tag}: the Read as cell is at least 192 px`)
            .toBeGreaterThanOrEqual(192);
      }
      if (variant === 'light') {
        const n = await holdBack(page, '\\.rv-naming');
        await page.waitForTimeout(300);
        const base = await measureTable(page);
        note(`BASE ${tag}: ${n} rule(s) held back; ${fmtT(base)}`);
        if (vp === 'phone') {
          await page.screenshot({ path: resolve(OUT, `${tag}-base.png`) });
          shots++;
          expect(n, 'the rules were found and held back').toBeGreaterThan(0);
        }
        if (vp === 'desktop') {
          const moved = after.headLefts.map((l, i) => Math.abs(l - base.headLefts[i]!));
          const widened = after.headWidths.map((w, i) => Math.abs(w - base.headWidths[i]!));
          note(
            `DIFF ${tag}: column lefts moved ${moved.map((x) => x.toFixed(1)).join(',')}; widths ${widened.map((x) => x.toFixed(1)).join(',')}`,
          );
        }
      }
      expect(errors, 'no page errors').toEqual([]);
      await c.close();
    }
  }
  // Admin > ingest sources shows the same guide (the Owner is staff), phone, Light.
  const prefs = prefsFor('lounge-light');
  await setPrefs(octx.request, prefs);
  const c = await context(browser, octx, PHONE, prefs);
  const page = await c.newPage();
  page.setDefaultTimeout(30_000);
  try {
    await page.goto('/admin/ingest-sources');
    await typeAndParse(page);
    const m = await measureTable(page);
    note(`AFTER admin phone: ${fmtT(m)}`);
    await page.locator('.rv-table-wrap').scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(OUT, 'naming-admin-phone-after.png') });
    shots++;
  } catch (e) {
    note(`UNREACHED admin guide: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
    await page.screenshot({ path: resolve(OUT, 'naming-admin-phone-unreached.png') });
  }
  await c.close();
  await octx.close();
  note(`naming: ${shots} shots`);
});
