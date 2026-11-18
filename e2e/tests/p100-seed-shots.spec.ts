import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { seedActivityTier, seedTranscodingRow } from '../support/p100-seed';

/**
 * D-952 (wave 100, seed). Admin > Transcoding with rows, and an Arcade game with a content tier, shot and measured
 * at 1024 / 900 / 375 (Lounge Light). The rows come from `e2e/support/p100-seed.ts` (read its header for what each
 * needs and why the Transcoding rows are inserted into the stack's own database). A fault found here is a proposed
 * row in the handoff, never a failure of this spec; the spec fails only on a spec error (a row that did not show, a
 * hit-test row that was not reached).
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; never edited), a stack with `SR_ADMIN_REQUIRE_REAUTH=0` and `psql` + `DATABASE_URL` (the seeding of
 * the job rows). Own names `p100-seed-<random>` (D-942); the rows it made are removed again at the end.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8784 /
 * 5784 / 9484, database sr_p100_seed_ui):
 *   SR_P100_SEED_ON=1 SR_E2E_BASE_URL=http://localhost:5784 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P100_SEED_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-seed \
 *   npx playwright test tests/p100-seed-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The stack: sr-server with SR_DATABASE__URL=<db> SR_DATABASE__AUTO_MIGRATE=true SR_SERVER__BIND=127.0.0.1:8784
 * SR_SERVER__PUBLIC_URL=http://localhost:5784 SR_STORAGE__ENDPOINT=memory: SR_ACTIVITIES__ENABLED=true
 * SR_SERVER__METRICS_BIND=127.0.0.1:9484 SR_ADMIN_REQUIRE_REAUTH=0; Vite with SR_DEV_API_PORT=8784 VITE_SR_E2E=1
 * VITE_ACTIVITIES_ENABLED=true on 5784.
 * Variables: SR_P100_SEED_ON, SR_P100_SEED_OUT. One JSON line per measure in `measure.json`.
 */
const ON = process.env.SR_P100_SEED_ON === '1';
const OUT =
  process.env.SR_P100_SEED_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-seed';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const THEME = 'lounge-light';

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const SIZES = [
  { key: '1024', w: 1024, h: 768, phone: false },
  { key: '900', w: 900, h: 768, phone: false },
  { key: '375', w: 375, h: 812, phone: true },
] as const;

/** Runs in the page: sideways scroll of the document, and every element under `root` whose text is cut. */
function measureCut(arg: { root: string; texts: string[] }) {
  const root = document.querySelector<HTMLElement>(arg.root);
  const de = document.documentElement;
  const doc = de.scrollWidth - de.clientWidth;
  if (!root) return { doc, found: false as const };
  const flat = (root.textContent ?? '').replace(/\s+/g, ' ');
  const missing = arg.texts.filter((t) => !flat.includes(t.replace(/\s+/g, ' ')));
  const cut: {
    tag: string;
    cls: string;
    scrollWidth: number;
    clientWidth: number;
    text: string;
  }[] = [];
  for (const e of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
    if (e.getClientRects().length === 0) continue;
    const cs = getComputedStyle(e);
    if (
      e.scrollWidth > e.clientWidth + 1 &&
      cs.overflowX !== 'visible' &&
      !e.matches('.adm-table-wrap')
    )
      cut.push({
        tag: e.tagName.toLowerCase(),
        cls: String(e.className).slice(0, 30),
        scrollWidth: e.scrollWidth,
        clientWidth: e.clientWidth,
        text: (e.textContent ?? '').trim().slice(0, 40),
      });
    if (
      cs.textOverflow === 'ellipsis' &&
      e.scrollWidth > e.clientWidth + 1 &&
      !cut.some((c) => c.text === (e.textContent ?? '').trim().slice(0, 40))
    )
      cut.push({
        tag: e.tagName.toLowerCase(),
        cls: `${String(e.className).slice(0, 24)} (ellipsis)`,
        scrollWidth: e.scrollWidth,
        clientWidth: e.clientWidth,
        text: (e.textContent ?? '').trim().slice(0, 40),
      });
  }
  return { doc, found: true as const, missing, cut };
}

/** Runs in the page: the Transcoding wrapper and table. */
function measureTable() {
  const wrap = document.querySelector<HTMLElement>('.adm-table-wrap');
  const table = wrap?.querySelector<HTMLElement>('table');
  if (!wrap || !table) return null;
  const f = (n: number) => Math.round(n * 10) / 10;
  const th = table.querySelector('thead th');
  const rows = Array.from(table.querySelectorAll('tbody tr'));
  return {
    scrollWidth: wrap.scrollWidth,
    clientWidth: wrap.clientWidth,
    tableWidth: f(table.getBoundingClientRect().width),
    wrapRole: wrap.getAttribute('role'),
    wrapTabindex: wrap.getAttribute('tabindex'),
    stacked: table.classList.contains('adm-table-stack'),
    theadDisplay: th ? getComputedStyle(th.parentElement!.parentElement!).display : null,
    rows: rows.length,
    rowHeights: rows.map((r) => f(r.getBoundingClientRect().height)),
    lastCellRight: f(
      Math.max(...rows.map((r) => r.lastElementChild?.getBoundingClientRect().right ?? 0)),
    ),
    wrapRight: f(wrap.getBoundingClientRect().right),
    viewport: window.innerWidth,
  };
}

test('D-952: Transcoding rows and an Arcade game with a tier, shot and measured', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const notes: string[] = [];

  // ---------------------------------------------------------------- the rows
  const tx = seedTranscodingRow();
  const orig = await seedActivityTier({ owner: o }, 'TIER_1_ORIGINAL');
  const reimpl = await seedActivityTier({ owner: o });
  console.log(
    `REPORT seeded transcoding suffix ${tx.suffix}: ${tx.jobs.map((j) => `${j.id}/${j.state}`).join(', ')}`,
  );
  console.log(`REPORT seeded tiers: ${orig.id} ${orig.tier}; ${reimpl.id} ${reimpl.tier}`);

  const prefs = {
    theme: THEME,
    zoom: 100,
    sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: THEME },
  };
  await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
  const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: { arcade?: string } } };
  };
  console.log(
    `REPORT account theme set ${THEME}, read back: ${back.settings?.user_prefs?.theme}, arcade ${back.settings?.user_prefs?.sectionThemes?.arcade}`,
  );

  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };
  /** Back to the top: the page and every scrolled element (Admin scrolls its own content), wrappers to the start. */
  const top = (page: Page) =>
    page.evaluate(() => {
      window.scrollTo(0, 0);
      for (const e of Array.from(document.querySelectorAll<HTMLElement>('body *'))) {
        if (e.scrollTop > 0) e.scrollTop = 0;
        if (e.scrollLeft > 0) e.scrollLeft = 0;
      }
    });
  const hits = async (page: Page, root: Locator) => {
    await openCollapsed(page, root);
    const rows = await hitTestAll(page, root);
    const notReached = rows.filter((r) => r.status === 'not-reached');
    if (notReached.length)
      throw new Error(`hit test: ${notReached.length} not-reached (probe error)`);
    return rows.map((r) => ({
      name: r.name,
      status: r.status,
      box: r.result ? `${Math.round(r.result.box.width)}x${Math.round(r.result.box.height)}` : null,
      // Every failing point, with who took it (another control taking a point is the fault that matters most).
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map(
          (p) =>
            `${p.at}: ${p.taker?.tag ?? 'none'} "${p.taker?.name ?? ''}" (${p.taker?.path ?? ''})`,
        ),
    }));
  };

  try {
    for (const s of SIZES) {
      const ctx = await browser.newContext({
        baseURL: ORIGIN,
        viewport: { width: s.w, height: s.h },
        storageState: await ownerCtx.storageState(),
        bypassCSP: true,
        ...(s.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
      });
      await ctx.addInitScript((p) => {
        try {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: p }, version: 2 }),
          );
        } catch {
          /* no storage */
        }
      }, prefs);
      const page = await ctx.newPage();
      page.setDefaultTimeout(10_000);
      const themeOf = (sel: string) =>
        page.evaluate(
          (q) =>
            (document.querySelector(q) ?? document.querySelector('[data-theme]'))?.getAttribute(
              'data-theme',
            ) ?? 'no-theme',
          sel,
        );

      // ------------------------------------------------ Admin > Transcoding
      await page.goto('/admin/cinema-transcoding', {
        waitUntil: 'domcontentloaded',
        timeout: 15_000,
      });
      await idle(page, 5000);
      await page.locator('.adm-table-wrap table').first().waitFor({ timeout: 10_000 });
      const panel = page.locator('[role="tabpanel"]:visible').first();
      for (const j of tx.jobs)
        await expect(panel.getByText(j.id, { exact: true }), `job ${j.id} shown`).toHaveCount(1);
      const theme = await themeOf('.ss-root[data-theme]');
      const table = await page.evaluate(measureTable);
      const cut = await page.evaluate(measureCut, {
        root: '.adm-table-wrap',
        texts: [tx.jobs.find((j) => j.text)!.text!, ...tx.jobs.map((j) => j.id)],
      });
      await top(page);
      let file = `transcoding-${s.key}-${theme}.png`;
      await page.screenshot({ path: resolve(OUT, file) });
      shots.push(file);
      if (s.key === '375') {
        // The table in view, whole (the phone's page is long).
        await page
          .locator('.adm-table-wrap')
          .first()
          .evaluate((w) => w.scrollIntoView({ block: 'start' }));
        file = `transcoding-${s.key}-table-${theme}.png`;
        await page.screenshot({ path: resolve(OUT, file) });
        shots.push(file);
        await top(page);
      }
      const hit = await hits(page, panel);
      log({ id: 'transcoding', size: s.key, theme, table, cut, hit });
      console.log(
        `REPORT transcoding ${s.key} (${theme}): wrapper ${table?.scrollWidth}/${table?.clientWidth}, role ${table?.wrapRole}, tabindex ${table?.wrapTabindex}, stacked ${table?.stacked}, thead display ${table?.theadDisplay}, rows ${table?.rows}, doc sideways ${cut.doc}, text missing ${JSON.stringify((cut as { missing?: string[] }).missing)}, cut ${JSON.stringify((cut as { cut?: unknown[] }).cut)}`,
      );
      console.log(
        `REPORT transcoding ${s.key} hit${s.phone ? '' : ' (the 44 px touch rule, informational on a desktop width)'}: ${hit.length} controls, fails ${JSON.stringify(hit.filter((h) => h.status === 'fail'))}`,
      );

      // ------------------------------------------------ the Arcade (member's view)
      if (s.key !== '900') {
        await page.goto('/games', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await page.locator('.arc-root').waitFor({ timeout: 30_000 });
        await idle(page, 4000);
        const soon = page.locator('#coming-soon');
        await soon.waitFor({ timeout: 15_000 });
        const arcTheme = await page
          .locator('[data-theme]:has(.arc-root)')
          .last()
          .getAttribute('data-theme');
        expect(arcTheme, 'data-theme read back from the Arcade root').toBe(THEME);
        const cards = [orig, reimpl].map((t) =>
          soon.locator('.arc-card', { has: page.getByText(t.title, { exact: true }) }),
        );
        for (const c of cards) await expect(c).toHaveCount(1);
        const cardFacts = await Promise.all(
          cards.map((c) =>
            c.evaluate((el) => ({
              tag: el.querySelector('.arc-tag')?.textContent ?? null,
              tierLine: el.querySelector('.arc-tier')?.textContent ?? null,
              tierAria: el.querySelector('.arc-tier')?.getAttribute('aria-label') ?? null,
              box: (() => {
                const r = el.getBoundingClientRect();
                return [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10);
              })(),
            })),
          ),
        );
        const arcCut = await page.evaluate(measureCut, {
          root: '#coming-soon',
          texts: [orig.title, reimpl.title, orig.summary, reimpl.summary],
        });
        // The seeded cards in the viewport (the section is far down the page); shot before the hit test scrolls.
        await cards[0]!.evaluate((el) => el.scrollIntoView({ block: 'center' }));
        await page.waitForTimeout(400);
        let file = `activity-tier-${s.key}-${arcTheme}.png`;
        await page.screenshot({ path: resolve(OUT, file) });
        shots.push(file);
        if (s.phone) {
          await cards[1]!.evaluate((el) => el.scrollIntoView({ block: 'center' }));
          await page.waitForTimeout(400);
          file = `activity-tier-${s.key}-reimplementation-${arcTheme}.png`;
          await page.screenshot({ path: resolve(OUT, file) });
          shots.push(file);
        }
        const arcHit = await hits(page, soon);
        log({ id: 'activity-tier', size: s.key, theme: arcTheme, cardFacts, arcCut, arcHit });
        console.log(
          `REPORT activity-tier ${s.key} (${arcTheme}): cards ${JSON.stringify(cardFacts)}; doc sideways ${arcCut.doc}; missing ${JSON.stringify((arcCut as { missing?: string[] }).missing)}; cut ${JSON.stringify((arcCut as { cut?: unknown[] }).cut)}`,
        );
        console.log(
          `REPORT activity-tier ${s.key} hit${s.phone ? '' : ' (the 44 px touch rule, informational on a desktop width)'}: ${arcHit.length} controls, fails ${JSON.stringify(arcHit.filter((h) => h.status === 'fail'))}`,
        );
      }

      // ------------------------------------------------ Admin > Arcade (where the Operator sets the tier)
      if (s.key === '1024') {
        await page.goto('/admin/arcade', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(page, 5000);
        await page.locator('.arc-admin').waitFor({ timeout: 10_000 });
        const tierSel = page.getByRole('combobox', { name: 'Content tier' });
        await tierSel.scrollIntoViewIfNeeded();
        const before = (await tierSel.textContent())?.trim();
        const admCut = await page.evaluate(measureCut, {
          root: '.arc-admin',
          texts: [orig.title, reimpl.title],
        });
        // The tier is chosen by the form's Select: open it, pick the reimplementation, read it back.
        await tierSel.click();
        await page.getByRole('option', { name: /Reimplementation/i }).click();
        const after = (await tierSel.textContent())?.trim();
        await tierSel.scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
        file = `admin-arcade-tier-${s.key}-${await themeOf('.ss-root[data-theme]')}.png`;
        await page.screenshot({ path: resolve(OUT, file) });
        shots.push(file);
        const admHit = await hits(page, page.locator('.arc-admin'));
        log({ id: 'admin-arcade', size: s.key, before, after, admCut, admHit });
        console.log(
          `REPORT admin-arcade ${s.key}: tier select '${before}' -> '${after}'; doc sideways ${admCut.doc}; missing ${JSON.stringify((admCut as { missing?: string[] }).missing)}; cut ${JSON.stringify((admCut as { cut?: unknown[] }).cut)}; hit (the 44 px touch rule, informational on a desktop width) ${admHit.length} controls, fails ${admHit.filter((h) => h.status === 'fail').length}: ${JSON.stringify(admHit.filter((h) => h.status === 'fail'))}`,
        );
      }
      await ctx.close();
    }
  } finally {
    // The rows this run made are removed again; a failure here is only a note.
    try {
      tx.remove();
    } catch (e) {
      notes.push(`could not remove job rows: ${String(e).slice(0, 120)}`);
    }
    for (const t of [orig, reimpl])
      await t
        .remove()
        .catch((e) => notes.push(`could not remove ${t.id}: ${String(e).slice(0, 80)}`));
    await ownerCtx.close();
  }
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  for (const n of notes) console.log(`REPORT note ${n}`);
});
