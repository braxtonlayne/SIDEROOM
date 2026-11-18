import { execFileSync } from 'node:child_process';
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { DATABASE_URL, repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { seedTranscodingRow } from '../support/p100-seed';

/**
 * D-999 (wave 106, mount): the Transcoding queue's Mount cell reads the label from the job row (`mount_label`) and the
 * page makes NO request to `/admin/storage/mounts`. Rows: the three jobs of `seedTranscodingRow()` (queued, running,
 * failed; all `mount_id` 1, the instance mount), picked by the ids it returns (D-942) and removed in a `finally`. The
 * spec makes TWO REAL MOUNTS through the API under its own names (a user mount of the Owner, "p106 films" and a
 * 60-character label) and points the queued and the running job at them with one `UPDATE` of `transcode_jobs.mount_id`.
 * The failed job stays on mount 1: the instance mount is not in `storage_mounts` on a stack on `memory:` storage, so
 * it has no label and its cell reads the id alone (in the base too).
 * Writes `figures-<phase>.json` and the shots (base shots `base-` prefixed) into OUT. In the `after` phase it compares
 * with `figures-base.json` when that is in OUT: the column widths, the head boxes and the row heights must not move
 * (a limit of 0.5 px); the base makes the second request (expected), the after makes none.
 * Needs the sweep world, `psql` and `DATABASE_URL` (the runner exports it; see `p100-seed.ts`).
 *
 * The command:
 *   bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_MOUNT_ON=1 --env SR_P106_MOUNT_PHASE=after --env SR_P106_MOUNT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-mount -- tests/p106-mount-shots.spec.ts
 */
const ON = process.env.SR_P106_MOUNT_ON === '1';
const PHASE = process.env.SR_P106_MOUNT_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P106_MOUNT_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-mount';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SHORT = 'p106 films';
const LONG = 'p106 films: the long label of a mount that someone gave a very long name'.slice(
  0,
  60,
);

/** One statement on the stack's Postgres (`-c`, stdin closed); the password goes by PGPASSWORD, never printed. */
function psql(sql: string): void {
  const u = new URL(DATABASE_URL);
  const win = 'C:/Program Files/PostgreSQL/17/bin/psql.exe';
  execFileSync(
    process.env.PSQL ?? (existsSync(win) ? win : 'psql'),
    [
      '-X',
      '-q',
      '-h',
      u.hostname,
      '-p',
      u.port || '5432',
      '-U',
      decodeURIComponent(u.username),
      '-d',
      u.pathname.slice(1),
      '-v',
      'ON_ERROR_STOP=1',
      '-c',
      sql,
    ],
    {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, PGPASSWORD: decodeURIComponent(u.password) },
    },
  );
}

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const RUNS: ReadonlyArray<{
  key: string;
  theme: string;
  w: number;
  h: number;
  phone: boolean;
}> = [
  {
    key: '1024',
    theme: 'lounge-light',
    w: 1024,
    h: 768,
    phone: false,
  },
  { key: '1024', theme: 'lounge-dark', w: 1024, h: 768, phone: false },
  { key: '375', theme: 'lounge-light', w: 375, h: 812, phone: true },
];

/** Runs in the page. */
function measure(ids: string[]) {
  const f = (n: number) => Math.round(n * 10) / 10;
  const de = document.documentElement;
  const main = document.querySelector<HTMLElement>('main') ?? document.body;
  const wrap = document.querySelector<HTMLElement>('.adm-table-wrap')!;
  const table = wrap.querySelector<HTMLElement>('table')!;
  const ths = Array.from(table.querySelectorAll('thead th'));
  const rows = ids.map((id) => {
    const tr = Array.from(table.querySelectorAll('tbody tr')).find((r) =>
      r.textContent?.includes(id),
    ) as HTMLElement;
    const cell = tr.querySelector<HTMLElement>('td[data-label="Mount"]')!;
    const label = cell.querySelector<HTMLElement>('span:not(.adm-mono)');
    const err = tr.querySelector<HTMLElement>('.adm-tx-error');
    return {
      id,
      height: f(tr.getBoundingClientRect().height),
      cellText: cell.textContent,
      cellWidth: f(cell.getBoundingClientRect().width),
      labelCut: label ? label.scrollWidth > label.clientWidth : null,
      labelWidth: label ? f(label.getBoundingClientRect().width) : null,
      errHeight: err ? f(err.getBoundingClientRect().height) : null,
      errClamp: err ? getComputedStyle(err).webkitLineClamp : null,
      errTextLen: err?.textContent?.length ?? null,
    };
  });
  return {
    heads: ths.map((h) => h.textContent),
    headWidths: ths.map((h) => f(h.getBoundingClientRect().width)),
    colX: ths.map((h) => f(h.getBoundingClientRect().left)),
    tableWidth: f(table.getBoundingClientRect().width),
    wrapClient: wrap.clientWidth,
    wrapScroll: wrap.scrollWidth,
    wrapRole: wrap.getAttribute('role'),
    wrapTabindex: wrap.getAttribute('tabindex'),
    wrapLabel: wrap.getAttribute('aria-label'),
    bgImage: getComputedStyle(wrap).backgroundImage.slice(0, 60),
    mainScroll: main.scrollWidth,
    mainClient: main.clientWidth,
    docSideways: de.scrollWidth - de.clientWidth,
    rows,
    alerts: Array.from(document.querySelectorAll('[role="alert"],[role="alertdialog"]')).map(
      (a) => `${a.tagName}.${a.className}:${(a.textContent ?? '').slice(0, 40)}`,
    ),
  };
}

test('D-999: the Transcoding queue shows the mount label from the job and asks no mounts list', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `measure-${PHASE}.json`);
  const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const problems: string[] = [];
  const figures: Record<string, unknown> = {};
  const shots: string[] = [];
  const tx = seedTranscodingRow();
  const ids = tx.jobs.map((j) => j.id);
  const mounts: Array<{ id: string }> = [];
  const makeMount = async (label: string) => {
    const m = await apiPost<{ id: string }>(o, '/users/@me/storage/mounts', {
      label,
      provider: 's3',
      region: 'us-east-1',
      bucket: 'p106-mount',
      prefix: '',
      path_style: false,
      access_key_id: 'p106mountkey',
      secret_access_key: 'p106mountsecret-not-real',
    });
    mounts.push(m);
    return m;
  };
  console.log(
    `REPORT phase ${PHASE}; suffix ${tx.suffix}: ${tx.jobs.map((j) => `${j.id}/${j.state}`).join(', ')}`,
  );

  try {
    const m1 = await makeMount(SHORT);
    const m2 = await makeMount(LONG);
    psql(`UPDATE transcode_jobs SET mount_id = ${m1.id} WHERE id = ${ids[0]}`);
    psql(`UPDATE transcode_jobs SET mount_id = ${m2.id} WHERE id = ${ids[1]}`);
    console.log(
      `REPORT mounts made: ${m1.id} (${SHORT}), ${m2.id} (${LONG.length} chars); failed job stays on 1`,
    );
    for (const s of RUNS) {
      const prefs = {
        theme: s.theme,
        zoom: 100,
        sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'lounge-editorial' },
      };
      await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
      {
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
        let mountReqs = 0;
        page.on('request', (r) => {
          if (r.url().includes('/admin/storage/mounts')) mountReqs++;
        });
        const jobsResp = page.waitForResponse(
          (r) => r.url().includes('/admin/transcode/jobs') && r.request().method() === 'GET',
        );
        await page.goto('/admin/cinema-transcoding', {
          waitUntil: 'domcontentloaded',
          timeout: 15_000,
        });
        const jr = await jobsResp;
        const jobs = (await jr.json()) as Array<{
          job_id?: string;
          id?: string;
          mount_id?: string | null;
          mount_label?: string | null;
        }>;
        await page.locator('.adm-table-wrap table').first().waitFor({ timeout: 10_000 });
        await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
        await page.waitForTimeout(600);
        const reqsAtLoad = mountReqs;
        if (s.key === '1024' && s.theme === 'lounge-light') {
          const list = await page.evaluate(async () => {
            const r = await fetch('/api/v1/admin/storage/mounts?limit=50', {
              credentials: 'include',
            });
            const body = r.ok
              ? ((await r.json()) as Array<{ id: string; label: string; owner_kind: string }>)
              : null;
            return {
              status: r.status,
              rows: body?.map((m) => `${m.id}:${m.owner_kind}:${m.label}`) ?? null,
            };
          });
          console.log(
            `REPORT STEP1 jobs mount_ids ${JSON.stringify(jobs.map((j) => [j.job_id ?? j.id, j.mount_id]))}; mounts list (the page's own fetch) ${JSON.stringify(list)}`,
          );
        }
        const theme = await page.evaluate(
          () =>
            document.querySelector('.ss-root[data-theme]')?.getAttribute('data-theme') ?? 'none',
        );
        expect(theme, 'data-theme read back').toBe(s.theme);
        const m = await page.evaluate(measure, ids);
        log({ phase: PHASE, tag, mountReqs: reqsAtLoad, ...m });
        figures[tag] = {
          mountRequests: reqsAtLoad,
          jobLabels: ids.map(
            (id) => jobs.find((j) => (j.job_id ?? j.id) === id)?.mount_label ?? null,
          ),
          cells: ids.map((id) => m.rows.find((r) => r.id === id)!.cellText),
          heads: m.heads,
          headWidths: m.headWidths,
          colX: m.colX,
          tableWidth: m.tableWidth,
          wrapClient: m.wrapClient,
          wrapScroll: m.wrapScroll,
          rowHeights: ids.map((id) => m.rows.find((r) => r.id === id)!.height),
          mountCellWidths: ids.map((id) => m.rows.find((r) => r.id === id)!.cellWidth),
          errHeights: ids.map((id) => m.rows.find((r) => r.id === id)!.errHeight),
        };
        console.log(`REPORT ${tag} mountReqs ${mountReqs} ${JSON.stringify(m)}`);
        if (m.mainScroll > m.mainClient)
          problems.push(`${tag}: main scrolls sideways ${m.mainScroll}/${m.mainClient}`);
        if (m.docSideways > 0) problems.push(`${tag}: document scrolls sideways`);
        // An empty live region (the shell's) is always there; an alert with text is a toast or an error banner.
        const alertsNow = m.alerts.filter((x) => !x.endsWith(':'));
        if (alertsNow.length) problems.push(`${tag}: alert with text ${JSON.stringify(alertsNow)}`);
        const want = [`${SHORT} ${mounts[0]!.id}`, `${LONG} ${mounts[1]!.id}`, '1'];
        const got = ids.map((id) => m.rows.find((r) => r.id === id)!.cellText);
        if (PHASE === 'after') {
          // D-999: no request to the mounts list; the labels ride on the jobs response.
          if (reqsAtLoad !== 0)
            problems.push(`${tag}: ${reqsAtLoad} request(s) to /admin/storage/mounts`);
          const fromJobs = ids.map(
            (id) => jobs.find((j) => (j.job_id ?? j.id) === id)?.mount_label ?? null,
          );
          if (JSON.stringify(fromJobs) !== JSON.stringify([SHORT, LONG, null]))
            problems.push(`${tag}: jobs response labels ${JSON.stringify(fromJobs)}`);
          if (JSON.stringify(got) !== JSON.stringify(want))
            problems.push(
              `${tag}: Mount cells ${JSON.stringify(got)}, wanted ${JSON.stringify(want)}`,
            );
        } else {
          // The base asks the list a second time (expected); its cells are logged, not asserted.
          console.log(
            `REPORT ${tag} base: ${reqsAtLoad} mount list request(s); cells ${JSON.stringify(got)}`,
          );
        }
        if (!s.phone && m.wrapRole !== 'region')
          problems.push(`${tag}: the wrapper is not the keyboard region`);

        const names =
          PHASE === 'base'
            ? s.key === '1024' && s.theme === 'lounge-light'
              ? ['base-transcoding-1024-light.png']
              : []
            : [`transcoding-${s.key}-${s.theme.replace('lounge-', '')}.png`];
        for (const name of names) {
          await page.evaluate(() => window.scrollTo(0, 0));
          await page.locator('.adm-table-wrap').evaluate((w) => {
            w.scrollIntoView({ block: 'start' });
            w.scrollLeft = 0;
          });
          await page.mouse.move(0, 0);
          await page.waitForTimeout(250);
          await page.screenshot({ path: resolve(OUT, name) });
          shots.push(name);
        }

        if (s.phone) {
          const opened = await openCollapsed(page, page.getByRole('main'));
          const rows = await hitTestAll(page, page.getByRole('main'));
          const bad = rows.filter((r) => r.status !== 'pass' && r.status !== 'skipped');
          console.log(
            `REPORT ${tag} hit test: opened ${opened.length}, rows ${rows.length}, not pass ${bad.length} ${JSON.stringify(bad.map((b) => [b.name, b.status, b.reason]))}`,
          );
          log({ phase: PHASE, tag, hit: { rows: rows.length, bad: bad.length } });
          const btnH = await page.evaluate(() =>
            Array.from(document.querySelectorAll('.adm-table button')).map((b) =>
              Math.round(b.getBoundingClientRect().height),
            ),
          );
          console.log(`REPORT ${tag} table button heights ${JSON.stringify(btnH)}`);
          if (btnH.some((h) => h < 44)) problems.push(`${tag}: a table button under 44 px`);
          if (bad.length) problems.push(`${tag}: ${bad.length} hit test rows not pass`);
        }
        await ctx.close();
      }
    }
  } finally {
    if (mounts.length) {
      try {
        psql(`DELETE FROM storage_mounts WHERE id IN (${mounts.map((m) => m.id).join(',')})`);
        console.log(`REPORT mounts removed: ${mounts.map((m) => m.id).join(', ')}`);
      } catch (e) {
        console.log(`REPORT note could not remove the mounts: ${String(e).slice(0, 120)}`);
      }
    }
    try {
      tx.remove();
    } catch (e) {
      console.log(`REPORT note could not remove job rows: ${String(e).slice(0, 120)}`);
    }
    await ownerCtx.close();
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figures, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<
      string,
      Record<string, unknown>
    >;
    const near = (x: unknown, y: unknown, tol = 0.5): boolean =>
      Array.isArray(x) && Array.isArray(y)
        ? x.length === y.length && x.every((v, i) => near(v, y[i], tol))
        : typeof x === 'number' && typeof y === 'number'
          ? Math.abs(x - y) <= tol
          : x === y;
    for (const [tag, fig] of Object.entries(figures) as Array<[string, Record<string, unknown>]>) {
      const b = base[tag];
      if (!b) continue;
      // (A stacked row at 375 holds a job id and an error text with a random seed suffix, whose wrapping differs by a
      // pixel from run to run: the 375 row heights have 1.5 px, every other figure 0.5 px.)
      // Must not change: the columns, the heads, the rows and the Mount cell's own box; the cell text may gain the label.
      for (const k of [
        'heads',
        'headWidths',
        'colX',
        'tableWidth',
        'wrapClient',
        'wrapScroll',
        'rowHeights',
        'mountCellWidths',
        'errHeights',
      ])
        if (!near(fig[k], b[k], k === 'rowHeights' && tag.startsWith('375') ? 1.5 : 0.5))
          problems.push(
            `${tag}: ${k} moved, base ${JSON.stringify(b[k])}, after ${JSON.stringify(fig[k])}`,
          );
    }
  }
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT problems ${problems.length}: ${JSON.stringify(problems)}`);
  expect(problems, 'problem lines').toEqual([]);
});
