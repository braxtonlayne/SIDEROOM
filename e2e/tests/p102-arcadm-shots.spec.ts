import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { seedActivityTier } from '../support/p100-seed';
import { apiDelete } from '../support/world';

/**
 * Wave 102, arcadm (SEC-AUD-449 / D-963, then SEC-AUD-453 / D-958 for the Arcade's three raw selects).
 *
 * ITEM 1. Admin > Arcade at 375 (touch) and 1024: the boxes of every button of two Coming soon entries (Feature on
 * the candidates', Release and Remove on the entries) and of the "Released as" field, the entry blocks' heights,
 * `hitTestAll` over the section with the takers named, sideways scroll. Two entries are seeded through the Operator's
 * route (`seedActivityTier`, own names `p100-seed-<random>`, D-942) and removed again in a `finally`; Release and
 * Remove are never clicked.
 *
 * ITEM 2. The three selects: `ArcadeSections.tsx` (the cartridge dialog's Kind; the server picker on the page, which
 * shows from two servers) and `StartInDialog.tsx` (the server picker of the Start dialog, also from two servers). The
 * spec makes a second server of its own (`p102-arcadm-<random>`, removed in the `finally`) so both pickers show. Boxes
 * of the select, its label and its neighbour, the computed text-transform/font-size/padding, the container's hit
 * test, sideways scroll, one keyboard run (focus, ArrowDown changes the value, ArrowUp puts it back).
 *
 * One run measures one PHASE: `SR_P102_ARCADM_PHASE=base` (`arcade.css`, `ArcadeSections.tsx` and `StartInDialog.tsx`
 * put back to the base's text from copies, never a stash; prefix `base-` on the shots) or `after` (default). Every
 * measure is one JSON line in `measure.json`. A hit-test row `not-reached` is a spec error and throws.
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`.
 * The account's settings are put back at the end (`sectionThemes.arcade = 'lounge-editorial'` set explicitly).
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8784 /
 * 5784 / 9484, database sr_p102_arcadm_ui):
 *   SR_P102_ARCADM_ON=1 SR_E2E_BASE_URL=http://localhost:5784 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P102_ARCADM_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-arcadm \
 *   npx playwright test tests/p102-arcadm-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The stack: sr-server with SR_DATABASE__URL=<db> SR_DATABASE__AUTO_MIGRATE=true SR_SERVER__BIND=127.0.0.1:8784
 * SR_SERVER__PUBLIC_URL=http://localhost:5784 SR_STORAGE__ENDPOINT=memory: SR_ACTIVITIES__ENABLED=true
 * SR_SERVER__METRICS_BIND=127.0.0.1:9484 SR_ADMIN_REQUIRE_REAUTH=0; Vite with SR_DEV_API_PORT=8784 VITE_SR_E2E=1
 * VITE_ACTIVITIES_ENABLED=true on 5784.
 * Variables: SR_P102_ARCADM_ON, SR_P102_ARCADM_OUT, SR_P102_ARCADM_PHASE (base | after).
 */
const ON = process.env.SR_P102_ARCADM_ON === '1';
const OUT =
  process.env.SR_P102_ARCADM_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-arcadm';
const PHASE = process.env.SR_P102_ARCADM_PHASE === 'base' ? 'base' : 'after';
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
  main: string;
  arcade: string;
  /** Shots (file names without the phase prefix); `base` ones are also kept in the base run. */
  shot: string | null;
}
const SCENARIOS: Scenario[] = [
  {
    key: 'admin-375-lounge-light',
    page: 'admin',
    w: 375,
    h: 812,
    phone: true,
    main: 'lounge-light',
    arcade: SHIPPED,
    shot: 'arcadm-375-lounge-light.png',
  },
  {
    key: 'admin-375-lounge-dark',
    page: 'admin',
    w: 375,
    h: 812,
    phone: true,
    main: 'lounge-dark',
    arcade: SHIPPED,
    shot: 'arcadm-375-lounge-dark.png',
  },
  {
    key: 'admin-1024-lounge-light',
    page: 'admin',
    w: 1024,
    h: 768,
    phone: false,
    main: 'lounge-light',
    arcade: SHIPPED,
    shot: 'arcadm-1024-lounge-light.png',
  },
  ...(['carts', 'start'] as const).flatMap((page): Scenario[] => [
    {
      key: `${page}-1024-editorial`,
      page,
      w: 1024,
      h: 768,
      phone: false,
      main: 'lounge-light',
      arcade: SHIPPED,
      shot: `${page}-1024-editorial.png`,
    },
    {
      key: `${page}-375-editorial`,
      page,
      w: 375,
      h: 812,
      phone: true,
      main: 'lounge-light',
      arcade: SHIPPED,
      shot: `${page}-375-editorial.png`,
    },
    {
      key: `${page}-1024-light`,
      page,
      w: 1024,
      h: 768,
      phone: false,
      main: 'lounge-light',
      arcade: 'lounge-light',
      shot: `${page}-1024-light.png`,
    },
  ]),
];

const r1 = (n: number) => Math.round(n * 10) / 10;
type Box = { x: number; y: number; w: number; h: number };
const boxOf = async (l: Locator): Promise<Box | null> => {
  const b = await l.boundingBox();
  return b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null;
};

test('SEC-AUD-449 and 453: Admin > Arcade buttons on a phone, the Arcade selects as the kit field', async ({
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
  const orig = await seedActivityTier({ owner: o }, 'TIER_1_ORIGINAL');
  const reimpl = await seedActivityTier({ owner: o }, 'TIER_2_REIMPLEMENTATION');
  const seeded = [orig, reimpl];
  const second = await apiPost<{ id: string }>(o, '/guilds', {
    name: `p102-arcadm-${Math.random().toString(36).slice(2, 8)}`,
  });
  console.log(`REPORT phase ${PHASE}; seeded ${orig.id}, ${reimpl.id}; second server ${second.id}`);

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
  const shoot = async (page: Page, file: string | null, baseToo: boolean) => {
    if (!file || (PHASE === 'base' && !baseToo)) return;
    const name = `${PHASE === 'base' ? 'base-' : ''}${file}`;
    await page.screenshot({ path: resolve(OUT, name) });
    shots.push(name);
  };
  /** The select's measures: box, label, computed look, text width against its box, and one keyboard run. */
  const selectFacts = async (sel: Locator, keyboard: boolean) => {
    const box = await boxOf(sel);
    const label = sel.locator('xpath=ancestor::label[1]/span[1]');
    const look = await sel.evaluate((el) => {
      const s = getComputedStyle(el);
      const wrap = el.parentElement ? getComputedStyle(el.parentElement) : null;
      return {
        tag: el.tagName,
        wrapper: el.parentElement?.tagName ?? null,
        className: el.className,
        textTransform: s.textTransform,
        fontSize: s.fontSize,
        paddingInline: `${s.paddingLeft} ${s.paddingRight}`,
        wrapFontSize: wrap?.fontSize ?? null,
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

  try {
    for (const sc of SCENARIOS) {
      const prefs = {
        theme: sc.main,
        zoom: 100,
        sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: sc.arcade },
      };
      await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
      const back = await readPrefs();
      expect(back.theme, 'account theme read back').toBe(sc.main);
      expect(
        (back.sectionThemes as { arcade?: string } | undefined)?.arcade,
        'arcade section theme read back',
      ).toBe(sc.arcade);

      const ctx = await browser.newContext({
        baseURL: ORIGIN,
        viewport: { width: sc.w, height: sc.h },
        storageState: await ownerCtx.storageState(),
        bypassCSP: true,
        ...(sc.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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

      if (sc.page === 'admin') {
        await page.goto('/admin/arcade', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(page, 5000);
        const admin = page.locator('.arc-admin');
        await admin.waitFor({ timeout: 10_000 });
        const theme = await page.evaluate(
          () =>
            document.querySelector('.ss-root[data-theme]')?.getAttribute('data-theme') ?? 'none',
        );
        expect(theme, 'data-theme read back from the Admin root').toBe(sc.main);
        const items = seeded.map((s) =>
          admin.locator('li', { has: page.locator('strong', { hasText: s.title }) }),
        );
        for (const it of items) await expect(it).toHaveCount(1);
        const facts = [];
        for (const [i, it] of items.entries()) {
          await it.scrollIntoViewIfNeeded();
          facts.push({
            title: seeded[i]!.title,
            li: await boxOf(it),
            form: await boxOf(it.locator('form.arc-release')),
            field: await boxOf(it.locator('.arc-field')),
            buttons: await Promise.all(
              (await it.getByRole('button').all()).map(async (b) => ({
                name: ((await b.textContent()) ?? '').trim().slice(0, 24),
                box: await boxOf(b),
              })),
            ),
          });
        }
        // Every other button of the list (the candidates' Feature buttons), by position in the section.
        const listButtons = await admin.locator('.arc-admin-list button').count();
        const sw = await sideways(page);
        await items[0]!.scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
        await shoot(page, sc.shot, sc.key !== 'admin-375-lounge-dark');
        const hit = await hits(page, admin);
        log({ scenario: sc.key, theme, facts, listButtons, sideways: sw, hit });
        const ownFails = hit.filter((h) => h.status === 'fail');
        console.log(
          `REPORT ${sc.key}: sideways ${sw.scrollWidth}/${sw.clientWidth}; list buttons ${listButtons}; entries ${JSON.stringify(facts.map((f) => [f.li, f.form, f.field, f.buttons]))}; hit ${hit.length} controls, fails ${JSON.stringify(ownFails)}`,
        );
      } else if (sc.page === 'carts') {
        await page.goto('/games', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await page.locator('.arc-root').waitFor({ timeout: 30_000 });
        await idle(page, 4000);
        const theme = await page
          .locator('[data-theme]:has(.arc-root)')
          .last()
          .getAttribute('data-theme');
        expect(theme, 'data-theme read back from the Arcade root').toBe(sc.arcade);
        const section = page.locator('#cartridges');
        await section.waitFor({ timeout: 15_000 });
        await section.scrollIntoViewIfNeeded();
        const pick = section.locator('.arc-server-pick select');
        await expect(pick).toHaveCount(1);
        await pick.scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
        const pickFacts = await selectFacts(pick, true);
        const sw1 = await sideways(page);
        await shoot(page, `carts-page-${sc.shot}`, true);
        const hit1 = await hits(page, section);
        await section.getByRole('button', { name: 'List a cartridge' }).first().click();
        const dialog = page.getByRole('dialog');
        await dialog.waitFor({ timeout: 10_000 });
        await page.waitForTimeout(400);
        const kind = dialog.locator('label.arc-field select');
        await expect(kind).toHaveCount(1);
        const kindFacts = await selectFacts(kind, true);
        const sw2 = await sideways(page);
        await shoot(page, `carts-dialog-${sc.shot}`, true);
        const hit2 = await hits(page, dialog);
        log({
          scenario: sc.key,
          theme,
          serverPick: pickFacts,
          kind: kindFacts,
          sideways: [sw1, sw2],
          hitPage: hit1,
          hitDialog: hit2,
        });
        console.log(
          `REPORT ${sc.key}: server pick ${JSON.stringify(pickFacts)}; kind ${JSON.stringify(kindFacts)}; sideways ${JSON.stringify([sw1, sw2])}; page hit ${hit1.length} fails ${JSON.stringify(hit1.filter((h) => h.status === 'fail'))}; dialog hit ${hit2.length} fails ${JSON.stringify(hit2.filter((h) => h.status === 'fail'))}`,
        );
      } else {
        await page.goto('/games', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await page.locator('.arc-root').waitFor({ timeout: 30_000 });
        await idle(page, 4000);
        const theme = await page
          .locator('[data-theme]:has(.arc-root)')
          .last()
          .getAttribute('data-theme');
        expect(theme, 'data-theme read back from the Arcade root').toBe(sc.arcade);
        const play = page.locator('.arc-actions button').first();
        await play.scrollIntoViewIfNeeded();
        await play.click();
        const dialog = page.getByRole('dialog');
        await dialog.waitFor({ timeout: 10_000 });
        const pick = dialog.locator('label.arc-field select');
        await expect(pick).toHaveCount(1);
        await idle(page, 3000);
        const facts = await selectFacts(pick, true);
        const sw = await sideways(page);
        await shoot(page, sc.shot, true);
        const hit = await hits(page, dialog);
        log({ scenario: sc.key, theme, pick: facts, sideways: sw, hit });
        console.log(
          `REPORT ${sc.key}: start pick ${JSON.stringify(facts)}; sideways ${sw.scrollWidth}/${sw.clientWidth}; hit ${hit.length} fails ${JSON.stringify(hit.filter((h) => h.status === 'fail'))}`,
        );
      }
      await ctx.close();
    }
  } finally {
    for (const s of seeded)
      await s
        .remove()
        .catch((e) => notes.push(`could not remove ${s.id}: ${String(e).slice(0, 80)}`));
    await apiDelete(o, `guilds/${second.id}`).catch((e) =>
      notes.push(`could not remove server ${second.id}: ${String(e).slice(0, 80)}`),
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
