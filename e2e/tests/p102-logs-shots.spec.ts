import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { seedTranscodingRow } from '../support/p100-seed';

/**
 * D-962 (wave 102, logs; SEC-AUD-448). Admin > Cinema transcoding > Queue with the three rows of `seedTranscodingRow()`
 * (queued, running, failed with a long error holding an unbroken file name), picked by the ids the function returns
 * (D-942) and removed again in a `finally`. Measures the three rows' boxes at 1024 / 900 / 375 (touch) in Lounge Light
 * and once at 375 in Lounge Dark; opens Logs on the failed and on the queued job (the dialog's text, its heading, the
 * pre, sideways scroll, Tab reaching the pre, a hit test over the dialog at 375). A fault is a number in the handoff;
 * the spec fails only on a spec error (a row that did not show, a not-reached).
 *
 * PHASE `base` (the two app files put back to the base's text by the round's script) or `after`; shots of the base carry
 * `base-` and are only the two the task names. Needs the sweep world (`e2e/.state/sweep/owner.json`), a stack with
 * `SR_ADMIN_REQUIRE_REAUTH=0`, and `psql` + `DATABASE_URL` (see the header of `e2e/support/p100-seed.ts`).
 *
 * The command (from `e2e/`, after the sweep world is seeded on the same database; ports 8783 / 5783 / 9483, database
 * sr_p102_logs_ui):
 *   SR_P102_LOGS_ON=1 SR_P102_LOGS_PHASE=after SR_E2E_BASE_URL=http://localhost:5783 SR_CLI_BIN=<sr-cli> \
 *   SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P102_LOGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-logs \
 *   npx playwright test tests/p102-logs-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Stack: sr-server with SR_DATABASE__URL=<db> SR_DATABASE__AUTO_MIGRATE=true SR_SERVER__BIND=127.0.0.1:8783
 * SR_SERVER__PUBLIC_URL=http://localhost:5783 SR_STORAGE__ENDPOINT=memory: SR_ACTIVITIES__ENABLED=true
 * SR_SERVER__METRICS_BIND=127.0.0.1:9483 SR_ADMIN_REQUIRE_REAUTH=0; Vite with SR_DEV_API_PORT=8783 VITE_SR_E2E=1
 * VITE_ACTIVITIES_ENABLED=true on 5783. One JSON line per measure in `measure-<phase>.json`.
 * Variables: SR_P102_LOGS_ON, SR_P102_LOGS_PHASE, SR_P102_LOGS_OUT.
 */
const ON = process.env.SR_P102_LOGS_ON === '1';
const PHASE = process.env.SR_P102_LOGS_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P102_LOGS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-logs';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PREFIX = PHASE === 'base' ? 'base-' : '';

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const RUNS = [
  { key: '1024', theme: 'lounge-light', w: 1024, h: 768, phone: false },
  { key: '900', theme: 'lounge-light', w: 900, h: 768, phone: false },
  { key: '375', theme: 'lounge-light', w: 375, h: 812, phone: true },
  { key: '375', theme: 'lounge-dark', w: 375, h: 812, phone: true },
] as const;

/** Runs in the page: the box of one job's row and of its State cell's error, picked by the job id. */
function measureRows(arg: { ids: string[]; error: string }) {
  const f = (n: number) => Math.round(n * 10) / 10;
  const de = document.documentElement;
  const table = document.querySelector<HTMLElement>('.adm-table-wrap table');
  const out: Record<string, unknown> = { docSideways: de.scrollWidth - de.clientWidth };
  for (const id of arg.ids) {
    const tr = Array.from(table?.querySelectorAll('tbody tr') ?? []).find((r) =>
      Array.from(r.querySelectorAll('.adm-mono')).some((e) => e.textContent === id),
    );
    if (!tr) {
      out[id] = null;
      continue;
    }
    const r = tr.getBoundingClientRect();
    const errEl = tr.querySelector<HTMLElement>('.adm-tx-error');
    let err = null;
    if (errEl) {
      const b = errEl.getBoundingClientRect();
      const lh = parseFloat(getComputedStyle(errEl).lineHeight) || 0;
      err = {
        box: [f(b.width), f(b.height)],
        visibleLines: lh ? Math.round(b.height / lh) : null,
        clamp: getComputedStyle(errEl).getPropertyValue('-webkit-line-clamp'),
        whole: (errEl.textContent ?? '') === arg.error,
        scroll: [errEl.scrollHeight, errEl.clientHeight],
      };
    }
    out[id] = {
      row: [f(r.width), f(r.height)],
      cells: Array.from(tr.children).map((c) => {
        const b = c.getBoundingClientRect();
        return [f(b.left - r.left), f(b.top - r.top), f(b.width), f(b.height)];
      }),
      err,
    };
  }
  return out;
}

/** Runs in the page: the Logs dialog's blocks. */
function measureDialog(error: string) {
  const dlg = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).pop()!;
  const de = document.documentElement;
  const text = (dlg.textContent ?? '').replace(/\s+/g, ' ');
  const pres = Array.from(dlg.querySelectorAll<HTMLElement>('pre')).map((p) => {
    const lab = p.getAttribute('aria-labelledby');
    return {
      label: lab
        ? (document.getElementById(lab)?.textContent ?? null)
        : p.getAttribute('aria-label'),
      labelledby: !!lab,
      tabindex: p.getAttribute('tabindex'),
      scroll: [p.scrollWidth, p.clientWidth],
      text: (p.textContent ?? '').slice(0, 60),
    };
  });
  return {
    wholeError: text.includes(error.replace(/\s+/g, ' ')),
    headings: Array.from(dlg.querySelectorAll('h3')).map((h) => h.textContent),
    pres,
    dlgScroll: [dlg.scrollWidth, dlg.clientWidth],
    docSideways: de.scrollWidth - de.clientWidth,
    text: text.slice(0, 260),
  };
}

test('D-962: the Logs dialog shows a failed job error; the State cell clamps it', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `measure-${PHASE}.json`);
  const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const tx = seedTranscodingRow();
  const failed = tx.jobs.find((j) => j.state === 'failed')!;
  const queued = tx.jobs.find((j) => j.state === 'queued')!;
  const error = failed.text!;
  console.log(
    `REPORT phase ${PHASE}; suffix ${tx.suffix}: ${tx.jobs.map((j) => `${j.id}/${j.state}`).join(', ')}`,
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

      const rows = (await page.evaluate(measureRows, {
        ids: tx.jobs.map((j) => j.id),
        error,
      })) as Record<string, unknown>;
      const named = Object.fromEntries(tx.jobs.map((j) => [j.state, rows[j.id]]));
      log({ phase: PHASE, size: s.key, theme, docSideways: rows.docSideways, ...named });
      console.log(`REPORT ${tag} rows ${JSON.stringify(named)} docSideways ${rows.docSideways}`);

      if (s.theme === 'lounge-light' && s.key !== '900') {
        await page.evaluate(() => window.scrollTo(0, 0));
        await rowOf(page, failed.id).evaluate((el) => el.scrollIntoView({ block: 'center' }));
        await page.waitForTimeout(300);
        const name = `${PREFIX}queue-${tag}.png`;
        if (PHASE === 'after' || s.key === '1024') {
          await page.screenshot({ path: resolve(OUT, name) });
          shots.push(name);
        }
      }

      const open = async (id: string) => {
        const btn = rowOf(page, id).getByRole('button', { name: /^Logs|logs/i });
        await btn.scrollIntoViewIfNeeded();
        await btn.click();
        const dlg = page.getByRole('dialog').last();
        await dlg.getByRole('button', { name: /^Done$/ }).waitFor({ timeout: 8000 });
        await page.waitForTimeout(800);
        return dlg;
      };
      const close = async () => {
        await page.keyboard.press('Escape');
        await page.waitForTimeout(500);
        if ((await page.getByRole('dialog').count()) > 1)
          await page
            .getByRole('dialog')
            .last()
            .getByRole('button', { name: /^Done$/ })
            .click();
        await expect(page.getByRole('dialog')).toHaveCount(1);
      };
      if (s.key !== '900') {
        const dlg = await open(failed.id);
        const dm = await page.evaluate(measureDialog, error);
        log({ phase: PHASE, size: s.key, theme, logsFailed: dm });
        console.log(`REPORT ${tag} Logs failed ${JSON.stringify(dm)}`);
        if (PHASE === 'after') {
          expect(dm.wholeError, 'the whole error is in the Logs dialog').toBe(true);
          expect(dm.headings, 'the Error heading').toContain('Error');
        }
        if (PHASE === 'after' || (s.key === '1024' && s.theme === 'lounge-light')) {
          const name = `${PREFIX}logs-failed-${tag}.png`;
          await page.screenshot({ path: resolve(OUT, name) });
          shots.push(name);
        }
        if (PHASE === 'after') {
          // Tab from the heading reaches the error pre (a named stop).
          await dlg
            .locator('h3')
            .first()
            .evaluate((h) => h.setAttribute('tabindex', '-1'));
          await dlg.locator('h3').first().focus();
          await page.keyboard.press('Tab');
          const focused = await page.evaluate(() => {
            const a = document.activeElement as HTMLElement | null;
            return { tag: a?.tagName, labelledby: a?.getAttribute('aria-labelledby') ?? null };
          });
          console.log(`REPORT ${tag} Tab from the heading lands on ${JSON.stringify(focused)}`);
          expect(focused.tag, 'Tab reaches the error pre').toBe('PRE');
          if (s.phone) {
            await openCollapsed(page, dlg);
            const hr = await hitTestAll(page, dlg);
            const nr = hr.filter((r) => r.status === 'not-reached');
            if (nr.length) throw new Error(`hit test over the dialog: ${nr.length} not-reached`);
            const fails = hr
              .filter((r) => r.status === 'fail')
              .map((r) => ({
                name: r.name,
                takers: (r.result?.points ?? [])
                  .filter((p) => !p.ok)
                  .map(
                    (p) =>
                      `${p.at}: ${p.taker?.tag ?? 'none'} "${p.taker?.name ?? ''}" (${p.taker?.path ?? ''})`,
                  ),
              }));
            log({ phase: PHASE, size: s.key, theme, dialogHit: { total: hr.length, fails } });
            console.log(
              `REPORT ${tag} dialog hit: ${hr.length} controls, fails ${JSON.stringify(fails)}`,
            );
          }
        }
        await close();
        if (s.theme === 'lounge-light') {
          await open(queued.id);
          const qm = await page.evaluate(measureDialog, error);
          log({ phase: PHASE, size: s.key, theme, logsQueued: qm });
          console.log(`REPORT ${tag} Logs queued ${JSON.stringify(qm)}`);
          if (PHASE === 'after')
            expect(qm.headings, 'no Error heading on the queued job').not.toContain('Error');
          if (s.key === '1024') {
            const name = `${PREFIX}logs-queued-${tag}.png`;
            await page.screenshot({ path: resolve(OUT, name) });
            shots.push(name);
          }
          await close();
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
});
