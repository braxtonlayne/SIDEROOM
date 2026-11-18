import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { seedTranscodingRow } from '../support/p100-seed';

/**
 * D-955 (wave 101, trans; SEC-AUD-443 and 444). The Admin > Cinema transcoding > Queue table with the three rows of
 * `seedTranscodingRow()` (queued, running, failed with a long error holding an unbroken file name), measured and shot at
 * 375 (touch) / 900 / 1024 in Lounge Light, and once at 375 in Lounge Dark. The rows are picked by the ids the function
 * returns, never by position or count (D-942); they are removed again in a `finally`. A fault is a number in the
 * handoff, never a failure of this spec; the spec fails only on a spec error (a row that did not show, a not-reached).
 *
 * PHASE: `base` (the files as the base has them) or `after`; the names of the shots carry `base-` for the first. The
 * phase only names the files and the extra checks (the dialogs, the keyboard); it changes nothing in the app.
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; never edited), a stack with `SR_ADMIN_REQUIRE_REAUTH=0`, and `psql` + `DATABASE_URL` (the seeding
 * of the job rows: see the header of `e2e/support/p100-seed.ts`).
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8782 /
 * 5782 / 9482, database sr_p101_trans_ui):
 *   SR_P101_TRANS_ON=1 SR_P101_TRANS_PHASE=after SR_E2E_BASE_URL=http://localhost:5782 SR_CLI_BIN=<sr-cli> \
 *   SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P101_TRANS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-trans \
 *   npx playwright test tests/p101-trans-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The stack: sr-server with SR_DATABASE__URL=<db> SR_DATABASE__AUTO_MIGRATE=true SR_SERVER__BIND=127.0.0.1:8782
 * SR_SERVER__PUBLIC_URL=http://localhost:5782 SR_STORAGE__ENDPOINT=memory: SR_ACTIVITIES__ENABLED=true
 * SR_SERVER__METRICS_BIND=127.0.0.1:9482 SR_ADMIN_REQUIRE_REAUTH=0; Vite with SR_DEV_API_PORT=8782 VITE_SR_E2E=1
 * VITE_ACTIVITIES_ENABLED=true on 5782.
 * Variables: SR_P101_TRANS_ON, SR_P101_TRANS_PHASE, SR_P101_TRANS_OUT. One JSON line per measure in `measure-<phase>.json`.
 */
const ON = process.env.SR_P101_TRANS_ON === '1';
const PHASE = process.env.SR_P101_TRANS_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P101_TRANS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-trans';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PREFIX = PHASE === 'base' ? 'base-' : '';

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const RUNS = [
  { key: '375', theme: 'lounge-light', w: 375, h: 812, phone: true },
  { key: '900', theme: 'lounge-light', w: 900, h: 768, phone: false },
  { key: '1024', theme: 'lounge-light', w: 1024, h: 768, phone: false },
  { key: '375', theme: 'lounge-dark', w: 375, h: 812, phone: true },
] as const;

/** Runs in the page: one job's row, picked by its id. */
function measureRow(arg: { id: string; error: string | null }) {
  const f = (n: number) => Math.round(n * 10) / 10;
  const wrap = document.querySelector<HTMLElement>('.adm-table-wrap');
  const table = wrap?.querySelector<HTMLElement>('table') ?? null;
  const de = document.documentElement;
  const tr = Array.from(table?.querySelectorAll('tbody tr') ?? []).find((r) =>
    Array.from(r.querySelectorAll('.adm-mono')).some((e) => e.textContent === arg.id),
  ) as HTMLElement | undefined;
  const base = {
    wrapScroll: wrap ? [wrap.scrollWidth, wrap.clientWidth] : null,
    wrapRole: wrap?.getAttribute('role') ?? null,
    wrapTabindex: wrap?.getAttribute('tabindex') ?? null,
    tableWidth: table ? f(table.getBoundingClientRect().width) : null,
    stacked: !!table?.classList.contains('adm-table-stack'),
    theadDisplay: table ? getComputedStyle(table.querySelector('thead')!).display : null,
    docSideways: de.scrollWidth - de.clientWidth,
  };
  if (!tr) return { ...base, found: false as const };
  const r = tr.getBoundingClientRect();
  const cells = Array.from(tr.children).map((c) => {
    const b = c.getBoundingClientRect();
    return {
      label: c.getAttribute('data-label'),
      actions: c.hasAttribute('data-actions'),
      box: [f(b.left), f(b.top - r.top), f(b.width), f(b.height)],
      display: getComputedStyle(c).display,
    };
  });
  const errEl = tr.children[3]?.querySelector<HTMLElement>('.ss-meta:last-child') ?? null;
  let err: null | {
    box: number[];
    lines: number;
    textLen: number;
    whole: boolean;
    clamp: string;
    scroll: number[];
    minInline: string;
  } = null;
  if (arg.error && errEl && (errEl.textContent ?? '').includes(arg.error.slice(0, 12))) {
    const b = errEl.getBoundingClientRect();
    const rng = document.createRange();
    rng.selectNodeContents(errEl);
    const tops = new Set(Array.from(rng.getClientRects()).map((x) => Math.round(x.top)));
    const cs = getComputedStyle(errEl);
    err = {
      box: [f(b.left - r.left), f(b.top - r.top), f(b.width), f(b.height)],
      lines: tops.size,
      textLen: (errEl.textContent ?? '').length,
      whole: (errEl.textContent ?? '') === arg.error,
      clamp: cs.getPropertyValue('-webkit-line-clamp'),
      scroll: [errEl.scrollHeight, errEl.clientHeight],
      minInline: cs.minInlineSize,
    };
  }
  const buttons = Array.from(tr.querySelectorAll('button')).map((b) => {
    const x = b.getBoundingClientRect();
    return { name: b.getAttribute('aria-label'), w: f(x.width), h: f(x.height) };
  });
  return {
    ...base,
    found: true as const,
    rowHeight: f(r.height),
    rowBox: [f(r.left), f(r.width)],
    cells,
    err,
    buttons,
  };
}

test('D-955: the Transcoding queue, measured and shot', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `measure-${PHASE}.json`);
  const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const notes: string[] = [];
  const tx = seedTranscodingRow();
  const failed = tx.jobs.find((j) => j.state === 'failed')!;
  const queued = tx.jobs.find((j) => j.state === 'queued')!;
  const running = tx.jobs.find((j) => j.state === 'running')!;
  console.log(
    `REPORT phase ${PHASE}; seeded suffix ${tx.suffix}: ${tx.jobs.map((j) => `${j.id}/${j.state}`).join(', ')}`,
  );

  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };
  const rowOf = (page: Page, id: string) =>
    page.locator('.adm-table-wrap tbody tr', { has: page.getByText(id, { exact: true }) });

  try {
    for (const s of RUNS) {
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

      const m: Record<string, unknown> = {};
      for (const j of tx.jobs)
        m[j.state] = await page.evaluate(measureRow, { id: j.id, error: j.text });
      log({ phase: PHASE, size: s.key, theme, ...m });
      const fm = m.failed as {
        rowHeight?: number;
        err?: { lines: number; whole: boolean; box: number[] };
      };
      const qm = m.queued as { rowHeight?: number };
      console.log(
        `REPORT ${tag}: ${JSON.stringify({ queued: qm.rowHeight, running: (m.running as { rowHeight?: number }).rowHeight, failedRow: fm.rowHeight, err: fm.err })}`,
      );
      console.log(`REPORT ${tag} plain row ${qm.rowHeight}, failed row ${fm.rowHeight}`);
      console.log(
        `REPORT ${tag} buttons ${JSON.stringify(tx.jobs.map((j) => (m[j.state] as { buttons?: unknown }).buttons))}`,
      );

      // The shots: top of the table, and (phone) the failed job in view.
      await page.evaluate(() => {
        window.scrollTo(0, 0);
        for (const e of Array.from(document.querySelectorAll<HTMLElement>('body *'))) {
          if (e.scrollTop > 0) e.scrollTop = 0;
        }
      });
      await page
        .locator('.adm-table-wrap')
        .first()
        .evaluate((w) => w.scrollIntoView({ block: 'start' }));
      await page.waitForTimeout(300);
      let file = `${PREFIX}transcoding-${tag}.png`;
      await page.screenshot({ path: resolve(OUT, file) });
      shots.push(file);
      if (s.phone && s.theme === 'lounge-light') {
        await rowOf(page, failed.id).evaluate((el) => el.scrollIntoView({ block: 'start' }));
        await page.waitForTimeout(300);
        file = `${PREFIX}transcoding-${s.key}-failed-${s.theme}.png`;
        await page.screenshot({ path: resolve(OUT, file) });
        shots.push(file);
      }

      // The hit test over the section; the three jobs' controls are named by their ids.
      if (s.phone) {
        await openCollapsed(page, panel);
        const rows = await hitTestAll(page, panel);
        const nr = rows.filter((r) => r.status === 'not-reached');
        if (nr.length) throw new Error(`hit test: ${nr.length} not-reached (probe error)`);
        const mine = rows.filter((r) => tx.jobs.some((j) => r.name.includes(j.id)));
        const fails = rows
          .filter((r) => r.status === 'fail')
          .map((r) => ({
            name: r.name,
            mine: tx.jobs.some((j) => r.name.includes(j.id)),
            box: r.result
              ? `${Math.round(r.result.box.width)}x${Math.round(r.result.box.height)}`
              : null,
            takers: (r.result?.points ?? [])
              .filter((p) => !p.ok)
              .map(
                (p) =>
                  `${p.at}: ${p.taker?.tag ?? 'none'} "${p.taker?.name ?? ''}" (${p.taker?.path ?? ''})`,
              ),
          }));
        log({
          phase: PHASE,
          size: s.key,
          theme,
          hit: { total: rows.length, mine: mine.length, fails },
        });
        console.log(
          `REPORT ${tag} hit: ${rows.length} controls, ${mine.length} named for my jobs, fails ${JSON.stringify(fails)}`,
        );
      }

      // Logs on the failed job: is the whole error string in the dialog?  (375 light, both phases)
      if (s.key === '375' && s.theme === 'lounge-light') {
        const open = async (job: { id: string }, re: RegExp) => {
          await rowOf(page, job.id).getByRole('button', { name: re }).scrollIntoViewIfNeeded();
          await rowOf(page, job.id).getByRole('button', { name: re }).click();
          const dlg = page.getByRole('dialog').last();
          await dlg.waitFor({ timeout: 8000 });
          await page.waitForTimeout(800);
          return dlg;
        };
        const dlg = await open(failed, /^Logs|logs/i);
        const text = ((await dlg.textContent()) ?? '').replace(/\s+/g, ' ');
        const whole = text.includes(failed.text!.replace(/\s+/g, ' '));
        const partial = text.includes('could not read the source');
        console.log(
          `REPORT ${PHASE} Logs dialog on the failed job: whole error in text ${whole}; first words ${partial}; text: ${text.slice(0, 300)}`,
        );
        log({ phase: PHASE, logsDialog: { whole, partial, text: text.slice(0, 400) } });
        await closeDlg(page, notes, 'Logs');
        if (PHASE === 'after') {
          const dQ = await open(queued, /^Cancel|cancel/i);
          console.log(
            `REPORT after Cancel dialog shows: ${((await dQ.textContent()) ?? '').replace(/\s+/g, ' ').slice(0, 120)}`,
          );
          await closeDlg(page, notes, 'Cancel');
          const dF = await open(failed, /re-?run/i);
          console.log(
            `REPORT after Re-run dialog shows: ${((await dF.textContent()) ?? '').replace(/\s+/g, ' ').slice(0, 120)}`,
          );
          await closeDlg(page, notes, 'Re-run');
          // nothing was confirmed: the three rows are still there
          for (const j of [queued, running, failed]) await expect(rowOf(page, j.id)).toHaveCount(1);
        }
      }

      // 1024 / 900: the wrapper is a named stop while it scrolls; ArrowRight reaches the Actions column.
      if (!s.phone) {
        const wrap: Locator = page.locator('.adm-table-wrap').first();
        const role = await wrap.getAttribute('role');
        if (role === 'region') {
          await wrap.focus();
          for (let i = 0; i < 12; i++) await page.keyboard.press('ArrowRight');
          await page.waitForTimeout(300);
          const end = await wrap.evaluate((w) => [w.scrollLeft, w.scrollWidth - w.clientWidth]);
          console.log(`REPORT ${tag} ArrowRight x12: scrollLeft ${end[0]} of max ${end[1]}`);
          if (s.key === '1024') {
            await wrap.evaluate((w) => w.scrollIntoView({ block: 'start' }));
            await page.waitForTimeout(300);
            file = `${PREFIX}transcoding-1024-focus-end-${s.theme}.png`;
            await page.screenshot({ path: resolve(OUT, file) });
            shots.push(file);
          }
        } else console.log(`REPORT ${tag} wrapper role ${role}: no scroll, no stop`);
      }
      await ctx.close();
    }
  } finally {
    try {
      tx.remove();
    } catch (e) {
      notes.push(`could not remove job rows: ${String(e).slice(0, 120)}`);
    }
    await ownerCtx.close();
  }
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  for (const n of notes) console.log(`REPORT note ${n}`);
});
/** Close the open dialog without confirming: Escape, else its Done / Cancel button; a note says which one worked. */
async function closeDlg(page: Page, notes: string[], what: string) {
  const open = page.getByRole('dialog');
  const base = 1; // the Admin shell is itself a dialog
  await page.keyboard.press('Escape');
  await page.waitForTimeout(600);
  if ((await open.count()) <= base) return;
  notes.push(`${what} dialog did not close on Escape; closed with its button`);
  await open
    .last()
    .getByRole('button', { name: /^(Done|Cancel)$/ })
    .first()
    .click();
  await expect(open).toHaveCount(base);
}
