import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { deflateSync } from 'node:zlib';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-335 (wave 81; D-838), on the real stack: a button in a Developers form row is a button's height, level with the
 * fields' controls. Plus two looks (findings only): the authorize page's server picker and the bridge plan view's Tabs
 * at 375 (the plan request answered by page.route: only the kit strip is looked at).
 * Probes: every visible `.dev-actions` row: the row's box, each child's box (button, link, field wrapper) and the box of
 * the control inside a field (input, select), all relative to the row's top; written to OUT/rows-<base|after>.json.
 * SR_P81_BASE=1: the base case (developers.css unchanged): the rows are logged, then two variants of the rule are
 * injected and measured; nothing is asserted. Without it the rule is asserted.
 * Local only (SR_P81_DEVELOPERS=1); shots go to SR_P81_DEVELOPERS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P81_DEVELOPERS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-developers';
const BASE = process.env.SR_P81_BASE === '1';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const THEMES = ['lounge-dark', 'lounge-light'] as const;

test.skip(process.env.SR_P81_DEVELOPERS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number };
const log = (line: string) => console.log(`REPORT ${line}`);
const r1 = (n: number) => Math.round(n * 10) / 10;
let shots = 0;
const rowLog: Record<string, unknown> = {};

/** A solid-colour PNG, built by hand (no dependency). */
function png(size: number, rgb: [number, number, number]): Buffer {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (b: Buffer) => {
    let c = 0xffffffff;
    for (const x of b) c = crcTable[(c ^ x) & 0xff]! ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc(body));
    return Buffer.concat([len, body, sum]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const row = Buffer.concat([Buffer.from([0]), Buffer.from(Array(size).fill(rgb).flat())]);
  const raw = Buffer.concat(Array(size).fill(row));
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    bypassCSP: true,
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

async function reach(name: string, run: () => Promise<void>) {
  try {
    await run();
  } catch (e) {
    log(`UNREACHED ${name}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
  }
}

async function shoot(page: Page, name: string) {
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${name}.png`) });
  shots++;
}

const noSideways = async (page: Page, tag: string) => {
  const over = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  log(`sideways ${tag}: ${over}`);
  expect.soft(over, `${tag}: document scrolls sideways`).toBeLessThanOrEqual(0);
};

const themeOf = (page: Page) =>
  page.evaluate(() => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null);

interface Box {
  dy: number;
  h: number;
  w: number;
  x: number;
}
interface Kid {
  tag: string;
  text: string;
  box: Box;
  ctrl: Box | null;
}
interface Row {
  i: number;
  tag: string;
  h: number;
  w: number;
  kids: Kid[];
}

/** Every visible `.dev-actions` row: boxes relative to the row's top. */
async function rows(page: Page): Promise<Row[]> {
  return page.evaluate(() => {
    const r = (n: number) => Math.round(n * 10) / 10;
    return Array.from(document.querySelectorAll<HTMLElement>('.dev-actions'))
      .filter((e) => e.offsetParent !== null)
      .map((row, i) => {
        const rb = row.getBoundingClientRect();
        const box = (el: Element) => {
          const b = el.getBoundingClientRect();
          return { dy: r(b.top - rb.top), h: r(b.height), w: r(b.width), x: r(b.left - rb.left) };
        };
        return {
          i,
          tag: row.tagName.toLowerCase(),
          h: r(rb.height),
          w: r(rb.width),
          kids: Array.from(row.children).map((k) => {
            const c = k.matches('button,a,input,select')
              ? null
              : k.querySelector('input:not([type=hidden]),select,textarea');
            return {
              tag: k.tagName.toLowerCase(),
              text: (k.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 28),
              box: box(k),
              ctrl: c ? box(c) : null,
            };
          }),
        };
      });
  });
}

/** Log a page's rows, keep them for the diff, and (after the change) assert the rule. */
async function rowCheck(page: Page, key: string) {
  const rs = await rows(page);
  rowLog[key] = rs;
  for (const row of rs) {
    for (const k of row.kids) {
      if (k.tag !== 'button' && k.tag !== 'a') continue;
      const mate = row.kids.find(
        (m) =>
          m !== k &&
          m.ctrl &&
          m.ctrl.dy < k.box.dy + k.box.h - 2 &&
          m.ctrl.dy + m.ctrl.h > k.box.dy + 2 &&
          m.ctrl.w > 0,
      );
      log(
        `row ${key}#${row.i} <${row.tag}> h=${row.h} ${k.tag} "${k.text}" top=${k.box.dy} h=${k.box.h}` +
          (mate
            ? ` | mate "${mate.text}" ctrl top=${mate.ctrl!.dy} h=${mate.ctrl!.h} (d top ${r1(k.box.dy - mate.ctrl!.dy)}, d bottom ${r1(k.box.dy + k.box.h - mate.ctrl!.dy - mate.ctrl!.h)})`
            : ' | alone on its line'),
      );
      if (BASE) continue;
      expect
        .soft(k.box.h, `${key}#${row.i} "${k.text}": a button's height`)
        .toBeLessThanOrEqual(46);
      if (mate?.ctrl && k.tag === 'button') {
        expect
          .soft(
            Math.abs(k.box.dy - mate.ctrl.dy),
            `${key}#${row.i} "${k.text}": top level with the control`,
          )
          .toBeLessThanOrEqual(2);
        expect
          .soft(
            Math.abs(k.box.dy + k.box.h - (mate.ctrl.dy + mate.ctrl.h)),
            `${key}#${row.i} "${k.text}": bottom level with the control`,
          )
          .toBeLessThanOrEqual(2);
      }
    }
  }
}

const VARIANT_A = `.dev-actions > button, .dev-actions > a { align-self: end; }`;
const VARIANT_D = `.dev-actions > button, .dev-actions > a { align-self: end; }
.dev-actions > :is(label, div) ~ :is(button, a) { align-self: start; margin-block-start: 24px; }`;

const APP_TABS = [
  'general',
  'oauth2',
  'bot',
  'installation',
  'commands',
  'emojis',
  'richpresence',
  'activities',
  'review',
];

async function fieldLook(page: Page) {
  return page.evaluate(() => {
    const field = Array.from(
      document.querySelectorAll<HTMLInputElement>(
        '.dev-main input:not([type=file]):not([type=checkbox]):not([type=radio]):not([type=hidden])',
      ),
    ).find((i) => i.offsetParent);
    if (!field) return null;
    const c = getComputedStyle(field);
    return {
      bg: c.backgroundColor,
      color: c.color,
      border: c.borderTopColor,
      radius: c.borderTopLeftRadius,
      block: Math.round(field.getBoundingClientRect().height),
    };
  });
}

test('Developers rows, the authorize picker and the plan tabs', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });

  const stamp = Date.now().toString(36);
  const app = (
    await apiPost<{ application: { id: string; name: string } }>(o, '/applications', {
      name: `Row app ${stamp}`,
    })
  ).application;
  const appId = app.id;
  const redirect = `${ORIGIN}/p81-callback`;
  await reach('redirect uri', async () => {
    await apiPatch(o, `/applications/${appId}`, { redirect_uris: [redirect] });
  });
  const guild = await apiPost<{ id: string; name: string }>(o, '/guilds', {
    name: `Picker ${stamp}`,
  });
  await reach('emoji seed', async () => {
    await apiPost(o, `/applications/${appId}/emojis`, {
      name: `seed_${stamp}`,
      image: `data:image/png;base64,${png(64, [200, 80, 40]).toString('base64')}`,
    });
  });
  log(`world app=${appId} guild=${guild.id} redirect=${redirect} base=${BASE}`);

  const phase = BASE ? 'base' : 'after';
  const prefsList: Prefs[] = THEMES.map((theme) => ({ theme, zoom: 100 }));
  for (const prefs of BASE ? prefsList.slice(0, 1) : prefsList) {
    const short = prefs.theme.replace('lounge-', '');
    await setPrefs(prefs);
    for (const [sname, size] of Object.entries(SIZES)) {
      if (BASE && sname !== 'desktop') continue;
      const phone = sname === 'phone';
      const tag = `${short}-${sname}`;
      const c = await context(browser, octx, size, prefs);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);

      // The two forms: boxes, the shots, and (base) the two variants of the rule.
      for (const name of ['richpresence', 'emojis'] as const) {
        await reach(`${name}-${tag}`, async () => {
          await page.goto(`/developers/applications/${appId}?tab=${name}`);
          await expect(page.locator('.dev-main form.dev-actions').first()).toBeVisible();
          await page.waitForTimeout(400);
          log(`theme ${tag} ${await themeOf(page)}`);
          await page.locator('.dev-main form.dev-actions').first().scrollIntoViewIfNeeded();
          await rowCheck(page, `${name}-${tag}`);
          await noSideways(page, `${name}-${tag}`);
          await shoot(page, `${phase}-${name}-${tag}`);
          if (BASE) {
            for (const [vname, css] of [
              ['A', VARIANT_A],
              ['D', VARIANT_D],
            ] as const) {
              const h = await page.addStyleTag({ content: css });
              await page.waitForTimeout(150);
              await rowCheck(page, `${name}-${tag}-variant${vname}`);
              await shoot(page, `variant${vname}-${name}-${tag}`);
              await h.evaluate((el) => (el as Element).remove());
            }
          }
        });
      }

      // Every other `.dev-actions` row (Dark: both sizes, and the base at 1280).
      if (prefs.theme === 'lounge-dark') {
        const pages: [string, string][] = [
          ['overview', '/developers'],
          ['applications', '/developers/applications'],
          ...APP_TABS.filter((t) => t !== 'richpresence' && t !== 'emojis').map(
            (t) => [t, `/developers/applications/${appId}?tab=${t}`] as [string, string],
          ),
        ];
        for (const [name, url] of pages) {
          await reach(`rows-${name}-${tag}`, async () => {
            await page.goto(url);
            await page.waitForTimeout(600);
            await rowCheck(page, `${name}-${tag}`);
            await noSideways(page, `${name}-${tag}`);
            if (!phone && (name === 'oauth2' || name === 'general'))
              await shoot(page, `${phase}-row-${name}-${tag}`);
          });
        }
      }

      if (BASE) {
        await c.close();
        continue;
      }

      // Click-through (Light, both sizes): pick a file and a type, press Upload, the asset is listed.
      if (prefs.theme === 'lounge-light') {
        await reach(`upload-${tag}`, async () => {
          await page.goto(`/developers/applications/${appId}?tab=richpresence`);
          const form = page.locator('.dev-main form.dev-actions').first();
          await expect(form).toBeVisible();
          const asset = `art-${sname}-${stamp}`;
          await form.locator('input[type=file]').setInputFiles({
            name: `${asset}.png`,
            mimeType: 'image/png',
            buffer: png(128, [40, 120, 200]),
          });
          await form.locator('select').selectOption('2');
          await expect(form.getByRole('button', { name: 'Upload' })).toBeEnabled();
          await shoot(page, `upload-filled-${tag}`);
          await form.getByRole('button', { name: 'Upload' }).click();
          await expect(page.getByText(asset).first()).toBeVisible({ timeout: 15_000 });
          log(`upload ${tag}: asset ${asset} listed`);
          await page.locator('.dev-main form.dev-actions').first().scrollIntoViewIfNeeded();
          await shoot(page, `upload-listed-${tag}`);
          await rowCheck(page, `richpresence-after-upload-${tag}`);
        });
      }

      // The authorize page's server picker, against the kit field's look (read on the app's General page).
      await reach(`authorize-${tag}`, async () => {
        await page.goto(`/developers/applications/${appId}?tab=general`);
        await page.waitForTimeout(500);
        const field = await fieldLook(page);
        const q = new URLSearchParams({
          client_id: appId,
          scope: 'bot',
          response_type: 'code',
          redirect_uri: redirect,
        });
        await page.goto(`/oauth2/authorize?${q.toString()}`);
        const sel = page.locator('#dev-auth-guild');
        await expect(sel).toBeVisible();
        await page.waitForTimeout(400);
        const look = await sel.evaluate((el) => {
          const c = getComputedStyle(el);
          const b = el.getBoundingClientRect();
          return {
            bg: c.backgroundColor,
            color: c.color,
            border: c.borderTopColor,
            radius: c.borderTopLeftRadius,
            block: Math.round(b.height),
            right: Math.round(b.right),
            opts: Array.from((el as HTMLSelectElement).options).map(
              (x) => `${x.text}${x.disabled ? ' (disabled)' : ''}`,
            ),
          };
        });
        log(
          `authorize select ${tag} theme=${await themeOf(page)} ${JSON.stringify({ look, field })}`,
        );
        if (field) {
          expect.soft(look.bg, `${tag}: background`).toBe(field.bg);
          expect.soft(look.color, `${tag}: colour`).toBe(field.color);
          expect.soft(look.border, `${tag}: border`).toBe(field.border);
          expect.soft(look.radius, `${tag}: radius`).toBe(field.radius);
          expect.soft(look.block, `${tag}: block size`).toBeGreaterThanOrEqual(field.block);
        }
        expect
          .soft(look.right, `${tag}: picker inside the viewport`)
          .toBeLessThanOrEqual(size.width);
        await noSideways(page, `authorize-${tag}`);
        await sel.scrollIntoViewIfNeeded();
        await shoot(page, `authorize-picker-${tag}`);
        // "Authorize" is never pressed (its redirect would leave for the page's own callback).
      });

      // The plan view's Tabs on a phone, the plan answered by page.route (Dark and Light, 375 only).
      if (phone) {
        await reach(`plan-${tag}`, async () => {
          const g = guild.id;
          // The catch-all comes first: a later route wins.
          await c.route(`**/api/v1/guilds/${g}/bridge/**`, (r) => r.fulfill({ json: [] }));
          await c.route(`**/api/v1/guilds/${g}/sr/features`, (r) =>
            r.request().method() === 'GET'
              ? r.fulfill({
                  json: [
                    {
                      id: 'bridge.enabled',
                      enabled: true,
                      platform_disabled: false,
                      default: false,
                    },
                  ],
                })
              : r.fallback(),
          );
          await c.route(`**/api/v1/guilds/${g}/bridge/status`, (r) =>
            r.fulfill({
              json: {
                platform_on: true,
                module_on: true,
                connected: true,
                bridge: {
                  discord_guild_id: '123456789012345678',
                  discord_guild_name: 'Fake Discord',
                  bot_user_id: '1',
                  bot_name: 'Fake bot',
                  mode: 'selective',
                  status: 'connected',
                  intents_granted: 32768,
                  created_by: '1',
                  created_at: Date.now(),
                  invite_url: 'https://discord.com/oauth2/authorize?client_id=1',
                },
                maps: [],
                queued: 0,
                dead_letters: 0,
                leftovers: [],
                is_owner: true,
              },
            }),
          );
          await c.route(`**/api/v1/guilds/${g}/bridge/preflight`, (r) =>
            r.fulfill({
              json: {
                checks: ['bot_admin', 'bot_top_role', 'intents', 'caps', 'unique_guild'].map(
                  (id) => ({ id, ok: true, blocker: true }),
                ),
              },
            }),
          );
          const item = (kind: string, name: string, n: number) => ({
            kind,
            action: 'match',
            sr_id: `${n}`,
            discord_id: `${n}`,
            name,
            sr_permissions: '0',
            discord_permissions: '0',
            dangerous_bits: '0',
            import: false,
          });
          await c.route(`**/api/v1/guilds/${g}/bridge/plan`, (r) =>
            r.fulfill({ json: { plan_id: 'p81plan' } }),
          );
          await c.route(`**/api/v1/guilds/${g}/bridge/plan/p81plan`, (r) =>
            r.fulfill({
              json: {
                id: 'p81plan',
                status: 'ready',
                seed_structure: 'sr',
                plan_hash: 'hash',
                expires_at: Date.now() + 900_000,
                blockers: [],
                items: [
                  ...Array.from({ length: 12 }, (_, n) => item('role', `Role ${n}`, n)),
                  ...Array.from({ length: 14 }, (_, n) => item('channel', `chan-${n}`, 100 + n)),
                  item('thread', 'a thread', 200),
                ],
                not_mirrored: [],
                estimated_writes: 40,
              },
            }),
          );
          await page.goto(`/settings/server/${g}/bridge`);
          await page.getByRole('button', { name: 'Prepare the whole-server plan' }).click();
          await page.getByRole('button', { name: 'Build the plan' }).click();
          const tabs = page.getByRole('tab');
          await expect(tabs.first()).toBeVisible();
          const measure = async (label: string) => {
            const r = await page.evaluate(() => {
              const list = document.querySelector<HTMLElement>('[role=tablist]')!;
              const sel = list.querySelector<HTMLElement>('[role=tab][aria-selected=true]')!;
              const lb = list.getBoundingClientRect();
              const sb = sel.getBoundingClientRect();
              return {
                attr: list.getAttribute('data-overflow'),
                scrollLeft: Math.round(list.scrollLeft),
                scrollWidth: list.scrollWidth,
                clientWidth: list.clientWidth,
                listLeft: Math.round(lb.left),
                listRight: Math.round(lb.right),
                selLeft: Math.round(sb.left),
                selRight: Math.round(sb.right),
                selected: sel.textContent,
                docOver:
                  document.documentElement.scrollWidth - document.documentElement.clientWidth,
              };
            });
            log(`plan tabs ${tag} ${label} ${JSON.stringify(r)}`);
            const max = r.scrollWidth - r.clientWidth;
            const want =
              max <= 1
                ? null
                : r.scrollLeft > 1 && r.scrollLeft < max - 1
                  ? 'both'
                  : r.scrollLeft > 1
                    ? 'start'
                    : 'end';
            expect.soft(r.attr, `plan tabs ${tag} ${label}: data-overflow`).toBe(want);
            expect
              .soft(r.selLeft, `${label}: selected tab inside (left)`)
              .toBeGreaterThanOrEqual(r.listLeft - 1);
            expect
              .soft(r.selRight, `${label}: selected tab inside (right)`)
              .toBeLessThanOrEqual(r.listRight + 1);
            expect.soft(r.docOver, `${label}: document sideways`).toBeLessThanOrEqual(0);
          };
          log(`theme ${tag} ${await themeOf(page)}`);
          await page.getByRole('tablist').scrollIntoViewIfNeeded();
          await measure('first');
          await shoot(page, `plan-tabs-first-${tag}`);
          await tabs.last().click();
          await page.waitForTimeout(400);
          await measure('last');
          await shoot(page, `plan-tabs-last-${tag}`);
        });
      }

      await c.close();
    }
  }

  writeFileSync(resolve(OUT, `rows-${phase}.json`), JSON.stringify(rowLog, null, 1));
  log(`SUMMARY shots=${shots}`);
  await octx.close();
});
