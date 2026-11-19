import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 109, sections (D-1022 with the edge rule D-1025; SEC-AUD-530, 531, 532, 533). Measures, then (phase after)
 * compares: the Library's and Cinema's section bar links, the Cinema home's "Explore all" arrow links (and one
 * `.arrowLink` outside the Cinema: a Library Home link), the empty-shelf links (Cinema Discover with its API answered
 * empty, and the Library's twin if the world has an empty library), and the Cinema comment with a spoiler.
 * 375 x 812 touch (default reach) in the shipped section theme; the bars also at 1024 x 768 with `reach: 'box'`.
 *
 * PHASE=base writes figures-base.json and base- shots; PHASE=after writes figures-after.json, compares when
 * figures-base.json is in OUT and lists every broken limit as a problem line:
 *  - at 1024 every bar row (box, status) is equal to its base;
 *  - at 375 each bar link keeps its box (x, width, height) and the content under the bar moves by at most 5 px;
 *  - at 375 no row that passed in the base fails, and every bar link, arrow link and empty-state link passes;
 *  - the spoiler line (the comment's reveal paragraph before reveal, against the revealed text) grows by 15 px or less.
 * It never seeds; it only reads `world.json` extras (the comment id) and switches the storefront on through the API
 * (the world is fresh for every job).
 * The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p109-integ/apps/web/dist --env SR_P109_SECTIONS_ON=1 --env SR_P109_SECTIONS_PHASE=after --env SR_P109_SECTIONS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-sections -- tests/p109-sections-shots.spec.ts
 */
const ON = process.env.SR_P109_SECTIONS_ON === '1';
const PHASE = process.env.SR_P109_SECTIONS_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P109_SECTIONS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-sections';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SECTIONS = {
  library: 'sideroom',
  cinema: 'sideroom',
  'live-tv': 'sideroom-broadcast',
  arcade: 'lounge-editorial',
};

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(600);
};
const CHROME = 'header:not(main header), nav:not(main nav)';

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}
interface RowFig {
  name: string;
  tag: string;
  status: string;
  reason: string | null;
  edge: string | null;
  box: Box | null;
  fails: string[];
}
type Loose = { edge?: string | null };
const r1 = (n: number) => Math.round(n * 10) / 10;
const rb = (b: Box | null): Box | null =>
  b ? { x: r1(b.x), y: r1(b.y), width: r1(b.width), height: r1(b.height) } : null;

function figs(rows: ControlRow[]): RowFig[] {
  return rows.map((r) => ({
    name: r.name,
    tag: r.tag,
    status: r.status,
    reason: r.reason ?? null,
    edge: (r as Loose).edge ?? null,
    box: rb(r.result?.box ?? null),
    fails: (r.result?.points ?? [])
      .filter((p) => !p.ok)
      .map(
        (p) =>
          `${p.at}@${Math.round(p.y)} taker ${p.taker?.tag}${p.taker?.role ? `[${p.taker.role}]` : ''} ${p.taker?.path ?? ''}`,
      ),
  }));
}

async function newPage(browser: Browser, owner: BrowserContext, wide: boolean) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: wide ? { width: 1024, height: 768 } : { width: 375, height: 812 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(wide ? {} : { userAgent: PHONE_UA, isMobile: true, hasTouch: true }),
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme: 'lounge-light', zoom: 100, sectionThemes: SECTIONS },
  );
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const sideways = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  say(`${name}: scrollWidth/clientWidth ${sideways.join('/')}`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}

const figures: Record<string, unknown> = {};
const problems: string[] = [];

/** One bar: each link's box, the bar's box, what lies under the bar within 6 px, the hit test. */
async function measureBar(page: Page, key: string, wide: boolean) {
  const bar = page.locator(CHROME).first();
  const links = bar.locator('a');
  const n = await links.count();
  const linkFigs: { name: string; box: Box | null; under: string[] }[] = [];
  for (let i = 0; i < n; i++) {
    const l = links.nth(i);
    const box = rb(await l.boundingBox());
    const name = ((await l.innerText().catch(() => '')) || '').trim().slice(0, 30);
    const under = box
      ? await page.evaluate(
          ({ cx, bottom }) =>
            [2, 4, 6].map((k) => {
              const e = document.elementFromPoint(cx, bottom + k);
              if (!e) return 'null';
              const ctl = e.closest('a,button,input,select,textarea,[role=button],[role=link]');
              return `${k}px: ${e.tagName.toLowerCase()}.${String(e.className).slice(0, 30)}${ctl ? ' CONTROL' : ''}`;
            }),
          { cx: box.x + box.width / 2, bottom: box.y + box.height },
        )
      : [];
    linkFigs.push({ name, box, under });
  }
  const barBox = rb(await bar.boundingBox());
  const main = rb(await page.getByRole('main').first().boundingBox());
  const rows = figs(
    await hitTestAll(page, page.locator(CHROME), wide ? { reach: 'box' } : undefined),
  );
  say(`${key}: bar ${JSON.stringify(barBox)} main-top ${main?.y}`);
  for (const l of linkFigs)
    say(`  ${key} link "${l.name}" ${JSON.stringify(l.box)} under [${l.under.join(' | ')}]`);
  for (const r of rows)
    say(
      `  ${key} row "${r.name}" ${r.status} edge ${r.edge} box ${JSON.stringify(r.box)} ${r.fails.join('; ')}`,
    );
  figures[key] = { bar: barBox, mainTop: main?.y ?? null, links: linkFigs, rows };
}

async function measureRoot(
  page: Page,
  key: string,
  root: Locator,
  pick: (r: ControlRow) => boolean,
) {
  await openCollapsed(page, root);
  const all = await hitTestAll(page, root);
  const rows = figs(all.filter(pick));
  const total = all.length;
  const failing = all.filter((r) => r.status !== 'pass' && r.status !== 'skipped').length;
  say(`${key}: ${rows.length} picked of ${total} controls (${failing} not passing in the root)`);
  for (const r of rows)
    say(`  ${key} row "${r.name}" ${r.status} box ${JSON.stringify(r.box)} ${r.fails.join('; ')}`);
  return { rows, total, all: figs(all) };
}

test('wave 109 sections: section bars, arrow links, empty links, spoiler comment', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as {
    extras?: { spoilerCommentTitleId?: string | null };
  };
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: SECTIONS } },
    });
    await setStudySurfacesWith(ownerCtx.request, true).catch((e) =>
      say(`storefront switches: ${String(e).slice(0, 120)}`),
    );

    const visit = async (
      key: string,
      path: string,
      wide: boolean,
      ready: (p: Page) => Promise<unknown>,
      work: (p: Page) => Promise<void>,
    ) => {
      const { ctx, page } = await newPage(browser, ownerCtx, wide);
      try {
        await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });
        await ready(page);
        await idle(page, 4000);
        await work(page);
      } catch (e) {
        problems.push(`${key}: ${String(e).slice(0, 300)}`);
      } finally {
        await ctx.close();
      }
    };
    const waitMain = async (p: Page) => {
      await p.getByRole('main').first().waitFor({ timeout: 30_000 });
      await p.waitForTimeout(1500);
    };

    // 1. The section bars, 375 touch and 1024 box reach.
    for (const [lib, path] of [
      ['library', '/library/browse'],
      ['cinema', '/cinema/movies'],
    ] as const) {
      await visit(`bar-${lib}-375`, path, false, waitMain, async (p) => {
        await measureBar(p, `bar-${lib}-375`, false);
        await shot(p, `bar-${lib}-375`);
      });
      await visit(`bar-${lib}-1024`, path, true, waitMain, async (p) => {
        await measureBar(p, `bar-${lib}-1024`, true);
        if (lib === 'library') await shot(p, 'bar-library-1024');
      });
    }

    // 2. The Cinema home's "Explore all" links, and one ArrowLink outside the Cinema (Library Home).
    await visit('cinema-home-375', '/cinema', false, waitMain, async (p) => {
      const main = p.getByRole('main');
      const texts = await main
        .locator('a[class*="arrowLink"]')
        .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
      const heads = await main.locator('a[class*="arrowLink"]').evaluateAll((els) =>
        els.map((e) => {
          const r = (e as HTMLElement).getBoundingClientRect();
          const p = e.parentElement?.parentElement?.getBoundingClientRect();
          const prev = e.parentElement?.previousElementSibling?.getBoundingClientRect();
          return {
            link: [r.x, r.y, r.width, r.height],
            row: p ? [p.x, p.y, p.width, p.height] : null,
            title: prev ? [prev.x, prev.y, prev.width, prev.height] : null,
          };
        }),
      );
      say(`cinema-home: arrow links ${JSON.stringify(texts)} geometry ${JSON.stringify(heads)}`);
      figures['cinema-home-geometry'] = heads;
      const m = await measureRoot(p, 'cinema-home-375', main, (r) =>
        texts.some((t) => t.length > 0 && r.name.startsWith(t.slice(0, 10))),
      );
      figures['cinema-home-375'] = { texts, ...m };
      if (texts.length === 0) problems.push('cinema-home-375: no .arrowLink found');
      await shot(p, 'cinema-home-375');
    });
    await visit('library-home-375', '/library', false, waitMain, async (p) => {
      const main = p.getByRole('main');
      const texts = await main
        .locator('a[class*="arrowLink"]')
        .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
      const m = await measureRoot(p, 'library-home-375', main, (r) =>
        texts.some((t) => t.length > 0 && r.name.startsWith(t.slice(0, 10))),
      );
      figures['library-home-375'] = { texts, ...m };
      if (texts.length === 0) problems.push('library-home-375: no .arrowLink found');
    });

    // 3. The empty-shelf links: Cinema Discover answered empty (the same way a member with nothing mounted sees it).
    {
      const { ctx, page } = await newPage(browser, ownerCtx, false);
      try {
        await page.route('**/cinema/discover**', (route) =>
          route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ hero: null, rows: [] }),
          }),
        );
        await page.goto('/cinema', { waitUntil: 'domcontentloaded', timeout: 30_000 });
        await page.getByTestId('shelf-empty-cinema').waitFor({ timeout: 20_000 });
        await idle(page, 3000);
        const root = page.getByTestId('shelf-empty-cinema');
        const links = await root.locator('.shv-empty-links a').evaluateAll((els) =>
          els.map((e) => {
            const r = (e as HTMLElement).getBoundingClientRect();
            return [(e.textContent ?? '').trim(), r.x, r.y, r.width, r.height];
          }),
        );
        say(`cinema empty links ${JSON.stringify(links)}`);
        const m = await measureRoot(page, 'cinema-empty-375', root, (r) => r.tag === 'a');
        figures['cinema-empty-375'] = { links, ...m };
        await shot(page, 'cinema-discover-empty-375');
      } catch (e) {
        problems.push(`cinema-empty-375: ${String(e).slice(0, 300)}`);
      } finally {
        await ctx.close();
      }
    }
    // The Library's twin, if the world has an empty library (the world's Library may hold nothing).
    await visit('library-empty-375', '/library/browse', false, waitMain, async (p) => {
      const root = p.getByTestId('shelf-empty-library');
      if ((await root.count()) === 0) {
        say('library-empty-375: the Library is not empty in this world (no shelf-empty-library)');
        figures['library-empty-375'] = null;
        return;
      }
      const m = await measureRoot(p, 'library-empty-375', root, (r) => r.tag === 'a');
      figures['library-empty-375'] = m;
      await shot(p, 'library-empty-375');
    });

    // 4. The Cinema comment with a spoiler.
    const titleId = w.extras?.spoilerCommentTitleId;
    if (!titleId) problems.push('world.json has no extras.spoilerCommentTitleId');
    else
      await visit(
        'cinema-comment-375',
        `/cinema/t/${titleId}`,
        false,
        (p) =>
          p
            .getByRole('tab', { name: /^Comments/ })
            .first()
            .waitFor({ timeout: 30_000 }),
        async (p) => {
          await p
            .getByRole('tab', { name: /^Comments/ })
            .first()
            .click();
          const note = p.getByText(/This contains spoilers/).first();
          await note.waitFor({ timeout: 20_000 });
          await note.scrollIntoViewIfNeeded();
          const line = p.locator('p.cin-spoiler').first();
          const item = p.locator('li, article').filter({ has: line }).last();
          const lineBefore = rb(await line.boundingBox());
          const itemBefore = rb(await item.boundingBox());
          const m = await measureRoot(p, 'cinema-comment-375', item, () => true);
          await shot(p, 'cinema-comment-375');
          await line.getByRole('button', { name: /Show spoiler/ }).click();
          await p.waitForTimeout(600);
          const body = item.locator('p.cin-post-body').first();
          const bodyAfter = rb(await body.boundingBox());
          const itemAfter = rb(await item.boundingBox());
          await shot(p, 'cinema-comment-revealed-375');
          const grow = lineBefore && bodyAfter ? r1(lineBefore.height - bodyAfter.height) : null;
          say(
            `spoiler line before ${JSON.stringify(lineBefore)} revealed text ${JSON.stringify(bodyAfter)} item before ${itemBefore?.height} after ${itemAfter?.height}: line taller by ${grow} px (item by ${itemBefore && itemAfter ? r1(itemBefore.height - itemAfter.height) : null})`,
          );
          figures['cinema-comment-375'] = {
            lineBefore,
            bodyAfter,
            itemBefore,
            itemAfter,
            lineGrowth: grow,
            ...m,
          };
        },
      );

    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figures, null, 1));

    // After: compare with the base figures.
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, any>;
      const fig = figures as Record<string, any>;
      for (const key of ['bar-library-1024', 'bar-cinema-1024']) {
        if (JSON.stringify(base[key]) !== JSON.stringify(fig[key]))
          problems.push(
            `${key}: changed at 1024 (must not): base ${JSON.stringify(base[key]?.rows)} after ${JSON.stringify(fig[key]?.rows)}`,
          );
      }
      for (const key of ['bar-library-375', 'bar-cinema-375']) {
        const b = base[key];
        const a = fig[key];
        if (!b || !a) continue;
        a.links.forEach((l: any, i: number) => {
          const bl = b.links[i];
          if (!bl || !l.box || !bl.box) return;
          for (const k of ['x', 'y', 'width', 'height'] as const)
            if (Math.abs(l.box[k] - bl.box[k]) > 0.2)
              problems.push(
                `${key} link "${l.name}": ${k} ${bl.box[k]} -> ${l.box[k]} (drawn link must not change)`,
              );
        });
        const shift = (a.mainTop ?? 0) - (b.mainTop ?? 0);
        if (shift > 5 || shift < 0)
          problems.push(`${key}: content under the bar moved ${shift} px (max 5)`);
        say(
          `${key}: content under the bar moved ${shift} px; bar ${JSON.stringify(b.bar)} -> ${JSON.stringify(a.bar)}`,
        );
        for (const r of a.rows as RowFig[])
          if (r.status !== 'pass' && r.status !== 'skipped')
            problems.push(`${key} "${r.name}": ${r.status} ${r.fails.join('; ')}`);
      }
      for (const key of [
        'cinema-home-375',
        'library-home-375',
        'cinema-empty-375',
        'library-empty-375',
        'cinema-comment-375',
      ]) {
        const b = base[key];
        const a = fig[key];
        if (!a) continue;
        if (!b) continue;
        // No control that passed in the base fails now.
        for (const ar of (a.all ?? a.rows) as RowFig[]) {
          const br = ((b.all ?? b.rows) as RowFig[]).find((x) => x.name === ar.name);
          if (br && br.status === 'pass' && ar.status === 'fail')
            problems.push(
              `${key} "${ar.name}": passed in the base, fails now: ${ar.fails.join('; ')}`,
            );
        }
        for (const ar of a.rows as RowFig[]) {
          const br = (b.rows as RowFig[]).find((x) => x.name === ar.name);
          if (ar.status !== 'pass')
            problems.push(`${key} "${ar.name}": ${ar.status} ${ar.fails.join('; ')}`);
          if (
            br?.box &&
            ar.box &&
            (Math.abs(br.box.width - ar.box.width) > 0.2 ||
              Math.abs(br.box.height - ar.box.height) > 0.2)
          )
            problems.push(
              `${key} "${ar.name}": drawn size ${br.box.width}x${br.box.height} -> ${ar.box.width}x${ar.box.height}`,
            );
        }
      }
      const g = fig['cinema-comment-375']?.lineGrowth;
      if (typeof g === 'number' && g > 15)
        problems.push(`spoiler line is ${g} px taller than the revealed text (max 15)`);
    }
    say(`problems: ${JSON.stringify(problems, null, 1)}`);
    expect(problems).toEqual([]);
  } finally {
    await setStudySurfacesWith(ownerCtx.request, false).catch(() => undefined);
    await ownerCtx.close();
  }
});
