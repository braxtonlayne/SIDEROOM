import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { over, parseColor, ratio, textRatio, type Rgba } from '../support/contrast';

/**
 * Wave 105, tools builder "border" (SEC-AUD-476, figures only; nothing is changed in the page): what the kit's field
 * boundary measures in every theme, at 1024 x 768, for the kit's TextInput, NativeSelect and textarea wherever the page
 * has one. Per field: computed border colour, field ground, the ground just outside (every ancestor's background laid
 * over the next, down to the first opaque one); ratios border/outside, border/field, field/outside, placeholder and text
 * on the field ground; the border's ratios on `:hover` and the focus ring's colour against the outside ground; and two
 * what-if columns (the border as `--ui-text-faint`, as `--ui-text-muted`, both read from the field under that theme).
 * Places: a Server settings form (page ground), settings sections with cards or panels, the Create Channel dialog (a
 * kit Modal), toolbars (lounge channel page, Library, Cinema), each by the ground the field really sits on.
 * Themes: the five account themes on a Lounge/Settings page (lounge-light, lounge-dark, lounge-ash, lounge-onyx,
 * lounge-editorial) plus `sideroom` and `sideroom-broadcast` set as the main theme too, and the section pages
 * (Library, Cinema: `sideroom`; Live TV: `sideroom-broadcast`; once all `lounge-light`).
 * Finds fields by tag inside a root found by role; the kit's own are the ones whose resting border is `--ui-border` on a
 * `--ui-bg-raised` ground (`kit: true`). Data: none made. Needs the sweep world (`e2e/.state/sweep/`, never edited).
 * The bridge's answers are `page.route` fakes (status, features, channels, conflicts), only to get a card with a select.
 * Writes figures-<phase>.json and shots (`base-` prefix in the base phase) to SR_P105_BORDER_OUT. No comparison: measure only.
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit border shoot base --env SR_P105_BORDER_ON=1 --env SR_P105_BORDER_PHASE=base --env SR_P105_BORDER_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-border -- tests/p105-border-shots.spec.ts
 * (an after round, if ever asked: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_BORDER_ON=1 --env SR_P105_BORDER_PHASE=after --env SR_P105_BORDER_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-border -- tests/p105-border-shots.spec.ts)
 */
const ON = process.env.SR_P105_BORDER_ON === '1';
const PHASE = process.env.SR_P105_BORDER_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P105_BORDER_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-border';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };

const MAIN_THEMES = [
  'lounge-light',
  'lounge-dark',
  'lounge-ash',
  'lounge-onyx',
  'lounge-editorial',
  'sideroom',
  'sideroom-broadcast',
] as const;

interface Raw {
  n: number;
  tag: string;
  type: string;
  name: string;
  inDialog: boolean;
  inToolbar: boolean;
  box: { x: number; y: number; w: number; h: number };
  borderColor: string;
  borderWidth: string;
  borderStyle: string;
  bg: string;
  text: string;
  placeholder: string | null;
  stack: { bg: string; image: boolean; tag: string }[];
  tokens: Record<string, string>;
}

/** Tag every visible field under `root` with data-p105b and read what the browser computes for it. */
async function readFields(root: Locator): Promise<Raw[]> {
  return root.evaluate((el) => {
    const probe = document.createElement('span');
    document.body.appendChild(probe);
    const rgb = (v: string) => {
      probe.style.color = '';
      probe.style.color = v.trim();
      return getComputedStyle(probe).color;
    };
    const sel =
      'input:not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=file]):not([type=range]):not([type=color]):not([type=submit]):not([type=button]):not([type=image]), select, textarea';
    const out: unknown[] = [];
    const all = Array.from(el.querySelectorAll<HTMLElement>(sel));
    let n = 0;
    for (const f of all) {
      const r = f.getBoundingClientRect();
      if (f.getClientRects().length === 0 || r.width < 60 || r.height < 20) continue;
      if (r.bottom < 0 || r.top > window.innerHeight * 4) continue;
      f.setAttribute('data-p105b', String(n));
      const cs = getComputedStyle(f);
      const ph = f.matches('input, textarea') ? getComputedStyle(f, '::placeholder').color : null;
      const stack: { bg: string; image: boolean; tag: string }[] = [];
      for (let a = f.parentElement; a; a = a.parentElement) {
        const acs = getComputedStyle(a);
        const c = acs.backgroundColor;
        const image = acs.backgroundImage !== 'none';
        if (c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') {
          stack.push({ bg: c, image, tag: a.tagName.toLowerCase() });
          if (/^rgb\(/.test(c) || /, 1\)$/.test(c)) break;
        } else if (image)
          stack.push({ bg: 'rgba(0, 0, 0, 0)', image, tag: a.tagName.toLowerCase() });
      }
      const label =
        f.getAttribute('aria-label') ??
        (f.id ? document.querySelector(`label[for="${f.id}"]`)?.textContent : null) ??
        f.closest('label')?.textContent ??
        f.getAttribute('placeholder') ??
        f.id;
      const tok: Record<string, string> = {};
      for (const k of [
        '--ui-border',
        '--ui-text-faint',
        '--ui-text-muted',
        '--ui-focus',
        '--ui-bg',
        '--ui-bg-raised',
      ])
        tok[k] = rgb(cs.getPropertyValue(k) || 'transparent');
      out.push({
        n,
        tag: f.tagName.toLowerCase(),
        type: f.getAttribute('type') ?? '',
        name: (label ?? '').trim().slice(0, 40),
        inDialog: !!f.closest('[role=dialog]'),
        inToolbar: !!f.closest('[role=toolbar], [role=search], header, [role=banner]'),
        box: {
          x: Math.round(r.left),
          y: Math.round(r.top),
          w: Math.round(r.width),
          h: Math.round(r.height),
        },
        borderColor: cs.borderTopColor,
        borderWidth: cs.borderTopWidth,
        borderStyle: cs.borderTopStyle,
        bg: cs.backgroundColor,
        text: cs.color,
        placeholder: ph,
        stack,
        tokens: tok,
      });
      n++;
    }
    probe.remove();
    return out as never;
  });
}

/** Farthest layer first (the opaque one, or white when none), each nearer layer laid over it. */
const flatten = (stack: Raw['stack']): Rgba => {
  let g: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const l of [...stack].reverse()) g = over(parseColor(l.bg), g);
  return g;
};

interface FieldFig {
  cfg?: string;
  page: string;
  place: string;
  tag: string;
  type: string;
  name: string;
  kit: boolean;
  outsideKind: string;
  outside: string;
  outsideHasImage: boolean;
  borderWidth: string;
  border: string;
  fieldGround: string;
  tokens: Record<string, string>;
  ratios: {
    borderOutside: number;
    borderField: number;
    fieldOutside: number;
    placeholder: number | null;
    text: number;
  };
  hover: { border: string; borderOutside: number; borderField: number } | null;
  focus: {
    outlineStyle: string;
    outlineWidth: string;
    outline: string;
    ringOutside: number | null;
    border: string;
    borderOutside: number;
  } | null;
  whatIf: { faintOutside: number; mutedOutside: number; faintField: number; mutedField: number };
  box: Raw['box'];
}
const r2 = (n: number) => Math.round(n * 100) / 100;
const hex = (c: Rgba) =>
  '#' +
  [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('') +
  (c.a < 1 ? `/${r2(c.a)}` : '');

async function measureFields(
  page: Page,
  root: Locator,
  pageName: string,
  maxStates: number,
): Promise<FieldFig[]> {
  const raws = await readFields(root);
  const figs: FieldFig[] = [];
  const stated = new Map<string, number>();
  for (const f of raws) {
    const outside = flatten(f.stack);
    const field = over(parseColor(f.bg), outside);
    const borderVis = (css: string) => over(parseColor(css), field);
    const tk = f.tokens;
    const kit =
      f.borderStyle === 'solid' &&
      f.borderWidth === '1px' &&
      f.borderColor === tk['--ui-border'] &&
      f.bg === tk['--ui-bg-raised'];
    const is = (c: Rgba, css: string) => hex(c) === hex(parseColor(css));
    const outsideKind = is(outside, tk['--ui-bg']!)
      ? 'page ground (--ui-bg)'
      : is(outside, tk['--ui-bg-raised']!)
        ? 'raised (--ui-bg-raised)'
        : `other ${hex(outside)}`;
    const place = f.inDialog
      ? 'dialog'
      : f.inToolbar
        ? 'toolbar/header'
        : outsideKind.startsWith('raised')
          ? 'card/panel'
          : 'page';
    const bv = borderVis(f.borderColor);
    const faint = borderVis(tk['--ui-text-faint']!);
    const muted = borderVis(tk['--ui-text-muted']!);
    const fig: FieldFig = {
      page: pageName,
      place,
      tag: f.tag,
      type: f.type,
      name: f.name,
      kit,
      outsideKind,
      outside: hex(outside),
      outsideHasImage: f.stack.some((s) => s.image),
      borderWidth: f.borderWidth,
      border: hex(bv),
      fieldGround: hex(field),
      tokens: tk,
      ratios: {
        borderOutside: r2(ratio(bv, outside)),
        borderField: r2(ratio(bv, field)),
        fieldOutside: r2(ratio(field, outside)),
        placeholder: f.placeholder
          ? r2(ratio(over(parseColor(f.placeholder), field), field))
          : null,
        text: r2(textRatio(f.text, hex({ ...field, a: 1 }))),
      },
      hover: null,
      focus: null,
      whatIf: {
        faintOutside: r2(ratio(faint, outside)),
        mutedOutside: r2(ratio(muted, outside)),
        faintField: r2(ratio(faint, field)),
        mutedField: r2(ratio(muted, field)),
      },
      box: f.box,
    };
    // Hover and focus: the first field of each tag in this root only (what the table needs).
    if (kit && (stated.get(f.tag) ?? 0) < maxStates) {
      stated.set(f.tag, (stated.get(f.tag) ?? 0) + 1);
      const loc = page.locator(`[data-p105b="${f.n}"]`).first();
      try {
        await loc.scrollIntoViewIfNeeded({ timeout: 2000 });
        await loc.hover({ timeout: 2000 });
        const hb = await loc.evaluate((e) => getComputedStyle(e).borderTopColor);
        const hv = borderVis(hb);
        fig.hover = {
          border: hex(hv),
          borderOutside: r2(ratio(hv, outside)),
          borderField: r2(ratio(hv, field)),
        };
        await page.mouse.move(2, 2);
        await page.keyboard.press('Shift'); // keyboard modality, so :focus-visible matches a select too
        await loc.focus({ timeout: 2000 });
        const fc = await loc.evaluate((e) => {
          const cs = getComputedStyle(e);
          return {
            outlineStyle: cs.outlineStyle,
            outlineWidth: cs.outlineWidth,
            outline: cs.outlineColor,
            border: cs.borderTopColor,
          };
        });
        const ring = fc.outlineStyle !== 'none' && parseFloat(fc.outlineWidth) > 0;
        fig.focus = {
          ...fc,
          ringOutside: ring ? r2(ratio(over(parseColor(fc.outline), outside), outside)) : null,
          borderOutside: r2(ratio(borderVis(fc.border), outside)),
        };
        await loc.blur();
      } catch (e) {
        say(
          `${pageName}: hover/focus of ${f.tag} "${f.name}" not read: ${String(e).slice(0, 120)}`,
        );
      }
    }
    figs.push(fig);
  }
  return figs;
}

async function setPrefs(
  owner: BrowserContext,
  theme: string,
  sectionThemes: Record<string, string>,
) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100, sectionThemes } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.theme, 'account theme read back').toBe(theme);
  for (const [k, v] of Object.entries(sectionThemes))
    expect(back.settings?.user_prefs?.sectionThemes?.[k], `${k} read back`).toBe(v);
}

async function newPage(
  browser: Browser,
  owner: BrowserContext,
  theme: string,
  sectionThemes: Record<string, string>,
) {
  await setPrefs(owner, theme, sectionThemes);
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 1024, height: 768 },
    storageState: await owner.storageState(),
    bypassCSP: true,
  });
  await ctx.addInitScript(
    ([t, s]) => {
      try {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({
            state: { prefs: { theme: t, zoom: 100, sectionThemes: s } },
            version: 2,
          }),
        );
      } catch {
        /* no storage */
      }
    },
    [theme, sectionThemes] as const,
  );
  return { ctx, page: await ctx.newPage() };
}

const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(900);
};
async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const sideways = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  if (sideways > 0) say(`${name}: the page scrolls sideways by ${sideways}px`);
  await page.mouse.move(2, 2);
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}
const dataTheme = (page: Page) =>
  page.evaluate(
    () =>
      document.documentElement.getAttribute('data-theme') ??
      document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
      null,
  );

async function fakeBridge(page: Page, g: string) {
  const json = (body: unknown) => (route: Route) => route.fulfill({ json: body });
  await page.route(
    new RegExp(`/api/v1/guilds/${g}/bridge/discord-channels$`),
    json([{ id: '1600000000000000001', name: 'p105b-one' }]),
  );
  await page.route(new RegExp(`/api/v1/guilds/${g}/sr/features$`), async (route) => {
    const res = await route.fetch();
    const list = (await res.json()) as { id: string; enabled: boolean }[];
    await route.fulfill({
      response: res,
      json: list.map((m) => (m.id === 'bridge.enabled' ? { ...m, enabled: true } : m)),
    });
  });
  await page.route(
    new RegExp(`/api/v1/guilds/${g}/bridge/status$`),
    json({
      platform_on: true,
      module_on: true,
      connected: true,
      bridge: {
        discord_guild_id: '1100000000000000001',
        discord_guild_name: 'p105b-friends',
        bot_user_id: '1100000000000000002',
        bot_name: 'p105b-relay',
        mode: 'whole_guild',
        status: 'connected',
        intents_granted: (1 << 15) | 1,
        created_by: '1',
        created_at: 0,
        invite_url: 'https://discord.com/oauth2/authorize?client_id=1&scope=bot',
      },
      maps: [],
      queued: 0,
      dead_letters: 0,
      leftovers: [],
      erasure_leftovers: [],
      is_owner: true,
    }),
  );
  await page.route(
    new RegExp(`/api/v1/guilds/${g}/bridge/conflicts(\\?.*)?$`),
    json({
      conflicts: [
        {
          id: '1500000000000000001',
          kind: 'cap',
          object_kind: 'channel',
          sr_object_id: '1400000000000000001',
          discord_object_id: '1400000000000000002',
          field: 'name',
          discord_value: 'p105b-new',
          sr_value: 'p105b-old',
          discord_actor_name: 'p105b-actor',
          count: 1,
          status: 'open',
          first_at: Date.now() - 3_600_000,
          last_at: Date.now() - 60_000,
          object_name: 'p105b-general',
        },
      ],
      open: 1,
    }),
  );
}

async function visit(page: Page, url: string) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('main, [role=main], body', { timeout: 20_000 });
  await settled(page);
}

const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 200);

test(`p105 border: field boundary figures, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  const problems: string[] = [];
  const fields: FieldFig[] = [];
  const themeRead: Record<string, string | null> = {};

  const survey = async (cfg: string, page: Page, pageName: string, root: Locator) => {
    const figs = await measureFields(page, root, pageName, 1);
    for (const f of figs) {
      f.cfg = cfg;
      fields.push(f);
    }
    say(
      `${cfg} ${pageName}: ${figs.length} fields (${figs.filter((f) => f.kit).length} kit): ${figs
        .map((f) => `${f.tag}/${f.place}/${f.ratios.borderOutside}`)
        .join(' ')}`,
    );
  };

  try {
    // 1. The five account themes and the two sideroom themes as the main theme: settings pages, channel page, dialog.
    for (const theme of MAIN_THEMES) {
      const cfg = `main:${theme}`;
      const { ctx, page } = await newPage(browser, owner, theme, {});
      try {
        await fakeBridge(page, w.guildId);
        for (const sec of ['', 'bridge', 'invites', 'emoji', 'roles']) {
          try {
            await visit(page, `/settings/server/${w.guildId}${sec ? `/${sec}` : ''}`);
            if (sec === '') {
              themeRead[cfg] = await dataTheme(page);
              if (themeRead[cfg] !== theme)
                problems.push(`${cfg}: data-theme reads ${themeRead[cfg]}`);
              await page.waitForSelector('#ss-system-channel, input, select', { timeout: 15_000 });
            }
            await survey(cfg, page, `settings/${sec || 'overview'}`, page.locator('main').first());
            if (sec === '') await shot(page, `border-${theme}-1024`);
          } catch (e) {
            problems.push(`${cfg} settings/${sec}: not reached: ${msg(e)}`);
          }
        }
        try {
          await visit(page, `/channels/${w.guildId}/${w.channelId}`);
          await survey(cfg, page, 'lounge channel page', page.locator('body'));
          await page
            .getByRole('button', { name: /^Create Channel in / })
            .first()
            .click({ timeout: 8000 });
          const dlg = page.getByRole('dialog', { name: 'Create Channel' });
          await dlg.waitFor({ timeout: 8000 });
          await page.waitForTimeout(600);
          await survey(cfg, page, 'dialog Create Channel', dlg);
          await shot(page, `border-dialog-${theme}-1024`);
        } catch (e) {
          problems.push(`${cfg} dialog: not reached: ${msg(e)}`);
        }
      } finally {
        await ctx.close();
      }
    }

    // 2. Section pages: Library, Cinema (sideroom), Live TV (sideroom-broadcast), then once all lounge-light.
    const sectionSets = [
      {
        cfg: 'section:shipped',
        main: 'lounge-light',
        sec: {
          cinema: 'sideroom',
          library: 'sideroom',
          arcade: 'lounge-editorial',
          'live-tv': 'sideroom-broadcast',
        },
      },
      {
        cfg: 'section:lounge-light',
        main: 'lounge-light',
        sec: {
          cinema: 'lounge-light',
          library: 'lounge-light',
          arcade: 'lounge-light',
          'live-tv': 'lounge-light',
        },
      },
    ];
    for (const s of sectionSets) {
      const { ctx, page } = await newPage(browser, owner, s.main, s.sec);
      try {
        for (const [name, url] of [
          ['library browse', '/library/browse'],
          ['cinema', '/cinema'],
          ['live tv', '/cinema/tv'],
        ] as const) {
          try {
            await visit(page, url);
            themeRead[`${s.cfg} ${name}`] = await dataTheme(page);
            await survey(s.cfg, page, name, page.locator('body'));
            await shot(page, `border-${s.cfg.replace(':', '-')}-${name.replace(' ', '-')}-1024`);
          } catch (e) {
            problems.push(`${s.cfg} ${name}: not reached: ${msg(e)}`);
          }
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    // The account goes back to the shipped default.
    await apiPatch(owner.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100, sectionThemes: {} } },
    }).catch(() => undefined);
    writeFileSync(
      resolve(OUT, `figures-${PHASE}.json`),
      JSON.stringify({ fields, dataThemeRead: themeRead, problems }, null, 1),
    );
    for (const p of problems) say(`PROBLEM ${p}`);
    await owner.close();
  }
  expect(problems, 'places not reached').toEqual([]);
});
