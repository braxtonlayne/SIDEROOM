import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import type { Page } from '@playwright/test';
import { test, expect } from '../support/fixtures';
import { soloServer, buildWorld, type World } from '../support/world';

/**
 * RDY-I18N-002 (c): the core routes in the `en-XA` pseudo-locale (accented, about 35 percent longer, bracketed;
 * apps/web/src/i18n/runtime.ts, scripts/pseudo-locale.mjs) at a desktop and a phone width, on Chromium.
 * `?lang=en-XA` works on the dev server this suite runs against, and under `vite preview` for a build made with
 * SR_PSEUDO_LOCALE=1; a production build ignores it.
 *
 * Two assertions per route and width:
 *  1. the page does not scroll sideways (`documentElement.scrollWidth <= clientWidth`);
 *  2. no text is cut off inside a control: an element that is a control (button, link, tab, menu item, badge in
 *     a control, ...) or sits inside one, clips its overflow (`overflow-x: hidden|clip`), has no `text-overflow:
 *     ellipsis` (an ellipsis is a designed truncation, and the full text is in the title or the dialog), and has
 *     `scrollWidth > clientWidth`.
 *
 * Findings that are real but owned by a screen outside this task are listed in KNOWN with the file's owner, so
 * the spec stays green and the list is the work queue (docs/dev/handoff/feat-p26-i18n.md). A KNOWN entry that
 * stops matching fails the spec, so the list cannot go stale.
 */
test.use({ bypassCSP: true });
test.skip(
  ({ browserName }) => browserName !== 'chromium',
  'the pseudo-locale pass runs on Chromium',
);

const OUT = '.state/i18n-pseudo';
const WORLD_FILE = `${OUT}/world.json`;
const OWNER_STATE = `${OUT}/owner.json`;
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
const slug = (n: string) => n.replace(/[^a-z0-9]+/gi, '-').toLowerCase();

const WIDTHS = [
  { name: 'desktop', width: 1280, height: 800 },
  { name: 'phone', width: 375, height: 812 },
] as const;

interface Row {
  name: string;
  path: (w: World) => string;
  /** Needs the signed-in Owner; the entrance rows run without a session. */
  auth: boolean;
}

const ROWS: Row[] = [
  { name: 'entrance landing', path: () => '/', auth: false },
  { name: 'entrance login', path: () => '/login', auth: false },
  { name: 'entrance register', path: () => '/register', auth: false },
  {
    name: 'lounge channel',
    path: (w) => `/channels/${w.guildId}/${w.channelId}`,
    auth: true,
  },
  { name: 'settings account', path: () => '/settings/user/account', auth: true },
  { name: 'settings language', path: () => '/settings/user/language', auth: true },
  { name: 'library home', path: () => '/library', auth: true },
  { name: 'cinema title', path: (w) => `/cinema/t/${w.titleId}`, auth: true },
  { name: 'admin home', path: () => '/admin', auth: true },
];

export interface Clip {
  /** A short, stable description: tag, role, class and the start of the text. */
  where: string;
  scrollWidth: number;
  clientWidth: number;
}

/** Runs in the page. Kept free of closures over test code. */
async function scan(page: Page) {
  return page.evaluate(() => {
    const CONTROL =
      'button, a[href], summary, label, select, [role="button"], [role="tab"], [role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"], [role="option"], [role="switch"], [role="checkbox"], [role="radio"], [role="link"], [role="treeitem"], [data-testid*="chip"], [class*="chip"], [class*="badge"], [class*="pill"]';
    const doc = document.documentElement;
    const clips: { where: string; scrollWidth: number; clientWidth: number }[] = [];
    for (const el of Array.from(document.body.querySelectorAll<HTMLElement>('*'))) {
      if (el.matches('input, textarea, select, option, script, style, svg, svg *')) continue;
      if (!(el.matches(CONTROL) || el.parentElement?.closest(CONTROL))) continue;
      const cs = getComputedStyle(el);
      if (cs.display === 'none' || cs.visibility === 'hidden') continue;
      if (cs.overflowX !== 'hidden' && cs.overflowX !== 'clip') continue;
      if (cs.textOverflow === 'ellipsis') continue;
      const r = el.getBoundingClientRect();
      // Visually hidden helpers (1px boxes with clipped text) are meant to be clipped.
      if (r.width <= 2 || r.height <= 2) continue;
      if (!el.textContent?.trim()) continue;
      if (el.scrollWidth - el.clientWidth <= 1) continue;
      const cls = typeof el.className === 'string' ? el.className.trim().split(/\s+/)[0] : '';
      const text = (el.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40);
      clips.push({
        where: `${el.tagName.toLowerCase()}${el.getAttribute('role') ? `[role=${el.getAttribute('role')}]` : ''}${cls ? `.${cls}` : ''} "${text}"`,
        scrollWidth: el.scrollWidth,
        clientWidth: el.clientWidth,
      });
    }
    // The widest elements that stick out of the viewport and are not inside a clipping or scrolling box.
    const wide: string[] = [];
    if (doc.scrollWidth - doc.clientWidth > 1) {
      for (const el of Array.from(document.body.querySelectorAll<HTMLElement>('*'))) {
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.right <= doc.clientWidth + 1) continue;
        let clipped = false;
        for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
          const ox = getComputedStyle(p).overflowX;
          if (ox !== 'visible') clipped = true;
          if (getComputedStyle(p).position === 'fixed') clipped = true;
        }
        if (getComputedStyle(el).position === 'fixed' || clipped) continue;
        const cls = typeof el.className === 'string' ? el.className.trim().split(/\s+/)[0] : '';
        wide.push(`${el.tagName.toLowerCase()}${cls ? `.${cls}` : ''} to ${Math.round(r.right)}px`);
      }
    }
    return {
      pageOverflow: doc.scrollWidth - doc.clientWidth,
      wide: wide.slice(0, 4),
      pseudoActive: /[àƀçðéƒĝĥîĵķļɱñöþǫŕšţûṽŵẋýž]/.test(document.body.innerText),
      clips,
    };
  });
}

/**
 * Real findings that belong to a screen this task does not own. `where` is matched as a substring of
 * `<route> @ <width>: <description>`; `owner` is the file that has to change.
 */
const KNOWN: { match: string; owner: string; note: string }[] = [];

test('the scanner finds a control that clips its text', async ({ page }) => {
  await page.setContent(
    '<button style="width:60px;overflow:hidden;white-space:nowrap">A long pseudo label</button>' +
      '<button style="width:60px;overflow:hidden;white-space:nowrap;text-overflow:ellipsis">Designed truncation</button>' +
      '<div style="width:60px;overflow:hidden;white-space:nowrap">Not a control, left alone</div>',
  );
  const r = await scan(page);
  expect(r.clips.map((c) => c.where)).toEqual(['button "A long pseudo label"']);
});

test('build the fixture world', async ({ owner, signedIn }) => {
  const s = await soloServer(signedIn, owner, 'Pseudo');
  const world = await buildWorld(s);
  mkdirSync(OUT, { recursive: true });
  writeFileSync(WORLD_FILE, JSON.stringify(world));
  await s.a.context.storageState({ path: OWNER_STATE });
});

const seen = new Set<string>();

for (const vp of WIDTHS) {
  test.describe(`en-XA at ${vp.name} width`, () => {
    for (const row of ROWS) {
      test.describe(row.name, () => {
        test.use({
          viewport: { width: vp.width, height: vp.height },
          storageState: row.auth ? OWNER_STATE : { cookies: [], origins: [] },
        });
        test(`${row.name}`, async ({ page }) => {
          const world = row.auth ? loadWorld() : ({} as World);
          await page.goto(`${row.path(world)}?lang=en-XA`);
          await expect(page.getByRole('main').or(page.getByRole('log')).first()).toBeVisible({
            timeout: 20_000,
          });
          await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
          await page.waitForTimeout(500);

          const r = await scan(page);
          expect(r.pseudoActive, 'the page did not render in en-XA').toBe(true);

          const label = `${row.name} @ ${vp.name}`;
          const findings: string[] = [];
          if (r.pageOverflow > 1)
            findings.push(`page scrolls sideways by ${r.pageOverflow}px (${r.wide.join(', ')})`);
          for (const c of r.clips)
            findings.push(`${c.where} clipped (${c.scrollWidth} > ${c.clientWidth})`);

          mkdirSync(OUT, { recursive: true });
          writeFileSync(
            `${OUT}/${slug(label)}.json`,
            JSON.stringify({ route: row.name, width: vp.name, findings }, null, 1),
          );

          const open = findings.filter((f) => {
            const known = KNOWN.find((k) => `${label}: ${f}`.includes(k.match));
            if (known) seen.add(known.match);
            return !known;
          });
          if (findings.length > open.length) {
            test.info().annotations.push({
              type: 'known',
              description: `${findings.length - open.length} listed in KNOWN`,
            });
          }
          expect(open, label).toEqual([]);
        });
      });
    }
  });
}

test.afterAll(() => {
  // Only meaningful when the whole file ran in this worker; a partial run (--grep) skips the check.
  if (seen.size === 0) return;
  const stale = KNOWN.filter((k) => !seen.has(k.match)).map((k) => k.match);
  expect(stale, 'KNOWN entries that no longer match anything').toEqual([]);
});
