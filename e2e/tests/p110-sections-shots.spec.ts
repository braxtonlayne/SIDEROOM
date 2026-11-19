import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 110, sections (D-1022; SEC-AUD-543, 544, 553, 554). Measures, then (phase after) compares: the Arcade's room bar
 * (`/games`), the Study's room bar (`/study`), Library Home's section head with its segmented control and "See rankings",
 * and the footer links (`/library`), and the Cinema home's arrow links. 375 x 812 touch (default reach) in each page's
 * shipped theme; the same pages at 1024 x 768 with `reach: 'box'`.
 *
 * PHASE=base writes figures-base.json and base- shots; PHASE=after writes figures-after.json and, when figures-base.json
 * is in OUT, lists every broken limit as a problem line:
 *  - at 1024 every figure (boxes, rows, statuses) equals the base;
 *  - at 375 each bar link, the Library Home controls and the Cinema arrow links keep their drawn size, every one of them
 *    passes, and no control that passed in the base fails now;
 *  - the Study bar grows by 20 px or less, the Library head by 16 px or less, the Arcade bar by 2 px;
 *  - no page scrolls sideways.
 * It never seeds. The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p110-integ/apps/web/dist --env SR_P110_SECTIONS_ON=1 --env SR_P110_SECTIONS_PHASE=after --env SR_P110_SECTIONS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p110-sections -- tests/p110-sections-shots.spec.ts
 */
const ON = process.env.SR_P110_SECTIONS_ON === '1';
const PHASE = process.env.SR_P110_SECTIONS_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P110_SECTIONS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p110-sections';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
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

const figures: Record<string, any> = {};
const problems: string[] = [];

async function shot(page: Page, name: string, target?: Locator) {
  mkdirSync(OUT, { recursive: true });
  if (target) await target.scrollIntoViewIfNeeded().catch(() => undefined);
  else await page.evaluate(() => window.scrollTo(0, 0));
  await page.waitForTimeout(300);
  const sideways = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  say(`${name}: scrollWidth/clientWidth ${sideways.join('/')}`);
  if (sideways[0] !== sideways[1])
    problems.push(`${name}: page scrolls sideways ${sideways.join('/')}`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}

const boxOf = async (l: Locator) => rb(await l.boundingBox());

/** A bar: its box, scroll box, each link's box, the hit test over the bar. */
async function measureBar(page: Page, key: string, root: Locator, wide: boolean) {
  const links = root.locator('a');
  const n = await links.count();
  const lf: { name: string; box: Box | null }[] = [];
  for (let i = 0; i < n; i++) {
    const l = links.nth(i);
    lf.push({
      name: ((await l.innerText().catch(() => '')) || '').trim().slice(0, 30),
      box: await boxOf(l),
    });
  }
  const scroll = await root.evaluate((el) => [
    el.scrollHeight,
    el.clientHeight,
    el.scrollWidth,
    el.clientWidth,
  ]);
  const main = await boxOf(page.getByRole('main').first());
  const rows = figs(await hitTestAll(page, root, wide ? { reach: 'box' } : undefined));
  const f = { bar: await boxOf(root), scroll, mainTop: main?.y ?? null, links: lf, rows };
  figures[key] = f;
  say(`${key}: bar ${JSON.stringify(f.bar)} scroll(h,ch,w,cw) ${scroll} main-top ${f.mainTop}`);
  for (const l of lf) say(`  ${key} link "${l.name}" ${JSON.stringify(l.box)}`);
  for (const r of rows)
    say(`  ${key} row "${r.name}" ${r.status} box ${JSON.stringify(r.box)} ${r.fails.join('; ')}`);
}

async function visit(
  browser: Browser,
  owner: BrowserContext,
  key: string,
  path: string,
  wide: boolean,
  ready: string,
  work: (p: Page) => Promise<void>,
) {
  const { ctx, page } = await newPage(browser, owner, wide);
  try {
    await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });
    await page.locator(ready).first().waitFor({ timeout: 30_000 });
    await idle(page, 4000);
    await work(page);
  } catch (e) {
    problems.push(`${key}: ${String(e).slice(0, 300)}`);
  } finally {
    await ctx.close();
  }
}

test('wave 110 sections: Arcade and Study bars, Library Home head and footer', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: SECTIONS } },
    });

    for (const wide of [false, true]) {
      const w = wide ? '1024' : '375';
      await visit(
        browser,
        ownerCtx,
        `arcade-bar-${w}`,
        '/games',
        wide,
        '.arc-roombar-link',
        async (p) => {
          const bar = p.locator('header.arc-roombar').first();
          await measureBar(p, `arcade-bar-${w}`, bar, wide);
          await shot(p, `arcade-bar-${w}`);
        },
      );
      await visit(
        browser,
        ownerCtx,
        `study-bar-${w}`,
        '/study',
        wide,
        '.stu-bar-link',
        async (p) => {
          const bar = p.locator('.stu-bar').first();
          await measureBar(p, `study-bar-${w}`, bar, wide);
          await shot(p, `study-bar-${w}`);
        },
      );
      await visit(
        browser,
        ownerCtx,
        `library-home-${w}`,
        '/library',
        wide,
        'footer.lib-footer',
        async (p) => {
          const main = p.getByRole('main').first();
          const head = p
            .locator('.lib-section-head')
            .filter({ has: p.getByRole('radiogroup') })
            .first();
          const group = head.getByRole('radiogroup').first();
          const rank = head.locator('a').first();
          const footer = p.locator('footer.lib-footer').first();
          const reach = wide ? { reach: 'box' as const } : undefined;
          await openCollapsed(p, main);
          const mainRows = figs(await hitTestAll(p, main, reach));
          const footRows = figs(await hitTestAll(p, footer, reach));
          const g = await boxOf(group);
          const k = await boxOf(rank);
          const hb = await boxOf(head);
          const footLinks = await footer.locator('a').evaluateAll((els) =>
            els.map((e) => {
              const r = e.getBoundingClientRect();
              return {
                name: (e.textContent ?? '').trim(),
                x: r.x,
                y: r.y,
                w: r.width,
                h: r.height,
              };
            }),
          );
          const fb = await boxOf(footer);
          const fig = {
            head: hb,
            group: g,
            rank: k,
            gap: g && k ? r1(k.y - (g.y + g.height)) : null,
            footer: fb,
            footLinks,
            docHeight: await p.evaluate(() => document.documentElement.scrollHeight),
            mainRows,
            footRows,
          };
          figures[`library-home-${w}`] = fig;
          say(
            `library-home-${w}: head ${JSON.stringify(hb)} group ${JSON.stringify(g)} rank ${JSON.stringify(k)} gap ${fig.gap}`,
          );
          say(
            `  footer ${JSON.stringify(fb)} links ${JSON.stringify(footLinks)} doc ${fig.docHeight}`,
          );
          for (const r of [...mainRows, ...footRows].filter((x) => x.status !== 'pass'))
            say(
              `  library-home-${w} row "${r.name}" ${r.status} box ${JSON.stringify(r.box)} ${r.fails.join('; ')}`,
            );
          if (!wide) {
            await shot(p, 'library-home-head-375', head);
            await shot(p, 'library-home-footer-375', footer);
          } else {
            await shot(p, 'library-home-1024');
          }
        },
      );
    }

    await visit(
      browser,
      ownerCtx,
      'cinema-home-375',
      '/cinema',
      false,
      'main a[class*="arrowLink"]',
      async (p) => {
        const main = p.getByRole('main').first();
        await openCollapsed(p, main);
        const rows = figs(await hitTestAll(p, main));
        figures['cinema-home-375'] = { rows };
        for (const r of rows.filter((x) => x.status !== 'pass'))
          say(`  cinema-home row "${r.name}" ${r.status} ${r.fails.join('; ')}`);
        await shot(p, 'cinema-home-375');
      },
    );

    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figures, null, 1));

    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, any>;
      for (const key of ['arcade-bar-1024', 'study-bar-1024', 'library-home-1024']) {
        if (JSON.stringify(base[key]) !== JSON.stringify(figures[key]))
          problems.push(`${key}: changed at 1024 (must not)`);
      }
      for (const [key, maxGrow] of [
        ['arcade-bar-375', 2],
        ['study-bar-375', 20],
      ] as const) {
        const b = base[key];
        const a = figures[key];
        if (!b || !a) continue;
        a.links.forEach((l: any, i: number) => {
          const bl = b.links[i];
          if (!bl?.box || !l.box) return;
          for (const k of ['x', 'width', 'height'] as const)
            if (Math.abs(l.box[k] - bl.box[k]) > 0.2)
              problems.push(
                `${key} "${l.name}": ${k} ${bl.box[k]} -> ${l.box[k]} (drawn link must not change)`,
              );
        });
        const grow = r1((a.bar?.height ?? 0) - (b.bar?.height ?? 0));
        say(
          `${key}: bar height ${b.bar?.height} -> ${a.bar?.height} (+${grow}); scroll ${b.scroll} -> ${a.scroll}`,
        );
        if (grow > maxGrow || grow < 0)
          problems.push(`${key}: the bar grew ${grow} px (max ${maxGrow})`);
        if (a.scroll[0] > a.scroll[1] + 1)
          problems.push(`${key}: the bar scrolls vertically ${a.scroll}`);
        for (const r of a.rows as RowFig[])
          if (r.status !== 'pass' && r.status !== 'skipped')
            problems.push(`${key} "${r.name}": ${r.status} ${r.fails.join('; ')}`);
      }
      const bl = base['library-home-375'];
      const al = figures['library-home-375'];
      if (bl && al) {
        say(
          `library-home-375: gap ${bl.gap} -> ${al.gap}; head ${bl.head?.height} -> ${al.head?.height}; footer ${bl.footer?.height} -> ${al.footer?.height}; doc ${bl.docHeight} -> ${al.docHeight}`,
        );
        if (al.gap == null || al.gap < 8)
          problems.push(
            `library-home-375: gap between the control and "See rankings" is ${al.gap}`,
          );
        if ((al.head?.height ?? 0) - (bl.head?.height ?? 0) > 16)
          problems.push('library-home-375: the head grew more than 16 px');
        for (const r of [...al.mainRows, ...al.footRows] as RowFig[])
          if (r.status !== 'pass' && r.status !== 'skipped')
            problems.push(`library-home-375 "${r.name}": ${r.status} ${r.fails.join('; ')}`);
        const sizes = (f: any) =>
          [...f.mainRows, ...f.footRows].map(
            (r: RowFig) => `${r.name}|${r.box?.width}x${r.box?.height}`,
          );
        const bs = sizes(bl);
        for (const s of sizes(al))
          if (!bs.includes(s))
            problems.push(`library-home-375: drawn size changed or new control ${s}`);
      }
      const bc = base['cinema-home-375'];
      const ac = figures['cinema-home-375'];
      if (bc && ac)
        for (const ar of ac.rows as RowFig[]) {
          const br = (bc.rows as RowFig[]).find((x) => x.name === ar.name);
          if (br?.status === 'pass' && ar.status !== 'pass')
            problems.push(`cinema-home-375 "${ar.name}": passed in the base, ${ar.status} now`);
          if (
            br?.box &&
            ar.box &&
            (Math.abs(br.box.width - ar.box.width) > 0.2 ||
              Math.abs(br.box.height - ar.box.height) > 0.2)
          )
            problems.push(`cinema-home-375 "${ar.name}": drawn size changed`);
        }
    }
    say(`problems: ${JSON.stringify(problems, null, 1)}`);
    expect(problems).toEqual([]);
  } finally {
    await ownerCtx.close();
  }
});
