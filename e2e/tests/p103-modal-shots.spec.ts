import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * Wave 103, modal (SEC-AUD-460 / D-979): the kit Modal's Close button is a whole 44 px target on a phone.
 *
 * Three kit modals: the Lounge's Create Poll, the Arcade's List-a-cartridge form, and a long one (Create Poll in a
 * viewport only 420 / 400 px tall, so its body is taller than the dialog and scrolls). For each, at 375 touch (Lounge
 * Light and Lounge Dark; the Arcade in its shipped theme `lounge-editorial` set explicitly and read back) and at
 * 1024 x 768 (Light): the boxes of the Close button, header, title, body and dialog, the computed overflow, position and
 * z-index of header, body and button, the overlap of the body's box with the button's, `hitTestAll` over the dialog
 * (the shared tool; the Close row is reported with its takers), and a REAL tap on the Close button's lowest row of
 * pixels (it must close the dialog; the base's answer is recorded too). With the long body the body is scrolled to its
 * end first and the Close row re-tested.
 *
 * One run measures one PHASE: `SR_P103_MODAL_PHASE=base` (`Modal.module.css` put back to the base's text from a copy,
 * never a stash; shots get the prefix `base-`) or `after` (default; compares with `figures-base.json` when the folder
 * holds one: header, title and body boxes must equal the base). Needs the visual sweep's seeded world
 * (`e2e/.state/sweep/`) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`. The account's settings are put back at the end.
 *
 * The commander's command (from `e2e/`; ports 8807 / 5807 / 9507, database sr_p103_modal_ui; `DATABASE_URL` and
 * `SR_CLI_BIN` set; the sweep world seeded on the same database):
 *   SR_P103_MODAL_ON=1 SR_E2E_BASE_URL=http://localhost:5807 \
 *   SR_P103_MODAL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-modal \
 *   npx playwright test tests/p103-modal-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Variables: SR_P103_MODAL_ON, SR_P103_MODAL_OUT, SR_P103_MODAL_PHASE (base | after).
 */
const ON = process.env.SR_P103_MODAL_ON === '1';
const PHASE = process.env.SR_P103_MODAL_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_MODAL_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-modal';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SHIPPED = 'lounge-editorial';

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

type Modal = 'poll' | 'cart' | 'long';
interface Cfg {
  tag: string;
  modal: Modal;
  w: number;
  h: number;
  phone: boolean;
  main: string;
  shot: string;
}
const cfgs: Cfg[] = [];
for (const modal of ['poll', 'cart', 'long'] as const) {
  const h375 = modal === 'long' ? 420 : 812;
  const h1024 = modal === 'long' ? 400 : 768;
  cfgs.push(
    {
      tag: `${modal}-375-lounge-light`,
      modal,
      w: 375,
      h: h375,
      phone: true,
      main: 'lounge-light',
      shot: `${modal}-375-lounge-light.png`,
    },
    {
      tag: `${modal}-375-lounge-dark`,
      modal,
      w: 375,
      h: h375,
      phone: true,
      main: 'lounge-dark',
      shot: `${modal}-375-lounge-dark.png`,
    },
    {
      tag: `${modal}-1024`,
      modal,
      w: 1024,
      h: h1024,
      phone: false,
      main: 'lounge-light',
      shot: `${modal}-1024.png`,
    },
  );
}

const r1 = (n: number) => Math.round(n * 10) / 10;
type Box = { x: number; y: number; w: number; h: number };
const boxOf = async (l: Locator): Promise<Box | null> => {
  const b = await l.boundingBox();
  return b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null;
};
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };

async function openModal(page: Page, modal: Modal): Promise<Locator> {
  if (modal === 'cart') {
    await page.goto('/games', { waitUntil: 'domcontentloaded', timeout: 20_000 });
    await page.locator('.arc-root').waitFor({ timeout: 30_000 });
    await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
    const section = page.locator('#cartridges');
    await section.waitFor({ timeout: 15_000 });
    await section.scrollIntoViewIfNeeded();
    await section.getByRole('button', { name: 'List a cartridge' }).first().click();
    const dlg = page.getByRole('dialog');
    await dlg.waitFor({ timeout: 10_000 });
    await page.waitForTimeout(500);
    return dlg;
  }
  const w = world();
  await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  await page.getByRole('button', { name: 'Add to message' }).first().waitFor({ timeout: 20_000 });
  await page.waitForTimeout(800);
  await page.getByRole('button', { name: 'Add to message' }).first().click();
  await page.getByRole('menuitem', { name: 'Create Poll' }).click();
  const dlg = page.getByRole('dialog', { name: 'Create Poll' });
  await dlg.waitFor({ timeout: 15_000 });
  await dlg.locator('select').first().waitFor({ timeout: 15_000 });
  await page.waitForTimeout(600);
  return dlg;
}

const facts = async (dlg: Locator) => {
  const close = dlg.getByRole('button', { name: /^close$/i }).first();
  const header = close.locator('xpath=..');
  const dialog = dlg;
  const title = header.locator('h2');
  const bodyLoc = dlg.locator('xpath=./*[contains(@class,"_body_")]');
  const styles = await close.evaluate((btn) => {
    const hd = btn.parentElement as HTMLElement;
    const bd = hd.parentElement!.querySelector(':scope > [class*="_body_"]') as HTMLElement | null;
    const pick = (e: Element | null) => {
      if (!e) return null;
      const s = getComputedStyle(e);
      return { overflow: s.overflowY, position: s.position, zIndex: s.zIndex, display: s.display };
    };
    return { header: pick(hd), body: pick(bd), button: pick(btn) };
  });
  const cb = await boxOf(close);
  const bb = await boxOf(bodyLoc);
  const hb = await boxOf(header);
  const overlap = cb && bb ? r1(Math.max(0, cb.y + cb.h - bb.y)) : null;
  return {
    close: cb,
    header: hb,
    title: await boxOf(title),
    body: bb,
    dialog: await boxOf(dialog),
    bodyOverlapsCloseBy: overlap,
    headerEndsAt: hb ? r1(hb.y + hb.h) : null,
    styles,
  };
};

test('SEC-AUD-460: the kit Modal Close button is a whole target on a phone', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const readPrefs = async () => {
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: Record<string, unknown> & { theme?: string } };
    };
    return back.settings?.user_prefs ?? {};
  };
  const original = await readPrefs();
  const figs: Record<string, unknown> = {};
  const problems: string[] = [];
  const shots: string[] = [];
  let baseFigs: Record<string, ReturnType<typeof JSON.parse>> | null = null;
  if (PHASE === 'after') {
    try {
      baseFigs = JSON.parse(readFileSync(resolve(OUT, 'figures-base.json'), 'utf8'));
    } catch {
      baseFigs = null;
    }
  }
  const named = (n: string) => `${PHASE === 'base' ? 'base-' : ''}${n}`;

  try {
    for (const c of cfgs) {
      // Shots: every config is shot after; the base run keeps the 375 Light and the 1024 ones.
      const prefs = {
        theme: c.main,
        zoom: 100,
        sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: SHIPPED },
      };
      await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
      expect((await readPrefs()).theme, 'account theme read back').toBe(c.main);
      const ctx = await browser.newContext({
        baseURL: ORIGIN,
        viewport: { width: c.w, height: c.h },
        storageState: await ownerCtx.storageState(),
        bypassCSP: true,
        ...(c.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
      page.setDefaultTimeout(12_000);
      try {
        const dlg = await openModal(page, c.modal);
        const themeOf = await page.evaluate(
          () => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? 'none',
        );
        const sw = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          clientWidth: document.documentElement.clientWidth,
        }));
        const f = await facts(dlg);
        const bodyScroll = await dlg.evaluate((d) => {
          const b = d.querySelector(':scope > [class*="_body_"]') as HTMLElement | null;
          const b2 = b ?? (d.children[2] as HTMLElement);
          return { scrollHeight: b2.scrollHeight, clientHeight: b2.clientHeight };
        });
        await openCollapsed(page, dlg);
        const rows = await hitTestAll(page, dlg);
        const nr = rows.filter((r) => r.status === 'not-reached');
        if (nr.length) throw new Error(`${c.tag}: ${nr.length} not-reached (probe error)`);
        const closeRow = rows.find((r) => /close/i.test(r.name));
        const closeHit = closeRow
          ? {
              status: closeRow.status,
              box: closeRow.result?.box,
              takers: (closeRow.result?.points ?? [])
                .filter((p) => !p.ok)
                .map((p) => `${p.at}: ${p.taker?.tag} .${p.taker?.path}`),
            }
          : null;
        const otherFails = rows
          .filter((r) => r.status === 'fail' && !/close/i.test(r.name))
          .map((r) => r.name);
        // The long body: scroll it to its end, then test the Close again.
        let afterScroll: unknown = null;
        if (c.modal === 'long') {
          await dlg.evaluate((d) => {
            const b = d.querySelector(':scope > [class*="_body_"]') as HTMLElement | null;
            if (b) b.scrollTop = b.scrollHeight;
          });
          await page.waitForTimeout(200);
          const rows2 = await hitTestAll(page, dlg);
          const cr = rows2.find((r) => /close/i.test(r.name));
          afterScroll = {
            status: cr?.status,
            takers: (cr?.result?.points ?? [])
              .filter((p) => !p.ok)
              .map((p) => `${p.at}: ${p.taker?.tag} .${p.taker?.path}`),
          };
          await dlg.evaluate((d) => {
            const b = d.querySelector(':scope > [class*="_body_"]') as HTMLElement | null;
            if (b) b.scrollTop = 0;
          });
          await page.waitForTimeout(200);
        }
        // Shot (viewport; no sideways scroll asserted by the figures).
        const keepInBase = c.tag.endsWith('-375-lounge-light') || c.tag.endsWith('-1024');
        if (PHASE === 'after' || keepInBase) {
          await page.screenshot({ path: resolve(OUT, named(c.shot)) });
          shots.push(named(c.shot));
        }
        // A real tap on the Close button's lowest row of pixels (phone) or its centre (1024).
        const closeBtn = dlg.getByRole('button', { name: /^close$/i }).first();
        const cb = (await closeBtn.boundingBox())!;
        const cx = cb.x + cb.width / 2;
        const cy = c.phone ? cb.y + cb.height - 1.5 : cb.y + cb.height / 2;
        if (c.phone) await page.touchscreen.tap(cx, cy);
        else await page.mouse.click(cx, cy);
        await page.waitForTimeout(500);
        const closed = (await page.getByRole('dialog').count()) === 0;
        figs[c.tag] = {
          theme: themeOf,
          sideways: sw,
          ...f,
          bodyScroll,
          closeHit,
          otherFails,
          afterScroll,
          tapLowestRow: { at: [r1(cx), r1(cy)], closed },
        };
        console.log(`REPORT ${PHASE} ${c.tag}: ${JSON.stringify(figs[c.tag])}`);
        if (PHASE === 'after') {
          if (c.phone) {
            if (closeHit?.status !== 'pass')
              problems.push(`${c.tag}: Close hit test ${JSON.stringify(closeHit)}`);
            if (!closed)
              problems.push(`${c.tag}: a tap on the lowest row did not close the dialog`);
            const as = afterScroll as { status?: string } | null;
            if (as && as.status !== 'pass')
              problems.push(`${c.tag}: Close after scrolling ${JSON.stringify(as)}`);
          }
          const b = baseFigs?.[c.tag];
          if (b) {
            for (const k of ['header', 'title', 'body', 'dialog'] as const) {
              if (JSON.stringify(b[k]) !== JSON.stringify((f as Record<string, unknown>)[k]))
                problems.push(
                  `${c.tag}: ${k} box ${JSON.stringify((f as Record<string, unknown>)[k])} differs from base ${JSON.stringify(b[k])}`,
                );
            }
            if (JSON.stringify(b.close) !== JSON.stringify(f.close))
              problems.push(
                `${c.tag}: close box ${JSON.stringify(f.close)} differs from base ${JSON.stringify(b.close)}`,
              );
          }
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: original } }).catch(
      () => undefined,
    );
    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
    console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
    console.log(`REPORT problems ${JSON.stringify(problems)}`);
    await ownerCtx.close();
  }
  expect(problems).toEqual([]);
});
