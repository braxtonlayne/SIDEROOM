import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { seedActivityTier } from '../support/p100-seed';
import { apiDelete } from '../support/world';

/**
 * Wave 103, arcade.
 *
 * SEC-AUD-456 / D-981: the Arcade Start dialog's channel rows are 44 px tall at phone width (nothing changes above it).
 * The spec opens the Start dialog (Play on the first card) with a server of its own that has 12 channels, and a second
 * of its own with 25 (own names `p103-arcade-<random>`, removed in the `finally`, D-942), picks each by name, and
 * measures the rows' boxes, the list's scroll, `hitTestAll` over the dialog (takers named) and sideways scroll.
 *
 * SEC-AUD-464 / D-972: `arcade.css` stops styling selects by element name, and the label rule no longer matches the
 * kit select's wrapper. The spec measures the three NativeSelects (Start dialog's server picker, the cartridge
 * dialog's Kind, the page's server picker) and every `.arc-field` label and kit text-field label of the List a
 * cartridge dialog: box, computed type, padding, text clear of the chevron, one keyboard run; and Admin > Arcade's
 * kit Select labels.
 *
 * One run measures one PHASE: `SR_P103_ARCADE_PHASE=base` (`arcade.css` put back to the base's text from a copy kept
 * outside the repository, never a stash; prefix `base-` on the shots) or `after` (default). One JSON line per
 * measure in `measure.json`. A hit-test row `not-reached` is a spec error and throws.
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`.
 * Ports 8804 / 5804 / 9504, database sr_p103_arcade_ui. From `e2e/`:
 *   SR_P103_ARCADE_ON=1 SR_E2E_BASE_URL=http://localhost:5804 SR_CLI_BIN=(sr-cli) SR_DATABASE__URL=(db)
 *   DATABASE_URL=(db) SR_P103_ARCADE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-arcade
 *   npx playwright test tests/p103-arcade-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P103_ARCADE_ON === '1';
const OUT =
  process.env.SR_P103_ARCADE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-arcade';
const PHASE = process.env.SR_P103_ARCADE_PHASE === 'base' ? 'base' : 'after';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SHIPPED = 'lounge-editorial';

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

interface Scenario {
  key: string;
  page: 'admin' | 'carts' | 'start';
  w: number;
  h: number;
  phone: boolean;
  arcade: string;
  /** Which of the spec's own servers the Start dialog shows. */
  big?: 'mid' | 'large';
  shot: string | null;
}
const sc = (
  key: string,
  page: Scenario['page'],
  w: number,
  h: number,
  arcade: string,
  shot: string | null,
  big?: Scenario['big'],
): Scenario => ({ key, page, w, h, phone: w < 500, arcade, shot, big });
const SCENARIOS: Scenario[] = [
  sc('start-375', 'start', 375, 812, SHIPPED, 'start-375.png', 'mid'),
  sc('start-375-light', 'start', 375, 812, 'lounge-light', 'start-375-light.png', 'mid'),
  sc('start-375-large', 'start', 375, 812, SHIPPED, 'start-375-large.png', 'large'),
  sc('start-1024', 'start', 1024, 768, SHIPPED, 'start-1024.png', 'mid'),
  sc('start-1024-light', 'start', 1024, 768, 'lounge-light', null, 'mid'),
  sc('carts-1024', 'carts', 1024, 768, SHIPPED, 'cartridge-form-1024.png'),
  sc('carts-375', 'carts', 375, 812, SHIPPED, 'cartridge-form-375.png'),
  sc('carts-1024-light', 'carts', 1024, 768, 'lounge-light', null),
  sc('admin-1024', 'admin', 1024, 768, SHIPPED, 'admin-arcade-1024.png'),
  sc('admin-375', 'admin', 375, 812, SHIPPED, null),
];

const r1 = (n: number) => Math.round(n * 10) / 10;
type Box = { x: number; y: number; w: number; h: number };
const boxOf = async (l: Locator): Promise<Box | null> => {
  const b = await l.boundingBox();
  return b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null;
};

test('SEC-AUD-456 and 464: Start dialog rows on a phone, arcade.css without a select rule', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const log = (rec: Record<string, unknown>) =>
    appendFileSync(jsonl, `${JSON.stringify({ phase: PHASE, ...rec })}\n`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const notes: string[] = [];
  const tag = Math.random().toString(36).slice(2, 8);
  const made: string[] = [];
  const mkGuild = async (suffix: string, n: number) => {
    const name = `p103-arcade-${suffix}-${tag}`;
    let g: { id: string } | undefined;
    for (let attempt = 0; !g; attempt++) {
      try {
        g = await apiPost<{ id: string }>(o, '/guilds', { name });
      } catch (e) {
        // A 429 is the stack's own limit on server creation, not a page fault: wait it out (three tries).
        if (!String(e).includes('429') || attempt >= 3) throw e;
        await new Promise((r) => setTimeout(r, 30_000));
      }
    }
    made.push(g.id);
    for (let i = 0; i < n; i++)
      await apiPost(o, `/guilds/${g.id}/channels`, {
        name: `p103-ch-${suffix}-${i + 1}`,
        type: i % 4 === 3 ? 2 : 0,
      });
    return { id: g.id, name };
  };
  const mid = await mkGuild('mid', 12);
  const large = await mkGuild('large', 25);
  const soon = await seedActivityTier({ owner: o }, 'TIER_1_ORIGINAL');
  console.log(`REPORT phase ${PHASE}; guilds ${mid.id} ${large.id}; soon entry ${soon.id}`);

  const readPrefs = async () => {
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: Record<string, unknown> & { theme?: string } };
    };
    return back.settings?.user_prefs ?? {};
  };
  const original = await readPrefs();
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };
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
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map(
          (p) =>
            `${p.at}: ${p.taker?.tag ?? 'none'} "${p.taker?.name ?? ''}" (${p.taker?.path ?? ''})`,
        ),
    }));
  };
  const sideways = (page: Page) =>
    page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }));
  const shoot = async (page: Page, file: string | null) => {
    if (!file) return;
    const name = `${PHASE === 'base' ? 'base-' : ''}${file}`;
    await page.screenshot({ path: resolve(OUT, name) });
    shots.push(name);
  };
  const selectFacts = async (sel: Locator, keyboard: boolean) => {
    const box = await boxOf(sel);
    const label = sel.locator('xpath=ancestor::label[1]/span[1]');
    const look = await sel.evaluate((el) => {
      const s = getComputedStyle(el);
      const wrap = el.parentElement ? getComputedStyle(el.parentElement) : null;
      return {
        wrapper: el.parentElement?.tagName ?? null,
        textTransform: s.textTransform,
        fontSize: s.fontSize,
        fontWeight: s.fontWeight,
        paddingInline: `${s.paddingLeft} ${s.paddingRight}`,
        paddingBlock: `${s.paddingTop} ${s.paddingBottom}`,
        border: s.borderTopWidth,
        radius: s.borderTopLeftRadius,
        wrapTransform: wrap?.textTransform ?? null,
        wrapFontSize: wrap?.fontSize ?? null,
        wrapLetter: wrap?.letterSpacing ?? null,
        scrollW: el.scrollWidth,
        clientW: el.clientWidth,
      };
    });
    let key: Record<string, unknown> | null = null;
    if (keyboard) {
      const before = await sel.inputValue();
      await sel.focus();
      await sel.page().keyboard.press('ArrowDown');
      const down = await sel.inputValue();
      await sel.page().keyboard.press('ArrowUp');
      const up = await sel.inputValue();
      key = { before, down, up, changed: down !== before, back: up === before };
    }
    return { box, label: await boxOf(label), look, key };
  };
  /** The look of every `.arc-field` span and every label (the kit text fields') inside `root`. */
  const labelLooks = (root: Locator) =>
    root.evaluate((el) => {
      const pick = (n: Element) => {
        const s = getComputedStyle(n);
        const b = n.getBoundingClientRect();
        return {
          tag: n.tagName,
          text: (n.textContent ?? '').trim().slice(0, 24),
          transform: s.textTransform,
          size: s.fontSize,
          weight: s.fontWeight,
          letter: s.letterSpacing,
          color: s.color,
          w: Math.round(b.width * 10) / 10,
          h: Math.round(b.height * 10) / 10,
        };
      };
      return Array.from(el.querySelectorAll('.arc-field > span, .arc-field > label, label')).map(
        pick,
      );
    });

  try {
    for (const s of SCENARIOS) {
      const prefs = {
        theme: 'lounge-light',
        zoom: 100,
        sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: s.arcade },
      };
      await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
      const back = await readPrefs();
      expect(
        (back.sectionThemes as { arcade?: string } | undefined)?.arcade,
        'arcade section theme read back',
      ).toBe(s.arcade);

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

      if (s.page === 'admin') {
        await page.goto('/admin/arcade', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(page, 5000);
        const admin = page.locator('.arc-admin');
        await admin.waitFor({ timeout: 10_000 });
        const item = admin.locator('li', { has: page.locator('strong', { hasText: soon.title }) });
        await expect(item).toHaveCount(1);
        await item.scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
        const field = await boxOf(item.locator('.arc-field'));
        const labels = await labelLooks(admin);
        const sw = await sideways(page);
        await shoot(page, s.shot);
        log({ scenario: s.key, field, labels, sideways: sw });
        console.log(
          `REPORT ${s.key}: field ${JSON.stringify(field)}; labels ${JSON.stringify(labels)}; sideways ${sw.scrollWidth}/${sw.clientWidth}`,
        );
      } else if (s.page === 'carts') {
        await page.goto('/games', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await page.locator('.arc-root').waitFor({ timeout: 30_000 });
        await idle(page, 4000);
        const theme = await page
          .locator('[data-theme]:has(.arc-root)')
          .last()
          .getAttribute('data-theme');
        expect(theme, 'data-theme read back from the Arcade root').toBe(s.arcade);
        const section = page.locator('#cartridges');
        await section.waitFor({ timeout: 15_000 });
        await section.scrollIntoViewIfNeeded();
        const pick = section.locator('.arc-server-pick select');
        await expect(pick).toHaveCount(1);
        await pick.scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
        const pickFacts = await selectFacts(pick, true);
        const sw1 = await sideways(page);
        await section.getByRole('button', { name: 'List a cartridge' }).first().click();
        const dialog = page.getByRole('dialog');
        await dialog.waitFor({ timeout: 10_000 });
        await page.waitForTimeout(400);
        const kind = dialog.locator('label.arc-field select');
        await expect(kind).toHaveCount(1);
        const kindFacts = await selectFacts(kind, true);
        const labels = await labelLooks(dialog);
        const sw2 = await sideways(page);
        await shoot(page, s.shot);
        const hit = await hits(page, dialog);
        log({
          scenario: s.key,
          theme,
          serverPick: pickFacts,
          kind: kindFacts,
          labels,
          sideways: [sw1, sw2],
          hit,
        });
        console.log(
          `REPORT ${s.key}: server pick ${JSON.stringify(pickFacts)}; kind ${JSON.stringify(kindFacts)}; labels ${JSON.stringify(labels)}; sideways ${JSON.stringify([sw1, sw2])}; hit ${hit.length} fails ${JSON.stringify(hit.filter((h) => h.status === 'fail'))}`,
        );
      } else {
        await page.goto('/games', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await page.locator('.arc-root').waitFor({ timeout: 30_000 });
        await idle(page, 4000);
        const theme = await page
          .locator('[data-theme]:has(.arc-root)')
          .last()
          .getAttribute('data-theme');
        expect(theme, 'data-theme read back from the Arcade root').toBe(s.arcade);
        const play = page.locator('.arc-actions button').first();
        await play.scrollIntoViewIfNeeded();
        await play.click();
        const dialog = page.getByRole('dialog');
        await dialog.waitFor({ timeout: 10_000 });
        const pick = dialog.locator('label.arc-field select');
        await expect(pick).toHaveCount(1);
        const which = s.big === 'large' ? 'large' : 'mid';
        await pick.selectOption({ label: which === 'large' ? large.name : mid.name });
        await expect(dialog.getByText(`p103-ch-${which}-1`, { exact: true }).first()).toBeVisible();
        await idle(page, 3000);
        const rows = dialog.locator('.arc-start-row');
        const n = await rows.count();
        const rowBoxes: Box[] = [];
        for (let i = 0; i < n; i++) {
          const b = await boxOf(rows.nth(i));
          if (b) rowBoxes.push(b);
        }
        const heights = [...new Set(rowBoxes.map((b) => b.h))];
        const lists = await dialog
          .locator('.arc-start-list')
          .evaluateAll((els) =>
            els.map((e) => ({ scrollH: e.scrollHeight, clientH: e.clientHeight })),
          );
        const dlgBox = await boxOf(dialog);
        const facts = await selectFacts(pick, which !== 'large');
        const sw = await sideways(page);
        await shoot(page, s.shot);
        const hit = await hits(page, dialog);
        const fails = hit.filter((h) => h.status === 'fail');
        log({
          scenario: s.key,
          theme,
          rows: n,
          heights,
          lists,
          dialog: dlgBox,
          pick: facts,
          sideways: sw,
          hit,
        });
        console.log(
          `REPORT ${s.key}: rows ${n}, heights ${JSON.stringify(heights)}; lists ${JSON.stringify(lists)}; dialog ${JSON.stringify(dlgBox)}; pick ${JSON.stringify(facts)}; sideways ${sw.scrollWidth}/${sw.clientWidth}; hit ${hit.length} controls, fails ${fails.length} ${JSON.stringify(fails)}`,
        );
      }
      await ctx.close();
    }
  } finally {
    await soon
      .remove()
      .catch((e) => notes.push(`could not remove ${soon.id}: ${String(e).slice(0, 80)}`));
    for (const id of made)
      await apiDelete(o, `guilds/${id}`).catch((e) =>
        notes.push(`could not remove server ${id}: ${String(e).slice(0, 80)}`),
      );
    try {
      await apiPatch(o, '/users/@me/settings', {
        settings: {
          user_prefs: {
            ...original,
            sectionThemes: {
              ...((original.sectionThemes as Record<string, string> | undefined) ?? {}),
              arcade: SHIPPED,
            },
          },
        },
      });
      const end = await readPrefs();
      console.log(
        `REPORT settings put back: theme ${end.theme}, arcade ${(end.sectionThemes as { arcade?: string } | undefined)?.arcade}`,
      );
    } catch (e) {
      notes.push(`could not put the settings back: ${String(e).slice(0, 100)}`);
    }
    await ownerCtx.close();
  }
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  for (const n of notes) console.log(`REPORT note ${n}`);
});
