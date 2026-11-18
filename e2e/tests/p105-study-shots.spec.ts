import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { over, parseColor, ratio } from '../support/contrast';
import { apiPatch, expect, test } from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-477; D-1010 (wave 105, solo: study): a dialog the Study opens (the viewer, the edit dialog) draws the theme of
 * the page behind it. The Study page sits in `LoungeThemeProvider` (`study/routes.tsx`, section "lounge"), so it follows
 * the member's MAIN theme, not `sectionThemes.library`; the dialog portals into that same provider root. Per config
 * and dialog this records: `data-theme` of the first themed element of the page and of the dialog's nearest themed
 * ancestor, the panel's background, the title's colour, the fields' ground, the contrast of the title on the panel, and
 * the boxes of the dialog, its title, fields, labels and buttons. A fault is page and dialog on different themes.
 * Configs: 1024 shipped (main lounge-dark, sections shipped), 1024 light (main lounge-light, library lounge-light), 375
 * touch light, 375 touch shipped, and 1024 "library-only light" (main lounge-dark, library lounge-light: the page stays
 * dark, so the dialog must too). The storefront switches are put on for the run and off again. Faked in the browser:
 * the Study's shelf and files. Nothing is saved.
 * Phases: SR_P105_STUDY_PHASE=base | after; figures-<phase>.json and shots (base shots with the `base-` prefix) go to
 * SR_P105_STUDY_OUT; in the after phase, when figures-base.json is there, it compares (boxes within 0.5 px; shipped
 * colours equal) and lists problems.
 * The commander's command: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_STUDY_ON=1 --env SR_P105_STUDY_PHASE=after --env SR_P105_STUDY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-study -- tests/p105-study-shots.spec.ts
 */
const ON = process.env.SR_P105_STUDY_ON === '1';
const PHASE = process.env.SR_P105_STUDY_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P105_STUDY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-study';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

const SHIPPED = { cinema: 'sideroom', library: 'sideroom', 'live-tv': 'sideroom-broadcast' };
const LIGHT = { cinema: 'lounge-light', library: 'lounge-light', 'live-tv': 'lounge-light' };
interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  main: string;
  sections: Record<string, string>;
  shots: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-shipped',
    width: 1024,
    height: 768,
    touch: false,
    main: 'lounge-dark',
    sections: SHIPPED,
    shots: true,
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    main: 'lounge-light',
    sections: LIGHT,
    shots: true,
  },
  {
    tag: '375-light',
    width: 375,
    height: 812,
    touch: true,
    main: 'lounge-light',
    sections: LIGHT,
    shots: true,
  },
  {
    tag: '375-shipped',
    width: 375,
    height: 812,
    touch: true,
    main: 'lounge-dark',
    sections: SHIPPED,
    shots: false,
  },
  {
    tag: '1024-libonly',
    width: 1024,
    height: 768,
    touch: false,
    main: 'lounge-dark',
    sections: LIGHT,
    shots: true,
  },
];

const FILES = [
  { key: 'Films/heat.mp4', size: 1000 },
  { key: 'Music/blue.mp3', size: 2000 },
  { key: 'Covers/one.png', size: 3000 },
  { key: 'Covers/two.png', size: 4000 },
];
const json = (r: Route, body: unknown) =>
  r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
async function fakeStudy(page: Page) {
  const API = '**/api/v1';
  await page.route(`${API}/shelves/visible`, (r) =>
    json(r, { shelves: [{ shelf_id: '9501', label: 'p105s Home', via: 'own', object_count: 4 }] }),
  );
  await page.route(`${API}/storage/mounts/*/objects*`, (r) =>
    json(r, {
      folders: [],
      objects: FILES.map((f) => ({ ...f, last_modified: '2026-10-01T00:00:00Z' })),
      next_cursor: null,
      can_upload: false,
      can_manage: true,
    }),
  );
  await page.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) =>
    json(r, {
      objects: FILES.map((f) => ({
        object_hash: 'a'.repeat(64),
        path: f.key,
        size: f.size,
        class: 'other',
        title: '',
        cover: '',
        note: '',
        updated_at: '2026-10-01T00:00:00Z',
      })),
      next_cursor: null,
      can_edit: true,
    }),
  );
  await page.route('https://bucket.invalid/**', (r) => r.fulfill({ status: 404 }));
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Figures {
  pageTheme: string | null;
  dialogTheme: string | null;
  panelBg: string;
  panelOpaqueBg: string;
  titleColor: string;
  titleOnPanel: number;
  fieldGrounds: Record<string, string>;
  boxes: Record<string, Box | null>;
  scroll: [number, number];
  focusInside: boolean;
  escapeCloses: boolean;
}

const box = async (l: Locator): Promise<Box | null> => {
  const b = await l
    .first()
    .boundingBox()
    .catch(() => null);
  return b ? { x: b.x, y: b.y, w: b.width, h: b.height } : null;
};

async function measure(
  page: Page,
  dlg: Locator,
  fields: Record<string, Locator>,
): Promise<Figures> {
  const heading = dlg.getByRole('heading').first();
  const dom = await dlg.evaluate((el) => {
    const cs = getComputedStyle(el);
    const themed = el.closest('[data-theme]');
    const first = document.querySelector('[data-theme]');
    let bg = cs.backgroundColor;
    let n: Element | null = el;
    while (n && /rgba\(.*, 0\)$|transparent/.test(bg)) {
      n = n.parentElement;
      bg = n ? getComputedStyle(n).backgroundColor : 'rgb(0, 0, 0)';
    }
    return {
      panelBg: cs.backgroundColor,
      opaque: bg,
      dialogTheme: themed?.getAttribute('data-theme') ?? null,
      pageTheme: first?.getAttribute('data-theme') ?? null,
    };
  });
  const titleColor = await heading.evaluate((e) => getComputedStyle(e).color);
  const panel = parseColor(dom.opaque);
  const titleOn = ratio(over(parseColor(titleColor), panel), panel);
  const grounds: Record<string, string> = {};
  for (const [k, l] of Object.entries(fields))
    grounds[k] = await l
      .first()
      .evaluate((e) => getComputedStyle(e).backgroundColor)
      .catch(() => 'missing');
  const boxes: Record<string, Box | null> = { dialog: await box(dlg), title: await box(heading) };
  for (const [k, l] of Object.entries(fields)) boxes[k] = await box(l);
  const buttons = dlg.getByRole('button');
  const n = await buttons.count();
  for (let i = 0; i < n; i++) boxes[`button${i}`] = await box(buttons.nth(i));
  const scroll = await page.evaluate(
    () =>
      [document.documentElement.scrollWidth, document.documentElement.clientWidth] as [
        number,
        number,
      ],
  );
  const focusInside = await dlg.evaluate((el) => el.contains(document.activeElement));
  return {
    pageTheme: dom.pageTheme,
    dialogTheme: dom.dialogTheme,
    panelBg: dom.panelBg,
    panelOpaqueBg: dom.opaque,
    titleColor,
    titleOnPanel: Math.round(titleOn * 100) / 100,
    fieldGrounds: grounds,
    boxes,
    scroll,
    focusInside,
    escapeCloses: false,
  };
}

test(`p105 study: the dialogs follow the page's theme, ${PHASE}`, async ({ browser }) => {
  test.skip(!ON, 'set SR_P105_STUDY_ON=1');
  test.setTimeout(8 * 60_000);
  mkdirSync(OUT, { recursive: true });
  const owner: BrowserContext = await browser.newContext({
    baseURL: ORIGIN,
    storageState: OWNER_STATE,
  });
  const problems: string[] = [];
  const figs: Record<string, Figures> = {};
  const pre = PHASE === 'base' ? 'base-' : '';
  try {
    await setStudySurfacesWith(owner.request, true).catch(() => undefined);
    for (const c of CONFIGS) {
      await apiPatch(owner.request, '/users/@me/settings', {
        settings: { user_prefs: { theme: c.main, zoom: 100, sectionThemes: c.sections } },
      });
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
        { theme: c.main, zoom: 100, sectionThemes: c.sections },
      );
      const page = await ctx.newPage();
      page.setDefaultTimeout(15_000);
      try {
        await fakeStudy(page);
        await page.goto('/study', { waitUntil: 'domcontentloaded', timeout: 30_000 });
        const edit = page.getByRole('button', { name: /^Edit details of / }).first();
        await edit.waitFor({ timeout: 40_000 });
        await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
        await page.waitForTimeout(700);
        const pageTheme0 = await page.evaluate(
          () => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
        );

        await edit.click();
        const dlg = page.getByRole('dialog', { name: 'Edit details' }).last();
        await dlg.getByRole('combobox', { name: 'Cover' }).waitFor({ timeout: 25_000 });
        await page.waitForTimeout(700);
        const fields = {
          titleField: dlg.getByRole('textbox', { name: 'Title' }),
          noteField: dlg.getByRole('textbox', { name: 'Note' }),
          cover: dlg.getByRole('combobox', { name: 'Cover' }),
          labelTitle: dlg.getByText('Title', { exact: true }),
          labelNote: dlg.getByText('Note', { exact: true }),
          labelCover: dlg.getByText('Cover', { exact: true }),
        };
        const f = await measure(page, dlg, fields);
        f.pageTheme = pageTheme0;
        if (c.shots) await page.screenshot({ path: resolve(OUT, `${pre}study-edit-${c.tag}.png`) });
        await page.keyboard.press('Escape');
        f.escapeCloses = await dlg
          .waitFor({ state: 'hidden', timeout: 3000 })
          .then(() => true)
          .catch(() => false);
        figs[`edit/${c.tag}`] = f;

        await page
          .getByRole('button', { name: /heat/i })
          .first()
          .click({ timeout: 8000 })
          .catch(() => undefined);
        const view = page.getByRole('dialog').last();
        await view.waitFor({ timeout: 15_000 });
        await page.waitForTimeout(900);
        const v = await measure(page, view, {});
        v.pageTheme = pageTheme0;
        if (c.shots)
          await page.screenshot({ path: resolve(OUT, `${pre}study-viewer-${c.tag}.png`) });
        await page.keyboard.press('Escape');
        v.escapeCloses = await view
          .waitFor({ state: 'hidden', timeout: 3000 })
          .then(() => true)
          .catch(() => false);
        figs[`viewer/${c.tag}`] = v;
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).split('\n')[0]!.slice(0, 400)}`);
      } finally {
        await ctx.close();
      }
    }
  } finally {
    await setStudySurfacesWith(owner.request, false).catch(() => undefined);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  for (const [k, f] of Object.entries(figs)) {
    if (f.pageTheme !== f.dialogTheme)
      problems.push(`${k}: page theme ${f.pageTheme}, dialog theme ${f.dialogTheme}`);
    if (f.titleOnPanel < 4.5) problems.push(`${k}: title on panel ${f.titleOnPanel}:1`);
    if (!f.focusInside) problems.push(`${k}: focus not inside the dialog`);
    if (!f.escapeCloses) problems.push(`${k}: Escape did not close it`);
    if (f.scroll[0] > f.scroll[1]) problems.push(`${k}: page scrolls sideways ${f.scroll}`);
  }
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    for (const [k, f] of Object.entries(figs)) {
      const b = base[k];
      if (!b) {
        problems.push(`${k}: reached in the after phase only`);
        continue;
      }
      for (const [n, bx] of Object.entries(f.boxes)) {
        const bb = b.boxes[n];
        if (!bx || !bb) {
          if (!!bx !== !!bb) problems.push(`${k}: ${n} present in one phase only`);
          continue;
        }
        for (const d of ['x', 'y', 'w', 'h'] as const)
          if (Math.abs(bx[d] - bb[d]) > 0.5) problems.push(`${k}: ${n}.${d} ${bb[d]} -> ${bx[d]}`);
      }
      if (
        /shipped|libonly/.test(k) &&
        (f.panelOpaqueBg !== b.panelOpaqueBg || f.titleColor !== b.titleColor)
      )
        problems.push(`${k}: colours changed under a theme that must not change`);
    }
  }
  console.log(`problems: ${JSON.stringify(problems)}`);
  await owner.close();
  expect(problems).toEqual([]);
});
