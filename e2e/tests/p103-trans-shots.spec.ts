import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { edgeStrips } from '../support/edge-strips';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTest, ProbeError } from '../support/hit-test';
import { seedTranscodingRow } from '../support/p100-seed';

/**
 * SEC-AUD-455 (D-982) and D-969 (wave 103, trans). Admin > Cinema > Transcoding > Queue with the three rows of
 * `seedTranscodingRow()` (queued, running, failed), picked by the ids the function returns (D-942) and removed again in
 * a `finally`.
 * Item 1: the storage column's head and the stacked row's label read "Mount" (base: "Bucket"); the head and column
 * widths at 1024 / 900 / 375 are logged so the base and after can be compared.
 * Item 2: the Invoices table's edge shade on the queue's scroll wrapper (D-945, D-965). Each state is cut twice at the
 * wrapper's two edges with `edgeStrips`: with the shade switched off (a style tag that sets `background-image: none`)
 * and as the page draws it. A strip that equals its "off" strip has no shade; one that differs has. Fractional widths
 * are forced by the spec (TEST HARNESS: the table's inline size is set to the wrapper's client width + 81 + f, f in 0,
 * 0.25, 0.5, 0.75) as `p102-shade-shots.spec.ts` does.
 *
 * PHASE `base` (TranscodingSection.tsx and admin.css put back to the base's text by the round's script) or `after`.
 * Only item 1's measures and the base shot run in `base`; the shade checks need the new class and run in `after`.
 * Needs the sweep world (`e2e/.state/sweep/owner.json`), a stack with `SR_ADMIN_REQUIRE_REAUTH=0`, and `psql` +
 * `DATABASE_URL` (see the header of `e2e/support/p100-seed.ts`).
 *
 * The command (from `e2e/`; ports 8805 / 5805 / 9505, database sr_p103_trans_ui):
 *   SR_P103_TRANS_ON=1 SR_P103_TRANS_PHASE=after SR_E2E_BASE_URL=http://localhost:5805 SR_CLI_BIN=<sr-cli> \
 *   SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P103_TRANS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-trans \
 *   npx playwright test tests/p103-trans-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Variables: SR_P103_TRANS_ON, SR_P103_TRANS_PHASE, SR_P103_TRANS_OUT.
 */
const ON = process.env.SR_P103_TRANS_ON === '1';
const PHASE = process.env.SR_P103_TRANS_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_TRANS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-trans';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const WRAP = '.adm-table-wrap:has(> table[aria-label])';
const OFF_ID = 'p103-trans-shade-off';
const FRACTIONS = [0, 0.25, 0.5, 0.75];

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const RUNS = [
  { key: '1024', theme: 'lounge-light', w: 1024, h: 768, phone: false },
  { key: '1024', theme: 'lounge-dark', w: 1024, h: 768, phone: false },
  { key: '900', theme: 'lounge-light', w: 900, h: 768, phone: false },
  { key: '900', theme: 'lounge-dark', w: 900, h: 768, phone: false },
  { key: '375', theme: 'lounge-light', w: 375, h: 812, phone: true },
] as const;

/** Runs in the page: heads, column widths, the first row's labels, the wrapper's scroll figures. */
function measureTable() {
  const f = (n: number) => Math.round(n * 10) / 10;
  const de = document.documentElement;
  const wrap = document.querySelector<HTMLElement>('.adm-table-wrap')!;
  const table = wrap.querySelector<HTMLElement>('table')!;
  const ths = Array.from(table.querySelectorAll('thead th'));
  const row = table.querySelector('tbody tr');
  return {
    heads: ths.map((h) => h.textContent),
    headWidths: ths.map((h) => f(h.getBoundingClientRect().width)),
    headsVisible: getComputedStyle(ths[0]!).display !== 'none',
    labels: row ? Array.from(row.children).map((c) => c.getAttribute('data-label')) : [],
    tableWidth: f(table.getBoundingClientRect().width),
    wrapClient: wrap.clientWidth,
    wrapScroll: wrap.scrollWidth,
    wrapRect: f(wrap.getBoundingClientRect().width),
    wrapRole: wrap.getAttribute('role'),
    wrapTabindex: wrap.getAttribute('tabindex'),
    bgImage: getComputedStyle(wrap).backgroundImage,
    wrapBg: getComputedStyle(wrap).backgroundColor,
    rootBg: getComputedStyle(document.querySelector('.ss-root') ?? document.body).backgroundColor,
    docSideways: de.scrollWidth - de.clientWidth,
    tableClass: table.className,
  };
}

test('SEC-AUD-455 / D-969: the Transcoding queue names its Mount column; its table shows the edge shade', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `measure-${PHASE}.json`);
  const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const problems: string[] = [];
  const tx = seedTranscodingRow();
  const failed = tx.jobs.find((j) => j.state === 'failed')!;
  console.log(
    `REPORT phase ${PHASE}; suffix ${tx.suffix}: ${tx.jobs.map((j) => `${j.id}/${j.state}`).join(', ')}`,
  );
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };

  const decode = (page: Page, png: Buffer) =>
    page.evaluate(async (b64) => {
      const bin = atob(b64);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
      const c = new OffscreenCanvas(bmp.width, bmp.height);
      const x = c.getContext('2d')!;
      x.drawImage(bmp, 0, 0);
      return { w: bmp.width, data: Array.from(x.getImageData(0, 0, bmp.width, bmp.height).data) };
    }, png.toString('base64'));
  /** Columns (x) where the mean over the rows differs between the page's strip and the off strip; and the darkest mean. */
  const diffCols = (a: { w: number; data: number[] }, off: { w: number; data: number[] }) => {
    const h = a.data.length / 4 / a.w;
    const cols: string[] = [];
    let darkest = 0;
    for (let x = 0; x < a.w; x++) {
      let sum = 0;
      for (let y = 0; y < h; y++) {
        const i = (y * a.w + x) * 4;
        let m = 0;
        for (let k = 0; k < 4; k++) m = Math.max(m, Math.abs(a.data[i + k]! - off.data[i + k]!));
        sum += m;
      }
      const mean = sum / h;
      if (mean > 0) cols.push(`${x}:${Math.round(mean * 10) / 10}`);
      darkest = Math.max(darkest, mean);
    }
    return { cols, darkest: Math.round(darkest * 10) / 10 };
  };
  const setOff = (page: Page, off: boolean) =>
    page.evaluate(
      ([id, on]) => {
        document.getElementById(id as string)?.remove();
        if (!on) return;
        const st = document.createElement('style');
        st.id = id as string;
        st.textContent = `.adm-table-wrap { background-image: none !important; }`;
        document.head.appendChild(st);
      },
      [OFF_ID, off] as const,
    );
  /** TEST HARNESS: forces a fractional content width. */
  const setFraction = (page: Page, f: number) =>
    page.evaluate(
      ([sel, fr]) => {
        const w = document.querySelector<HTMLElement>(sel as string)!;
        const t = w.querySelector<HTMLElement>('table')!;
        t.style.inlineSize = '';
        t.style.minInlineSize = '';
        const natural = t.getBoundingClientRect().width;
        const target = Math.max(w.clientWidth + 81, Math.ceil(natural)) + (fr as number);
        t.style.inlineSize = `${target}px`;
        t.style.minInlineSize = `${target}px`;
        return { natural, target };
      },
      [WRAP, f] as const,
    );
  const scrollTo = async (page: Page, where: 'start' | 'middle' | 'end') => {
    await page.locator(WRAP).evaluate((w, wh) => {
      w.scrollIntoView({ block: 'center' });
      w.scrollLeft =
        wh === 'start' ? 0 : wh === 'end' ? w.scrollWidth : (w.scrollWidth - w.clientWidth) / 2;
    }, where);
    await page.mouse.move(0, 0);
    await page.waitForTimeout(150);
  };
  const strips = async (page: Page) => {
    await setOff(page, true);
    await page.waitForTimeout(60);
    const off = await edgeStrips(page, WRAP, { width: 16, maxHeight: 200 });
    await setOff(page, false);
    await page.waitForTimeout(60);
    const on = await edgeStrips(page, WRAP, { width: 16, maxHeight: 200 });
    return {
      off,
      on,
      l: diffCols(await decode(page, on.l), await decode(page, off.l)),
      r: diffCols(await decode(page, on.r), await decode(page, off.r)),
    };
  };

  try {
    for (const s of RUNS) {
      if (PHASE === 'base' && !(s.theme === 'lounge-light')) continue;
      const prefs = {
        theme: s.theme,
        zoom: 100,
        sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'lounge-editorial' },
      };
      await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
      const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
        settings?: { user_prefs?: { theme?: string } };
      };
      console.log(
        `REPORT account theme set ${s.theme}, read back ${back.settings?.user_prefs?.theme}`,
      );
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
      const tag = `${s.key}-${s.theme}`;
      await page.goto('/admin/cinema-transcoding', {
        waitUntil: 'domcontentloaded',
        timeout: 15_000,
      });
      await idle(page, 5000);
      await page.locator('.adm-table-wrap table').first().waitFor({ timeout: 10_000 });
      const panel = page.locator('[role="tabpanel"]:visible').first();
      for (const j of tx.jobs)
        await expect(panel.getByText(j.id, { exact: true }), `job ${j.id} shown`).toHaveCount(1);
      const theme = await page.evaluate(
        () => document.querySelector('.ss-root[data-theme]')?.getAttribute('data-theme') ?? 'none',
      );
      expect(theme, 'data-theme read back from the Admin root').toBe(s.theme);

      const m = await page.evaluate(measureTable);
      log({ phase: PHASE, size: s.key, theme, ...m });
      console.log(`REPORT ${tag} table ${JSON.stringify(m)}`);
      if (PHASE === 'after') {
        expect(m.heads[2], 'the storage column head').toBe('Mount');
        if (s.phone) expect(m.labels[2], 'the stacked label').toBe('Mount');
        else expect(m.labels[2], 'the data-label').toBe('Mount');
      }

      const shoot = async (name: string, where: 'start' | 'end' | null) => {
        await page.evaluate(() => window.scrollTo(0, 0));
        if (where) await scrollTo(page, where);
        else
          await page
            .locator('.adm-table-wrap tbody tr', {
              has: page.getByText(failed.id, { exact: true }),
            })
            .evaluate((el) => el.scrollIntoView({ block: 'center' }));
        await page.waitForTimeout(300);
        await page.screenshot({ path: resolve(OUT, name) });
        shots.push(name);
      };

      if (PHASE === 'base') {
        if (s.key === '1024') await shoot('base-queue-1024.png', null);
        // Confirm first (item 2): the table scrolls, and nothing shows there is more.
        console.log(
          `REPORT ${tag} confirm: scrollWidth ${m.wrapScroll} vs clientWidth ${m.wrapClient}; background-image ${m.bgImage}`,
        );
      }

      if (PHASE === 'after') {
        if (s.key === '1024') await shoot(`queue-${tag}.png`, null);
        if (s.key === '900' && s.theme === 'lounge-light') await shoot(`queue-${tag}.png`, null);
        if (s.key === '375') await shoot(`queue-${tag}.png`, null);

        if (!s.phone) {
          expect(m.wrapScroll, `${tag}: the table scrolls in its wrapper`).toBeGreaterThan(
            m.wrapClient,
          );
          expect(m.docSideways, `${tag}: the page does not scroll sideways`).toBeLessThanOrEqual(0);
          expect(m.wrapRole, 'the keyboard stop is on while it scrolls').toBe('region');
          const rep = (msg: string) => {
            console.log(`REPORT ${tag} ${msg}`);
          };
          // Natural width, start / middle / end.
          for (const where of ['start', 'middle', 'end'] as const) {
            await scrollTo(page, where);
            const e = await strips(page);
            log({ phase: PHASE, size: s.key, theme, where, f: 'natural', l: e.l, r: e.r });
            rep(`${where}: left shade ${JSON.stringify(e.l)} right shade ${JSON.stringify(e.r)}`);
            const shaded = (x: { cols: string[] }) => x.cols.length > 0;
            if (where === 'start') {
              if (shaded(e.l)) problems.push(`${tag} start: a shade at the start edge`);
              if (!shaded(e.r)) problems.push(`${tag} start: no shade at the end edge`);
            } else if (where === 'end') {
              if (!shaded(e.l)) problems.push(`${tag} end: no shade at the start edge`);
              if (shaded(e.r)) problems.push(`${tag} end: a shade or line at the end edge`);
            } else {
              if (!shaded(e.l) || !shaded(e.r))
                problems.push(`${tag} middle: not both edges shaded`);
            }
            if (s.theme === 'lounge-light' || s.key === '900') {
              const stem =
                where === 'middle'
                  ? null
                  : s.key === '1024' && s.theme === 'lounge-light'
                    ? `queue-1024-${where}`
                    : s.key === '900' && s.theme === 'lounge-dark' && where === 'end'
                      ? 'queue-900-end-dark'
                      : null;
              if (stem) {
                await page.screenshot({ path: resolve(OUT, `${stem}.png`) });
                shots.push(`${stem}.png`);
              }
            }
            writeFileSync(resolve(OUT, `strips-${tag}-${where}-L.png`), e.on.l);
            writeFileSync(resolve(OUT, `strips-${tag}-${where}-R.png`), e.on.r);
            shots.push(`strips-${tag}-${where}-L.png`, `strips-${tag}-${where}-R.png`);
          }
          // Forced fractional widths: no line at an edge that is at its end.
          for (const f of FRACTIONS) {
            const set = await setFraction(page, f);
            await page.waitForTimeout(150);
            for (const where of ['start', 'end'] as const) {
              await scrollTo(page, where);
              const e = await strips(page);
              const flat = where === 'start' ? e.l : e.r;
              const more = where === 'start' ? e.r : e.l;
              rep(
                `f${f} ${where}: table ${set.target}; flat edge cols ${JSON.stringify(flat.cols)}; shaded edge darkest ${more.darkest} (${more.cols.length} cols)`,
              );
              log({ phase: PHASE, size: s.key, theme, where, f, flat, more });
              if (flat.cols.length > 0)
                problems.push(
                  `${tag} f${f} ${where}: a line at the flat edge ${flat.cols.join(',')}`,
                );
              if (more.cols.length === 0)
                problems.push(`${tag} f${f} ${where}: no shade at the other edge`);
            }
          }
          // The Logs button of the failed row still takes its own point.
          await page.evaluate(
            ([sel]) => {
              const t = document.querySelector<HTMLElement>(sel as string)!;
              t.querySelectorAll('table').forEach((x) => {
                x.style.inlineSize = '';
                x.style.minInlineSize = '';
              });
            },
            [WRAP] as const,
          );
          const btn = page
            .locator('.adm-table-wrap tbody tr', {
              has: page.getByText(failed.id, { exact: true }),
            })
            .getByRole('button', { name: /logs/i });
          try {
            const h = await hitTest(page, btn);
            rep(
              `Logs button hit test: pass ${h.pass}; ${JSON.stringify(h.points.map((p) => [p.at, p.ok]))}`,
            );
            if (!h.pass) problems.push(`${tag}: the Logs button fails the hit test`);
          } catch (e) {
            if (e instanceof ProbeError)
              problems.push(`${tag}: Logs hit test not reached: ${e.message}`);
            else throw e;
          }
        } else {
          expect(m.bgImage, 'no shade in the stacked layout').toBe('none');
        }
      }
      await ctx.close();
    }
  } finally {
    try {
      tx.remove();
    } catch (e) {
      console.log(`REPORT note could not remove job rows: ${String(e).slice(0, 120)}`);
    }
    await ownerCtx.close();
  }
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT problems ${problems.length}: ${JSON.stringify(problems)}`);
  expect(problems, 'problem lines').toEqual([]);
});
