import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';
import { edgeStrips } from '../support/edge-strips';

/**
 * SEC-AUD-454 (wave 102; D-965). Admin > Donations > Invoices: the edge shade (SEC-AUD-438) must leave no 1 px line at an
 * edge that is at its end, whatever fraction the table's width has. One run holds three modes on the same page: `off`
 * (the shade block switched off by an injected style), `base` (an injected style that restores the wave 98 declarations
 * word for word, BASE_CSS below) and `after` (the sheet as it is in the tree, no injection). The strips are the two 16 px
 * columns at the wrapper's edges, cut with `edgeStrips` and decoded in the page (a canvas; no image library).
 *
 * TEST HARNESS STEP (not the page's behaviour): for each fraction f in 0, 0.25, 0.5, 0.75 the spec sets the table's
 * `inline-size` (and `min-inline-size`) by `evaluate` to max(wrapper clientWidth + 81, ceil(natural width)) + f px, so the
 * content is wider than the wrapper by a fractional amount. It reads the table's `getBoundingClientRect().width` and the
 * wrapper's `scrollWidth` / `clientWidth` back and reports them. The real order is checked too: run `p100-tables-shots`
 * (after phase) and then `p98-cue-shots` in the same world (the plan's lessons: the neighbours' data).
 *
 * A strip is "flat" when every pixel equals the same strip of the `off` mode; at an edge that is at its end the strip
 * must be flat; at an edge with more beyond it the strip must differ, and its darkest column (the largest, over columns,
 * of the mean over rows of the largest channel difference to `off`) must be within 2 levels of the base's.
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`,
 * `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`
 * (an invoice needs a price quote; a localhost stub is refused). It registers two accounts (own names, D-942) and makes two
 * invoices; the world keeps them (invoices cannot be deleted).
 *
 * The command the commander runs (from `e2e/`, sweep world seeded on the same database; ports 8782 / 5782):
 *   SR_P102_SHADE_ON=1 SR_E2E_BASE_URL=http://localhost:5782 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   SR_P102_SHADE_OUT=<folder> npx playwright test tests/p102-shade-shots.spec.ts --project=chromium --workers=1 \
 *   --retries=0 --reporter=line
 */
const ON = process.env.SR_P102_SHADE_ON === '1';
const OUT =
  process.env.SR_P102_SHADE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-shade';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const WRAP = '.adm-table-wrap:has(> .inv-table)';
const STYLE_ID = 'p102-shade-mode';
const TPUB =
  'tpubDCxX2sYFS5bDkSe5GKKYHjBW7tgyN1R3UchpLJvdbf54ohxeGRtd8MbDUe1cguVHe4vnK68DsuD5MXjxi9EXx16rb9EnNsaF5KT99CinaJz';
const FRACTIONS = [0, 0.25, 0.5, 0.75];
const WIDTHS = [1024, 900];

/** The wave 98 shade block, as it stood in the base (donations.css, SEC-AUD-438), restored by an injected style. */
const BASE_CSS = `@media (min-width: 721px) {
  .adm-table-wrap:has(> .inv-table) {
    background-image:
      linear-gradient(to right, var(--ui-bg) 50%, transparent),
      linear-gradient(to left, var(--ui-bg) 50%, transparent),
      linear-gradient(to right, color-mix(in srgb, var(--ui-text) 16%, transparent), transparent),
      linear-gradient(to left, color-mix(in srgb, var(--ui-text) 16%, transparent), transparent) !important;
    background-position: left center, right center, left center, right center !important;
    background-size: 32px 100%, 32px 100%, 16px 100%, 16px 100% !important;
    background-attachment: local, local, scroll, scroll !important;
    background-repeat: no-repeat !important;
  }
}`;
const OFF_CSS = `.adm-table-wrap:has(> .inv-table){background-image:none !important}`;

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

type Mode = 'off' | 'base' | 'after';
type Where = 'start' | 'end';
interface Px {
  w: number;
  h: number;
  data: number[];
}
interface Cmp {
  diffCols: { x: number; delta: number; colour: string }[];
  darkest: number;
}

const THEMES = [
  { key: 'light', theme: 'lounge-light' },
  { key: 'dark', theme: 'lounge-dark' },
] as const;

async function seedRows(ownerReq: APIRequestContext, fresh: () => Promise<APIRequestContext>) {
  const log: string[] = [];
  const step = async <T>(name: string, fn: () => Promise<T>) => {
    try {
      const r = await fn();
      log.push(`ok ${name}`);
      return r;
    } catch (e) {
      log.push(`FAIL ${name}: ${String(e).slice(0, 300)}`);
      return null;
    }
  };
  await step('wallet', () =>
    apiPut(ownerReq, '/admin/donations/wallet', {
      descriptor: `wpkh(${TPUB}/0/*)`,
      network: 'signet',
      reason: 'wave 102 shade shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 102 shade shots: donations on for the dev stack',
    }),
  );
  const people: { req: APIRequestContext }[] = [];
  for (const [label, name] of [
    ['p102shadeone', 'Bartholomew Featherstonehaugh-Vi'],
    ['p102shadetwo', 'Wren Alder-Okonkwo'],
  ] as const) {
    const anon = await fresh();
    const user = await registerUser(anon, label);
    const req = await fresh();
    await loginApi(req, user);
    await step(`name ${label}`, () => apiPatch(req, '/users/@me', { global_name: name }));
    people.push({ req });
  }
  await step('invoice 0', () =>
    apiPost(people[0]!.req, '/donations/invoices', { purpose: 'patron', months: 12 }),
  );
  await step('invoice 1', () =>
    apiPost(people[1]!.req, '/donations/invoices', { purpose: 'donate', usd_cents: 4999 }),
  );
  return log;
}

test('SEC-AUD-454: the Invoices edge shade leaves no line at an edge that is at its end', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const problems: string[] = [];
  const notes: string[] = [];
  const table: string[] = [];
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };
  const seedLog = await seedRows(
    o,
    async () => (await browser.newContext({ baseURL: ORIGIN })).request,
  );
  for (const l of seedLog) console.log(`REPORT seed ${l}`);
  writeFileSync(resolve(OUT, 'seed-log.txt'), seedLog.join('\n'));
  if (seedLog.some((l) => l.startsWith('FAIL invoice')))
    problems.push('an invoice was not seeded (stack error: rate source?)');

  for (const s of THEMES) {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${s.theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
    const mkCtx = async (w: number, h: number, phone: boolean) => {
      const ctx = await browser.newContext({
        baseURL: ORIGIN,
        viewport: { width: w, height: h },
        storageState: await ownerCtx.storageState(),
        bypassCSP: true,
        ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
      });
      await ctx.addInitScript((th) => {
        try {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: { theme: th, zoom: 100 } }, version: 2 }),
          );
        } catch {
          /* no storage */
        }
      }, s.theme);
      const page = await ctx.newPage();
      page.setDefaultTimeout(8000);
      return { ctx, page };
    };
    const goTo = async (page: Page) => {
      await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
      await idle(page, 4000);
      await page
        .locator(NAV)
        .filter({ hasText: /^Invoices$/ })
        .first()
        .evaluate((el) => (el as HTMLElement).click());
      await idle(page, 2500);
      await page.locator(WRAP).first().waitFor({ timeout: 8000 });
    };
    const themeOf = (page: Page) =>
      page.evaluate(
        () =>
          (
            document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
          )?.getAttribute('data-theme') ?? 'no-theme',
      );
    const setMode = (page: Page, mode: Mode) =>
      page.evaluate(
        ([id, css, m]) => {
          document.getElementById(id as string)?.remove();
          if (m === 'after') return;
          const st = document.createElement('style');
          st.id = id as string;
          st.textContent = css as string;
          document.head.appendChild(st);
        },
        [STYLE_ID, mode === 'off' ? OFF_CSS : BASE_CSS, mode] as const,
      );
    /** TEST HARNESS: forces a fractional content width (see the header). */
    const setFraction = (page: Page, f: number) =>
      page.evaluate(
        ([sel, fr]) => {
          const w = document.querySelector<HTMLElement>(sel as string)!;
          const t = w.querySelector<HTMLElement>('.inv-table')!;
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
    const figures = (page: Page) =>
      page.evaluate((sel) => {
        const w = document.querySelector<HTMLElement>(sel)!;
        const t = w.querySelector<HTMLElement>('.inv-table')!;
        return {
          tableWidth: t.getBoundingClientRect().width,
          scrollWidth: w.scrollWidth,
          clientWidth: w.clientWidth,
          scrollLeft: w.scrollLeft,
          bgImage: getComputedStyle(w).backgroundImage,
          docOver: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        };
      }, WRAP);
    const scrollTo = async (page: Page, where: Where) => {
      await page.locator(WRAP).evaluate((w, wh) => {
        w.scrollIntoView({ block: 'center' });
        w.scrollLeft = wh === 'start' ? 0 : w.scrollWidth;
      }, where);
      await page.mouse.move(0, 0);
      await page.waitForTimeout(150);
    };
    const decode = (page: Page, png: Buffer): Promise<Px> =>
      page.evaluate(async (b64) => {
        const bin = atob(b64);
        const bytes = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
        const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
        const c = new OffscreenCanvas(bmp.width, bmp.height);
        const x = c.getContext('2d')!;
        x.drawImage(bmp, 0, 0);
        return {
          w: bmp.width,
          h: bmp.height,
          data: Array.from(x.getImageData(0, 0, bmp.width, bmp.height).data),
        };
      }, png.toString('base64'));
    /** A strip enlarged 16 times, no smoothing, to a PNG file. */
    const zoomTo = async (page: Page, png: Buffer, file: string) => {
      const b64 = await page.evaluate(async (src) => {
        const bin = atob(src);
        const bytes = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
        const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
        const c = document.createElement('canvas');
        c.width = bmp.width * 16;
        c.height = Math.min(bmp.height, 60) * 16;
        const x = c.getContext('2d')!;
        x.imageSmoothingEnabled = false;
        x.drawImage(bmp, 0, 0, bmp.width, Math.min(bmp.height, 60), 0, 0, c.width, c.height);
        return c.toDataURL('image/png').split(',')[1]!;
      }, png.toString('base64'));
      writeFileSync(resolve(OUT, file), Buffer.from(b64, 'base64'));
      shots.push(file);
    };
    const compare = (a: Px, off: Px): Cmp => {
      const diffCols: Cmp['diffCols'] = [];
      let darkest = 0;
      for (let x = 0; x < a.w; x++) {
        let sum = 0;
        let first = '';
        let any = false;
        for (let y = 0; y < a.h; y++) {
          const i = (y * a.w + x) * 4;
          let m = 0;
          for (let k = 0; k < 4; k++) m = Math.max(m, Math.abs(a.data[i + k]! - off.data[i + k]!));
          sum += m;
          if (m > 0 && !any) {
            any = true;
            first = `rgba(${a.data[i]},${a.data[i + 1]},${a.data[i + 2]},${a.data[i + 3]}) vs off rgba(${off.data[i]},${off.data[i + 1]},${off.data[i + 2]},${off.data[i + 3]})`;
          }
        }
        const mean = sum / a.h;
        if (any) diffCols.push({ x, delta: Math.round(mean * 10) / 10, colour: first });
        darkest = Math.max(darkest, mean);
      }
      return { diffCols, darkest: Math.round(darkest * 10) / 10 };
    };
    /** Both strips of the wrapper in the three modes, decoded. */
    const readAll = async (page: Page) => {
      const px = {} as Record<Mode, { l: Px; r: Px }>;
      const png = {} as Record<Mode, { l: Buffer; r: Buffer }>;
      for (const m of ['off', 'base', 'after'] as Mode[]) {
        await setMode(page, m);
        await page.waitForTimeout(80);
        const e = await edgeStrips(page, WRAP, { width: 16, maxHeight: 200 });
        px[m] = { l: await decode(page, e.l), r: await decode(page, e.r) };
        png[m] = { l: e.l, r: e.r };
      }
      return { px, png };
    };
    const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);

    // ------------------------------------------------------------ desktop: 1024 then 900
    const d = await mkCtx(1024, 768, false);
    const page = d.page;
    await goTo(page);
    const th = await themeOf(page);
    console.log(`REPORT ${s.key}: data-theme ${th}`);
    if (th !== s.theme) problems.push(`${s.key}: data-theme reads ${th}, not ${s.theme}`);

    for (const w of WIDTHS) {
      await page.setViewportSize({ width: w, height: 768 });
      await page.waitForTimeout(400);
      for (const f of FRACTIONS) {
        const set = await setFraction(page, f);
        await page.waitForTimeout(150);
        for (const where of ['start', 'end'] as Where[]) {
          await scrollTo(page, where);
          const fig = await figures(page);
          const all = await readAll(page);
          const flatSide: 'l' | 'r' = where === 'start' ? 'l' : 'r';
          const moreSide: 'l' | 'r' = where === 'start' ? 'r' : 'l';
          const cmp = (mode: 'base' | 'after', side: 'l' | 'r') =>
            compare(all.px[mode][side], all.px.off[side]);
          const bFlat = cmp('base', flatSide);
          const aFlat = cmp('after', flatSide);
          const bMore = cmp('base', moreSide);
          const aMore = cmp('after', moreSide);
          const cols = (c: Cmp) =>
            c.diffCols.length
              ? c.diffCols.map((d2) => `x${d2.x}(${d2.delta}) ${d2.colour}`).join('; ')
              : 'flat';
          const line = `${s.key} ${w} f${f} ${where}: table ${fig.tableWidth} wrap ${fig.clientWidth}/${fig.scrollWidth} scrollLeft ${fig.scrollLeft} | flat strip ${flatSide.toUpperCase()}: base [${cols(bFlat)}] after [${cols(aFlat)}] | shaded strip ${moreSide.toUpperCase()}: base darkest ${bMore.darkest} (${bMore.diffCols.length} cols) after darkest ${aMore.darkest} (${aMore.diffCols.length} cols)`;
          table.push(line);
          console.log(`REPORT ${line}`);
          log({ s: s.key, w, f, where, set, fig, bFlat, aFlat, bMore, aMore });
          if (Math.abs(fig.tableWidth - set.target) > 0.02)
            notes.push(`${s.key} ${w} f${f}: table width ${fig.tableWidth}, asked ${set.target}`);
          if (fig.scrollWidth <= fig.clientWidth)
            problems.push(`${line.slice(0, 40)}: wrapper does not scroll`);
          if (aFlat.diffCols.length)
            problems.push(
              `${s.key} ${w} f${f} ${where}: a line at the ${flatSide} edge (cols ${aFlat.diffCols.map((c) => c.x)})`,
            );
          if (!aMore.diffCols.length)
            problems.push(`${s.key} ${w} f${f} ${where}: no shade at the ${moreSide} edge`);
          if (Math.abs(aMore.darkest - bMore.darkest) > 2)
            problems.push(
              `${s.key} ${w} f${f} ${where}: darkest column ${aMore.darkest} vs base ${bMore.darkest}`,
            );
          // Shots at 1024, f 0.5.
          if (w === 1024 && f === 0.5 && where === 'end' && s.key === 'light') {
            await setMode(page, 'base');
            await page.screenshot({
              path: resolve(OUT, `base-shade-1024-end-f50-lounge-${s.key}.png`),
            });
            shots.push(`base-shade-1024-end-f50-lounge-${s.key}.png`);
            await setMode(page, 'after');
            await page.screenshot({ path: resolve(OUT, `shade-1024-end-f50-lounge-${s.key}.png`) });
            shots.push(`shade-1024-end-f50-lounge-${s.key}.png`);
            await zoomTo(page, all.png.base.r, 'base-strip-r-end-f50.png');
            await zoomTo(page, all.png.after.r, 'strip-r-end-f50.png');
          }
          if (w === 1024 && f === 0.5 && where === 'start' && s.key === 'dark') {
            await setMode(page, 'after');
            await page.screenshot({ path: resolve(OUT, `shade-1024-start-lounge-${s.key}.png`) });
            shots.push(`shade-1024-start-lounge-${s.key}.png`);
          }
        }
      }
    }
    await setMode(page, 'after');
    await d.ctx.close().catch(() => undefined);

    // ------------------------------------------------------------ 1280: the table fits (own two invoices)
    const fit = await mkCtx(1280, 800, false);
    await goTo(fit.page);
    await fit.page.locator(WRAP).evaluate((w) => w.scrollIntoView({ block: 'center' }));
    const ff = await figures(fit.page);
    const fits = ff.scrollWidth <= ff.clientWidth + 1;
    console.log(`REPORT ${s.key} 1280: wrapper ${ff.clientWidth}/${ff.scrollWidth} fits ${fits}`);
    if (!fits)
      notes.push(`${s.key} 1280: the table does not fit (${ff.clientWidth}/${ff.scrollWidth})`);
    else {
      await setMode(fit.page, 'off');
      const eo = await edgeStrips(fit.page, WRAP, { width: 16, maxHeight: 200 });
      await setMode(fit.page, 'base');
      const eb = await edgeStrips(fit.page, WRAP, { width: 16, maxHeight: 200 });
      console.log(
        `REPORT ${s.key} 1280 fits BASE: strips equal to off L${eb.l.equals(eo.l)} R${eb.r.equals(eo.r)}`,
      );
      await setMode(fit.page, 'after');
      const ea = await edgeStrips(fit.page, WRAP, { width: 16, maxHeight: 200 });
      const lEq = ea.l.equals(eo.l);
      const rEq = ea.r.equals(eo.r);
      console.log(`REPORT ${s.key} 1280 fits: strips equal to off L${lEq} R${rEq}`);
      if (!lEq || !rEq) problems.push(`${s.key} 1280: a background shows where the table fits`);
      if (s.key === 'light') {
        await fit.page.screenshot({ path: resolve(OUT, `shade-1280-fits-lounge-${s.key}.png`) });
        shots.push(`shade-1280-fits-lounge-${s.key}.png`);
      }
    }
    await fit.ctx.close().catch(() => undefined);

    // ------------------------------------------------------------ phone 375 (stacked): no shade
    if (s.key === 'light') {
      const p = await mkCtx(375, 812, true);
      await goTo(p.page);
      const pf = await figures(p.page);
      console.log(`REPORT phone 375: bg ${pf.bgImage} doc over ${pf.docOver}`);
      if (pf.bgImage !== 'none') problems.push(`375: wrapper background-image ${pf.bgImage}`);
      await p.ctx.close().catch(() => undefined);
    }
  }
  await ownerCtx.close();
  writeFileSync(resolve(OUT, 'table.txt'), table.join('\n'));
  for (const n of notes) console.log(`REPORT note ${n}`);
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT problems ${JSON.stringify(problems)}`);
  writeFileSync(resolve(OUT, 'totals.json'), JSON.stringify({ shots, notes, problems }, null, 1));
  if (problems.length) throw new Error(`problems: ${problems.join(' ; ')}`);
});
