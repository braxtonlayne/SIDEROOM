import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * Wave 111, tint (SEC-AUD-560, D-1035 extended): a tapped message row on a touch device keeps no hover tint; the
 * tint belongs to a real pointer. Measures the row's computed background and the sheet rules that set it:
 * 375 x 812 with touch (Lounge Light and Dark): after a tap on the text, after a long press with the menu closed, a row
 * never touched, a mentioned row (the first message of another author in the seeded channel gets `mention_everyone`
 * through `page.route`; nothing is seeded) and keyboard focus; 1024 x 768 with a mouse (Light): hover, the mentioned
 * row. Two phases (SR_P111_TINT_PHASE=base|after): both write `figures-<phase>.json` and shots (base shots with the
 * `base-` prefix) into SR_P111_TINT_OUT; the after phase compares with `figures-base.json` when it is there and lists
 * every broken limit as a problem line (D-984).
 *
 * The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p111-integ/apps/web/dist --env SR_P111_TINT_ON=1 --env SR_P111_TINT_PHASE=after --env SR_P111_TINT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p111-tint -- tests/p111-tint-shots.spec.ts
 */
const ON = process.env.SR_P111_TINT_ON === '1';
const PHASE = process.env.SR_P111_TINT_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P111_TINT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p111-tint';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 160);

interface World {
  guildId: string;
  channelId: string;
  extras?: { extrasChannelId: string };
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

type Theme = 'light' | 'dark';
interface Cfg {
  tag: '375' | '1024';
  w: number;
  h: number;
  touch: boolean;
}
const C375: Cfg = { tag: '375', w: 375, h: 812, touch: true };
const C1024: Cfg = { tag: '1024', w: 1024, h: 768, touch: false };

interface Figures {
  phase: string;
  vals: Record<string, string | number | boolean | null>;
  notes: string[];
  problems: string[];
  covered: string[];
}
const fig: Figures = { phase: PHASE, vals: {}, notes: [], problems: [], covered: [] };
const note = (s: string) => {
  fig.notes.push(s);
  say(`note: ${s}`);
};
const val = (k: string, v: string | number | boolean | null) => {
  fig.vals[k] = v;
  say(`${k} = ${String(v)}`);
};

async function newPage(
  browser: Browser,
  owner: BrowserContext,
  cfg: Cfg,
  theme: Theme,
  mention: string | null,
) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: cfg.w, height: cfg.h },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(cfg.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((t) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: t, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, `lounge-${theme}`);
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  if (mention) {
    // The first message of another author is shown as one that mentions the member (the world has none that does).
    await page.route(/\/api\/v1\/channels\/\d+\/messages(\?.*)?$/, async (route: Route) => {
      if (route.request().method() !== 'GET') return route.continue();
      const res = await route.fetch();
      let body: unknown;
      try {
        body = await res.json();
      } catch {
        return route.fulfill({ response: res });
      }
      if (Array.isArray(body)) {
        const m = (body as { author?: { id?: string }; mention_everyone?: boolean }[]).find(
          (x) => x.author?.id && x.author.id !== mention,
        );
        if (m) m.mention_everyone = true;
      }
      return route.fulfill({ response: res, json: body });
    });
  }
  return { ctx, page };
}

async function settle(page: Page) {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
}
async function go(page: Page, url: string) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page
    .getByRole('log')
    .first()
    .waitFor({ timeout: 30_000 })
    .catch(() => note(`${url}: log not seen`));
  await settle(page);
}
async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  if (sw > cw) fig.problems.push(`${name}: the page scrolls sideways (${sw} > ${cw})`);
  await page.screenshot({ path: resolve(OUT, `${PHASE === 'base' ? 'base-' : ''}${name}.png`) });
}

/** The row's computed background and the sheet rules that match it now and set a background. */
async function read(row: Locator): Promise<{ bg: string; rules: string[]; marks: string }> {
  return row.evaluate((el) => {
    const rules: string[] = [];
    const walk = (list: CSSRuleList) => {
      for (const r of Array.from(list)) {
        const g = r as CSSGroupingRule & CSSMediaRule & CSSStyleRule;
        if (g.cssRules && g.type !== 1) {
          if (g.type === 4 && !matchMedia(g.conditionText || g.media.mediaText).matches) continue;
          walk(g.cssRules);
        } else if (g.type === 1) {
          const bg =
            g.style.getPropertyValue('background') || g.style.getPropertyValue('background-color');
          if (!bg) continue;
          try {
            if (el.matches(g.selectorText)) rules.push(`${g.selectorText} => ${bg}`);
          } catch {
            /* a selector this engine cannot match */
          }
        }
      }
    };
    for (const s of Array.from(document.styleSheets)) {
      try {
        walk(s.cssRules);
      } catch {
        /* cross-origin sheet */
      }
    }
    const marks = [
      el.matches(':hover') ? 'hover' : '',
      el.matches(':focus-within') ? 'focus-within' : '',
      el.matches(':focus-visible') ? 'focus-visible' : '',
      el.matches(':has(:focus-visible)') ? 'has-focus-visible' : '',
    ]
      .filter(Boolean)
      .join(',');
    return { bg: getComputedStyle(el).backgroundColor, rules, marks };
  });
}
async function record(prefix: string, row: Locator) {
  const r = await read(row);
  val(`${prefix}.bg`, r.bg);
  val(`${prefix}.rules`, r.rules.join(' | '));
  val(`${prefix}.state`, r.marks);
  return r.bg;
}

const articles = (page: Page) => page.locator('article.lng-msg');

async function longPress(page: Page, loc: Locator) {
  await loc.scrollIntoViewIfNeeded({ timeout: 5000 });
  await loc.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const x = r.left + Math.min(r.width / 2, 120);
    const y = r.top + Math.min(r.height / 2, 40);
    const t = new Touch({ identifier: 1, target: el, clientX: x, clientY: y });
    el.dispatchEvent(
      new TouchEvent('touchstart', {
        bubbles: true,
        cancelable: true,
        touches: [t],
        targetTouches: [t],
        changedTouches: [t],
      }),
    );
  });
  await page.waitForTimeout(800);
}

/** Touch at 375: tap, long press and menu closed, untouched row, keyboard focus. */
async function scenTouch(browser: Browser, owner: BrowserContext, w: World, theme: Theme) {
  const ex = w.extras;
  if (!ex) return note('the world has no `extras`');
  const cfg = C375;
  const k = `375|${theme}`;
  const { ctx, page } = await newPage(browser, owner, cfg, theme, null);
  try {
    const url = `/channels/${w.guildId}/${ex.extrasChannelId}`;
    await go(page, url);
    val(
      `${k}|data-theme`,
      await page.evaluate(
        () => document.documentElement.dataset.theme ?? document.body.dataset.theme ?? null,
      ),
    );
    const reacted = articles(page)
      .filter({ has: page.locator('.lng-reactions') })
      .first();
    await reacted.scrollIntoViewIfNeeded();
    const b = await reacted.locator('.lng-msg-content').first().boundingBox();
    if (!b) throw new Error('no message body box');
    // The untouched row: another article of the page, marked before any touch.
    const other = articles(page)
      .filter({ hasNot: page.locator('.lng-reactions') })
      .last();
    await other.evaluate((e) => e.setAttribute('data-p111-other', '1'));
    const untouched = await record(`${k}|untouched`, page.locator('[data-p111-other="1"]'));
    await page.touchscreen.tap(b.x + 8, b.y + Math.min(b.height / 2, 10));
    await page.waitForTimeout(600);
    const tapped = await record(`${k}|tapped`, reacted);
    val(`${k}|tapped.sameAsUntouched`, tapped === untouched);
    await shot(page, `tap-message-375-${theme}`);
    fig.covered.push(`375 ${theme} tap`);

    // A long press and the menu closed.
    await go(page, url);
    const last = articles(page).last();
    await last.evaluate((e) => e.setAttribute('data-p111-lp', '1'));
    try {
      await longPress(page, page.locator('[data-p111-lp="1"]'));
      await page.getByRole('menu').last().waitFor({ timeout: 4000 });
      await page.keyboard.press('Escape');
      await page.waitForTimeout(600);
      const lp = await record(`${k}|afterLongPress`, page.locator('[data-p111-lp="1"]'));
      const un2 = await record(
        `${k}|untouched2`,
        articles(page)
          .filter({ hasNot: page.locator('.lng-reactions') })
          .first(),
      );
      val(`${k}|afterLongPress.sameAsUntouched`, lp === un2);
      fig.covered.push(`375 ${theme} long press`);
    } catch (e) {
      note(`${k} long-press menu not opened (${msg(e)})`);
    }

    // Keyboard focus keeps what it shows today.
    await go(page, url);
    const kart = articles(page)
      .filter({ has: page.locator('.lng-reactions') })
      .first();
    await kart.scrollIntoViewIfNeeded();
    await page.keyboard.press('Shift');
    await kart.locator('button, a[href], [tabindex="0"]').first().focus();
    await page.waitForTimeout(500);
    await record(`${k}|keyboardFocus`, kart);
    fig.covered.push(`375 ${theme} keyboard`);
  } catch (e) {
    note(`${k} touch scenario stopped: ${msg(e)}`);
  } finally {
    await ctx.close();
  }
}

/** The mentioned row (touch at 375 Light; mouse at 1024 Light), and hover at 1024. */
async function scenMention(
  browser: Browser,
  owner: BrowserContext,
  w: World,
  cfg: Cfg,
  meId: string,
) {
  const k = `${cfg.tag}|light`;
  const { ctx, page } = await newPage(browser, owner, cfg, 'light', meId);
  try {
    await go(page, `/channels/${w.guildId}/${w.channelId}`);
    const row = page.locator('article.lng-msg[data-mention]').first();
    if ((await row.count()) === 0) {
      note(`${k} no mentioned row on the page`);
      return;
    }
    await row.scrollIntoViewIfNeeded();
    await record(`${k}|mention`, row);
    if (cfg.touch) {
      await shot(page, 'mention-row-375-light');
      const b = await row.locator('.lng-msg-content').first().boundingBox();
      if (b) {
        await page.touchscreen.tap(b.x + 8, b.y + Math.min(b.height / 2, 10));
        await page.waitForTimeout(500);
        await record(`${k}|mentionTapped`, row);
      }
    } else {
      const b = await row.boundingBox();
      if (b) {
        await page.mouse.move(b.x + 60, b.y + Math.min(b.height / 2, 10));
        await page.waitForTimeout(400);
        await record(`${k}|mentionHover`, row);
      }
    }
    fig.covered.push(`${cfg.tag} mention`);
  } catch (e) {
    note(`${k} mention scenario stopped: ${msg(e)}`);
  } finally {
    await ctx.close();
  }
}

async function scenHover(browser: Browser, owner: BrowserContext, w: World) {
  const ex = w.extras;
  if (!ex) return note('the world has no `extras`');
  const cfg = C1024;
  const { ctx, page } = await newPage(browser, owner, cfg, 'light', null);
  try {
    await go(page, `/channels/${w.guildId}/${ex.extrasChannelId}`);
    const art = articles(page)
      .filter({ has: page.locator('.lng-reactions') })
      .first();
    await art.scrollIntoViewIfNeeded();
    const b = await art.locator('.lng-msg-content').first().boundingBox();
    if (!b) throw new Error('no message body box');
    await page.mouse.move(b.x + 20, b.y + Math.min(b.height / 2, 10));
    await page.waitForTimeout(500);
    const bg = await record('1024|light|hover', art);
    await shot(page, 'hover-message-1024-light');
    await page.mouse.move(2, 2);
    await page.waitForTimeout(300);
    const off = await record('1024|light|noHover', art);
    val('1024|light|hover.differsFromRest', bg !== off);
    fig.covered.push('1024 hover');
  } catch (e) {
    note(`1024 hover scenario stopped: ${msg(e)}`);
  } finally {
    await ctx.close();
  }
}

function compare(base: Figures) {
  const same = (k: string) => {
    if (!(k in base.vals) || !(k in fig.vals)) return;
    if (base.vals[k] !== fig.vals[k])
      fig.problems.push(`${k}: ${String(base.vals[k])} -> ${String(fig.vals[k])}`);
  };
  // Must not change: hover with a mouse, the mentioned rows, keyboard focus, the untouched row.
  for (const k of Object.keys(fig.vals))
    if (/\|(hover|noHover|mention|mentionHover|keyboardFocus|untouched2?)\.bg$/.test(k)) same(k);
  // May change, and must: a touch tap or long press leaves the untouched row's background.
  for (const th of ['light', 'dark']) {
    for (const what of ['tapped', 'afterLongPress']) {
      const key = `375|${th}|${what}.sameAsUntouched`;
      if (fig.vals[key] === false)
        fig.problems.push(`${key} is false: the row keeps a tint after a touch`);
    }
  }
  // The mentioned row after a tap keeps its own tint.
  for (const k of Object.keys(fig.vals))
    if (
      /\|mentionTapped\.bg$/.test(k) &&
      fig.vals[k.replace('mentionTapped', 'mention')] !== fig.vals[k]
    )
      fig.problems.push(`${k}: a tapped mentioned row lost its own tint`);
}

test(`wave 111 tint: the message row's tint on touch, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    const meId = (
      (await (await ownerCtx.request.get('/api/v1/users/@me')).json()) as { id: string }
    ).id;
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    await scenTouch(browser, ownerCtx, w, 'light');
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-dark', zoom: 100 } },
    });
    await scenTouch(browser, ownerCtx, w, 'dark');
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    await scenMention(browser, ownerCtx, w, C375, meId);
    await scenHover(browser, ownerCtx, w);
    await scenMention(browser, ownerCtx, w, C1024, meId);
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after') {
      if (existsSync(baseFile)) compare(JSON.parse(readFileSync(baseFile, 'utf8')) as Figures);
      else note(`no ${baseFile}: nothing compared`);
    }
  } finally {
    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(fig, null, 1));
    for (const p of fig.problems) say(`PROBLEM ${p}`);
    say(
      `${PHASE}: ${fig.covered.length} scenarios covered, ${fig.problems.length} problem line(s)`,
    );
    await ownerCtx.close();
  }
});
