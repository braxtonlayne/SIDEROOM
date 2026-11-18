import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTest } from '../support/hit-test';
import { over, parseColor, ratio, textRatio, type Rgba } from '../support/contrast';

/**
 * Wave 106, census builder "fields" (D-1018's follow-up; MEASURE ONLY, nothing is changed in the page): every text field
 * that is NOT the kit's (raw input / textarea styled by an area sheet), and what its boundary measures: border colour,
 * field ground, the ground outside, the ratios border/outside, border/field, field/outside, placeholder and text on the
 * field ground; the field's height; hover and keyboard focus (border, outline colour/width/offset, box-shadow, "no
 * indicator"); the sheet rules that draw its border and ground (selector text, read from the page's stylesheets); and at
 * 375 touch its height and the hit test (`hitTest`, default reach). A kit field on the same page is read the same way.
 * Parts (SR_P106_FIELDS_PART): 1 = Lounge, settings, forum, Study at 1024 in Lounge Light and Lounge Dark;
 * 2 = the tools drawer in Light, 5 = the tools drawer in Dark; 4 = the section pages Library, Cinema, Emulator,
 * Developers, Admin (shipped theme, then lounge-light); 3 = the Lounge/settings, tools and section pages at 375 touch in Light. Data (D-942): one text
 * channel `p106-fields-<id>` in the sweep guild for the Poll and Tier list rooms, deleted in a finally. Needs the sweep
 * world (`e2e/.state/sweep/`, never edited). Writes figures-part<n>.json and shots (`base-` prefix in the base phase)
 * into SR_P106_FIELDS_OUT. Measure only: no comparison, no PHASE other than base.
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit fields shoot base --env SR_P106_FIELDS_ON=1 --env SR_P106_FIELDS_PART=1 --env SR_P106_FIELDS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-fields -- tests/p106-fields-shots.spec.ts
 * (an after round is not part of this census; if ever asked: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_FIELDS_ON=1 --env SR_P106_FIELDS_PART=1 --env SR_P106_FIELDS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-fields -- tests/p106-fields-shots.spec.ts)
 */
const ON = process.env.SR_P106_FIELDS_ON === '1';
const PART = Number(process.env.SR_P106_FIELDS_PART ?? '1');
const OUT =
  process.env.SR_P106_FIELDS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-fields';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 160);
const r2 = (n: number) => Math.round(n * 100) / 100;
const hex = (c: Rgba) =>
  '#' +
  [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('') +
  (c.a < 1 ? `/${r2(c.a)}` : '');

interface Cfg {
  tag: string;
  main: string;
  sections: Record<string, string>;
  w: number;
  h: number;
  touch: boolean;
}
const SHIPPED_SECTIONS = {
  cinema: 'sideroom',
  library: 'sideroom',
  arcade: 'lounge-editorial',
  'live-tv': 'sideroom-broadcast',
};
const LIGHT_SECTIONS = {
  cinema: 'lounge-light',
  library: 'lounge-light',
  arcade: 'lounge-light',
  'live-tv': 'lounge-light',
};
const C_LIGHT: Cfg = {
  tag: 'light',
  main: 'lounge-light',
  sections: {},
  w: 1024,
  h: 768,
  touch: false,
};
const C_DARK: Cfg = {
  tag: 'dark',
  main: 'lounge-dark',
  sections: {},
  w: 1024,
  h: 768,
  touch: false,
};
const C_SHIP: Cfg = {
  tag: 'shipped',
  main: 'lounge-light',
  sections: SHIPPED_SECTIONS,
  w: 1024,
  h: 768,
  touch: false,
};
const C_SECL: Cfg = {
  tag: 'seclight',
  main: 'lounge-light',
  sections: LIGHT_SECTIONS,
  w: 1024,
  h: 768,
  touch: false,
};
const C_PHONE: Cfg = {
  tag: 'phone',
  main: 'lounge-light',
  sections: {},
  w: 375,
  h: 812,
  touch: true,
};

interface RuleRead {
  sel: string;
  props: Record<string, string>;
}
interface Raw {
  n: number;
  tag: string;
  type: string;
  cls: string;
  name: string;
  inDialog: boolean;
  box: { x: number; y: number; w: number; h: number };
  borderColor: string;
  borderWidth: string;
  borderStyle: string;
  radius: string;
  bg: string;
  text: string;
  placeholder: string | null;
  stack: { bg: string; image: boolean; tag: string }[];
  tokens: Record<string, string>;
  rules: RuleRead[];
  wrap: {
    tag: string;
    cls: string;
    borderColor: string;
    borderWidth: string;
    borderStyle: string;
    bg: string;
    stack: { bg: string; image: boolean; tag: string }[];
    rules: RuleRead[];
    box: { w: number; h: number };
  } | null;
  kit: boolean;
}

/** Tag every visible text field under `root` with data-p106f and read what the browser computes for it. */
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
      'input:not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=file]):not([type=range]):not([type=color]):not([type=submit]):not([type=button]):not([type=image]):not([type=reset]), textarea, [contenteditable=""], [contenteditable="true"]';
    const PROPS = [
      'border',
      'border-top',
      'border-color',
      'border-top-color',
      'border-width',
      'border-style',
      'background',
      'background-color',
      'outline',
      'box-shadow',
    ];
    const rulesOf = (f: Element): { sel: string; props: Record<string, string> }[] => {
      const out: { sel: string; props: Record<string, string> }[] = [];
      const walk = (list: CSSRuleList) => {
        for (const rule of Array.from(list)) {
          const any = rule as CSSRule & { cssRules?: CSSRuleList; selectorText?: string };
          if (any.selectorText !== undefined) {
            let ok = false;
            try {
              ok = f.matches(any.selectorText);
            } catch {
              ok = false;
            }
            if (ok) {
              const st = (rule as CSSStyleRule).style;
              const props: Record<string, string> = {};
              for (const p of PROPS) {
                const v = st.getPropertyValue(p);
                if (v) props[p] = v.slice(0, 80);
              }
              if (Object.keys(props).length)
                out.push({ sel: any.selectorText.slice(0, 120), props });
            }
          }
          if (any.cssRules && any.cssRules.length) {
            // @media / @supports / @layer: only when the condition holds now
            const m = rule as CSSMediaRule;
            if (m.media !== undefined && !window.matchMedia(m.media.mediaText).matches) continue;
            walk(any.cssRules);
          }
        }
      };
      for (const ss of Array.from(document.styleSheets)) {
        try {
          walk(ss.cssRules);
        } catch {
          /* cross-origin sheet */
        }
      }
      return out;
    };
    const out: unknown[] = [];
    // The side that draws the border (a field with only a bottom line still has a boundary).
    const sideOf = (c: CSSStyleDeclaration) =>
      ['Top', 'Right', 'Bottom', 'Left'].find(
        (s) =>
          (c as unknown as Record<string, string>)['border' + s + 'Style'] !== 'none' &&
          parseFloat(String((c as unknown as Record<string, string>)['border' + s + 'Width'])) > 0,
      ) ?? 'Top';
    const bp = (c: CSSStyleDeclaration, k: string) =>
      (c as unknown as Record<string, string>)['border' + sideOf(c) + k] as string;
    const all = Array.from(el.querySelectorAll<HTMLElement>(sel));
    let n = 0;
    for (const f of all) {
      const r = f.getBoundingClientRect();
      if (f.getClientRects().length === 0 || r.width < 40 || r.height < 16) continue;
      if (r.bottom < 0 || r.top > window.innerHeight * 5) continue;
      const cs = getComputedStyle(f);
      if (cs.visibility === 'hidden' || cs.opacity === '0') continue;
      f.setAttribute('data-p106f', String(n));
      const ph = f.matches('input, textarea') ? getComputedStyle(f, '::placeholder').color : null;
      const stackOf = (node: Element) => {
        const st: { bg: string; image: boolean; tag: string }[] = [];
        for (let a = node.parentElement; a; a = a.parentElement) {
          const acs = getComputedStyle(a);
          const c = acs.backgroundColor;
          const image = acs.backgroundImage !== 'none';
          if (c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') {
            st.push({ bg: c, image, tag: a.tagName.toLowerCase() });
            if (/^rgb\(/.test(c) || /, 1\)$/.test(c)) break;
          } else if (image)
            st.push({ bg: 'rgba(0, 0, 0, 0)', image, tag: a.tagName.toLowerCase() });
        }
        return st;
      };
      const stack = stackOf(f);
      // A field with no border of its own: the wrapper that draws the visible box (a border or a ground, about its size).
      let wrap: unknown = null;
      if (bp(cs, 'Style') === 'none' || parseFloat(bp(cs, 'Width')) === 0) {
        let a = f.parentElement;
        for (let k = 0; k < 4 && a && a !== el; k++, a = a.parentElement) {
          const acs = getComputedStyle(a);
          const ar = a.getBoundingClientRect();
          const hb = bp(acs, 'Style') !== 'none' && parseFloat(bp(acs, 'Width')) > 0;
          const hbg =
            acs.backgroundColor !== 'rgba(0, 0, 0, 0)' && acs.backgroundColor !== 'transparent';
          if (
            (hb || hbg) &&
            ar.width <= r.width + 520 &&
            ar.height <= r.height + 100 &&
            ar.left <= r.left + 1 &&
            ar.top <= r.top + 1
          ) {
            a.setAttribute('data-p106w', String(n));
            wrap = {
              tag: a.tagName.toLowerCase(),
              cls: (typeof a.className === 'string' ? a.className : '').trim().slice(0, 80),
              borderColor: bp(acs, 'Color'),
              borderWidth: bp(acs, 'Width'),
              borderStyle: bp(acs, 'Style'),
              bg: acs.backgroundColor,
              stack: stackOf(a),
              rules: rulesOf(a),
              box: { w: Math.round(ar.width), h: Math.round(ar.height * 10) / 10 },
            };
            break;
          }
        }
      }
      const lab = f.id ? document.querySelector('label[for="' + f.id + '"]') : f.closest('label');
      let labText = '';
      if (lab) {
        const c = lab.cloneNode(true) as HTMLElement;
        c.querySelectorAll('select, input, textarea, option').forEach((x) => x.remove());
        labText = c.textContent ?? '';
      }
      const label =
        f.getAttribute('aria-label') || labText || f.getAttribute('placeholder') || f.id || '';
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
      const cls = typeof f.className === 'string' ? f.className.trim() : '';
      // The kit's field wears a CSS-module class (`control`, hashed); the app's own wear a prefix class.
      const kit =
        /(^|\s)(_?control_|[A-Za-z0-9]*_control_|control)/.test(cls) &&
        !/(^|\s)[a-z]{1,5}-/.test(cls);
      out.push({
        n,
        tag: f.tagName.toLowerCase(),
        type: f.getAttribute('type') ?? '',
        cls: cls.slice(0, 80),
        name: label.trim().replace(/\s+/g, ' ').slice(0, 40),
        inDialog: !!f.closest('[role=dialog]'),
        box: {
          x: Math.round(r.left),
          y: Math.round(r.top),
          w: Math.round(r.width),
          h: Math.round(r.height * 10) / 10,
        },
        borderColor: bp(cs, 'Color'),
        borderWidth: bp(cs, 'Width'),
        borderStyle: bp(cs, 'Style'),
        radius: cs.borderTopLeftRadius,
        bg: cs.backgroundColor,
        text: cs.color,
        placeholder: ph,
        stack,
        tokens: tok,
        rules: rulesOf(f),
        wrap,
        kit,
      });
      n++;
    }
    probe.remove();
    return out as never;
  });
}

const flatten = (stack: Raw['stack']): Rgba => {
  let g: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const l of [...stack].reverse()) g = over(parseColor(l.bg), g);
  return g;
};

interface Fig {
  cfg: string;
  stop: string;
  tag: string;
  type: string;
  cls: string;
  name: string;
  kit: boolean;
  inDialog: boolean;
  rules: string[];
  groupKey: string;
  outside: string;
  outsideHasImage: boolean;
  borderWidth: string;
  borderStyle: string;
  border: string;
  fieldGround: string;
  ratios: {
    borderOutside: number | null;
    borderField: number | null;
    fieldOutside: number;
    placeholder: number | null;
    text: number;
  };
  hasBorder: boolean;
  box: Raw['box'];
  hover: { border: string; borderOutside: number; changed: boolean } | null;
  focus: {
    outline: string;
    outlineWidth: string;
    outlineOffset: string;
    outlineStyle: string;
    boxShadow: string;
    border: string;
    indicator: 'outline' | 'box-shadow' | 'border' | 'none';
    ringOutside: number | null;
    borderOutside: number;
  } | null;
  wrap: {
    tag: string;
    cls: string;
    rules: string[];
    border: string;
    hasBorder: boolean;
    ground: string;
    outside: string;
    box: { w: number; h: number };
    ratios: { borderOutside: number | null; borderGround: number | null; groundOutside: number };
    focusIndicator: string;
  } | null;
  hit?: { status: string; height: number; takers: string } | null;
}

async function measure(
  page: Page,
  root: Locator,
  cfg: Cfg,
  stop: string,
  states: boolean,
  hit: boolean,
): Promise<Fig[]> {
  await page.waitForTimeout(500);
  const raws = await readFields(root);
  const figs: Fig[] = [];
  const stated = new Set<string>();
  for (const f of raws) {
    const outside = flatten(f.stack);
    const field = over(parseColor(f.bg), outside);
    const hasBorder = f.borderStyle !== 'none' && parseFloat(f.borderWidth) > 0;
    const borderVis = (css: string) => over(parseColor(css), field);
    const bv = hasBorder ? borderVis(f.borderColor) : null;
    const rules = f.rules.map(
      (r) =>
        `${r.sel} {${Object.entries(r.props)
          .map(([k, v]) => `${k}: ${v}`)
          .join('; ')}}`,
    );
    const groupKey = `${f.tag}|${f.rules
      .filter((r) => Object.keys(r.props).some((k) => /border|background/.test(k)))
      .map((r) => r.sel)
      .join(' + ')}`;
    const fig: Fig = {
      cfg: cfg.tag,
      stop,
      tag: f.tag,
      type: f.type,
      cls: f.cls,
      name: f.name,
      kit: f.kit,
      inDialog: f.inDialog,
      rules,
      groupKey,
      outside: hex(outside),
      outsideHasImage: f.stack.some((s) => s.image),
      borderWidth: f.borderWidth,
      borderStyle: f.borderStyle,
      border: hasBorder ? hex(bv!) : 'none',
      fieldGround: hex(field),
      ratios: {
        borderOutside: bv ? r2(ratio(bv, outside)) : null,
        borderField: bv ? r2(ratio(bv, field)) : null,
        fieldOutside: r2(ratio(field, outside)),
        placeholder: f.placeholder
          ? r2(ratio(over(parseColor(f.placeholder), field), field))
          : null,
        text: r2(textRatio(f.text, hex({ ...field, a: 1 }))),
      },
      hasBorder,
      box: f.box,
      hover: null,
      focus: null,
      wrap: null,
    };
    if (f.wrap) {
      const wo = flatten(f.wrap.stack);
      const wg = over(parseColor(f.wrap.bg), wo);
      const wb = f.wrap.borderStyle !== 'none' && parseFloat(f.wrap.borderWidth) > 0;
      const wbv = wb ? over(parseColor(f.wrap.borderColor), wg) : null;
      fig.wrap = {
        tag: f.wrap.tag,
        cls: f.wrap.cls,
        rules: f.wrap.rules.map(
          (r) =>
            `${r.sel} {${Object.entries(r.props)
              .map(([k, v]) => `${k}: ${v}`)
              .join('; ')}}`,
        ),
        border: wb ? hex(wbv!) : 'none',
        hasBorder: wb,
        ground: hex(wg),
        outside: hex(wo),
        box: f.wrap.box,
        ratios: {
          borderOutside: wbv ? r2(ratio(wbv, wo)) : null,
          borderGround: wbv ? r2(ratio(wbv, wg)) : null,
          groundOutside: r2(ratio(wg, wo)),
        },
        focusIndicator: 'not read',
      };
    }
    const gk = `${cfg.tag}|${stop}|${groupKey}|${f.kit}`;
    const loc = page.locator(`[data-p106f="${f.n}"]`).first();
    if (states && !stated.has(gk)) {
      stated.add(gk);
      try {
        await loc.scrollIntoViewIfNeeded({ timeout: 2000 });
        await loc.evaluate((e) => (e as HTMLElement).blur());
        await page.mouse.move(2, 2);
        const rest = await loc.evaluate((e) => getComputedStyle(e).borderTopColor);
        const wloc = page.locator(`[data-p106w="${f.n}"]`).first();
        const snap = () =>
          wloc.evaluate((e) => {
            const cs = getComputedStyle(e);
            return `${cs.outlineStyle}|${cs.outlineWidth}|${cs.outlineColor}|${cs.boxShadow}|${cs.borderTopColor}`;
          });
        const wRest = f.wrap ? await snap() : '';
        await loc.hover({ timeout: 3000, force: true });
        const hb = await loc.evaluate((e) => getComputedStyle(e).borderTopColor);
        fig.hover = {
          border: hasBorder ? hex(borderVis(hb)) : 'none',
          borderOutside: hasBorder ? r2(ratio(borderVis(hb), outside)) : 0,
          changed: hb !== rest,
        };
        await page.mouse.move(2, 2);
        await page.keyboard.press('Shift'); // keyboard modality: :focus-visible
        await loc.focus({ timeout: 2000 });
        const fc = await loc.evaluate((e) => {
          const cs = getComputedStyle(e);
          return {
            outline: cs.outlineColor,
            outlineWidth: cs.outlineWidth,
            outlineOffset: cs.outlineOffset,
            outlineStyle: cs.outlineStyle,
            boxShadow: cs.boxShadow,
            border: cs.borderTopColor,
          };
        });
        const ring = fc.outlineStyle !== 'none' && parseFloat(fc.outlineWidth) > 0;
        const shadow = fc.boxShadow !== 'none';
        const borderChanged = fc.border !== rest;
        fig.focus = {
          ...fc,
          indicator: ring ? 'outline' : shadow ? 'box-shadow' : borderChanged ? 'border' : 'none',
          ringOutside: ring ? r2(ratio(over(parseColor(fc.outline), outside), outside)) : null,
          borderOutside: hasBorder ? r2(ratio(borderVis(fc.border), outside)) : 0,
        };
        if (f.wrap && fig.wrap) {
          const wNow = await snap();
          const [os, ow, oc, bs, bc] = wNow.split('|');
          const [, , oc0, bs0, bc0] = wRest.split('|');
          fig.wrap.focusIndicator =
            wNow === wRest
              ? 'none'
              : os !== 'none' && parseFloat(ow!) > 0 && oc !== oc0
                ? `outline ${ow} ${oc}`
                : bs !== bs0
                  ? `box-shadow ${bs}`
                  : bc !== bc0
                    ? `border ${bc}`
                    : 'none';
        }
        await loc.blur();
      } catch (e) {
        say(`${stop}: hover/focus of ${f.tag} "${f.name}" not read: ${msg(e)}`);
      }
    }
    if (hit) {
      try {
        const res = await hitTest(page, loc);
        fig.hit = {
          status: res.pass ? 'pass' : 'fail',
          height: res.box.height,
          takers: res.points
            .filter((p) => !p.ok)
            .map((p) => `${p.at}:${p.taker?.tag ?? 'null'}`)
            .join(','),
        };
      } catch (e) {
        fig.hit = { status: 'not-reached', height: f.box.h, takers: msg(e).slice(0, 60) };
      }
    }
    figs.push(fig);
  }
  return figs;
}

async function setPrefs(owner: BrowserContext, cfg: Cfg) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: cfg.main, zoom: 100, sectionThemes: cfg.sections } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.theme, 'account theme read back').toBe(cfg.main);
  for (const [k, v] of Object.entries(cfg.sections))
    expect(back.settings?.user_prefs?.sectionThemes?.[k], `${k} read back`).toBe(v);
}

async function newPage(browser: Browser, owner: BrowserContext, cfg: Cfg) {
  await setPrefs(owner, cfg);
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: cfg.w, height: cfg.h },
    hasTouch: cfg.touch,
    isMobile: false,
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
    [cfg.main, cfg.sections] as const,
  );
  return { ctx, page: await ctx.newPage() };
}

const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(800);
};
async function visit(page: Page, url: string) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await page.waitForSelector('main, [role=main], body', { timeout: 20_000 });
  await settled(page);
}
const dataTheme = (page: Page) =>
  page.evaluate(
    () =>
      document.documentElement.getAttribute('data-theme') ??
      document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
      null,
  );
async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const sideways = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  if (sideways > 0) say(`${name}: the page scrolls sideways by ${sideways}px`);
  await page.mouse.move(2, 2);
  await page.waitForTimeout(250);
  await page.screenshot({ path: resolve(OUT, `base-${name}.png`), timeout: 10_000 });
}

async function openTool(page: Page, query: string, pick: RegExp) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 12_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.getByRole('option', { name: pick }).first().click({ timeout: 6000 });
  await page.waitForTimeout(900);
}
async function closeTool(page: Page) {
  const dlg = page.getByRole('dialog', { name: 'Drawer', exact: true });
  for (let i = 0; i < 3 && (await dlg.isVisible().catch(() => false)); i++) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
  }
}

interface Env {
  g: string;
  c: string;
  room: string;
  forum: string;
}
interface Stop {
  id: string;
  /** The page, opened the way a member does; returns the root to read. */
  open: (page: Page, env: Env, cfg: Cfg) => Promise<Locator>;
  /** Leave the page tidy for the next stop (close a drawer or a popover). */
  close?: (page: Page) => Promise<void>;
}
const goChannel = async (page: Page, env: Env, room = false) => {
  const url = `/channels/${env.g}/${room ? env.room : env.c}`;
  if (!page.url().endsWith(url)) await visit(page, url);
  await page.getByRole('textbox').first().waitFor({ timeout: 40_000 });
  await page.waitForTimeout(500);
};
const esc = async (page: Page) => {
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);
};
const tool = (
  id: string,
  query: string,
  pick: RegExp,
  room = false,
  setup?: (d: Locator, p: Page) => Promise<void>,
): Stop => ({
  id: `tool-${id}`,
  open: async (page, env) => {
    await goChannel(page, env, room);
    await openTool(page, query, pick);
    const d = page.getByRole('dialog', { name: 'Drawer', exact: true });
    await expect(d).toBeVisible();
    if (setup) await setup(d, page);
    await page.waitForTimeout(500);
    return d;
  },
  close: closeTool,
});

const LOUNGE_STOPS: Stop[] = [
  {
    id: 'channel',
    open: async (page, env) => {
      await goChannel(page, env);
      return page.locator('body');
    },
  },
  {
    id: 'emoji-picker',
    open: async (page, env) => {
      await goChannel(page, env);
      await page
        .getByRole('button', { name: /open emoji picker/i })
        .first()
        .click({ timeout: 8000 });
      await page.waitForTimeout(700);
      return page.locator('body');
    },
    close: esc,
  },
  {
    id: 'threads',
    open: async (page, env) => {
      await goChannel(page, env);
      await page
        .getByRole('button', { name: /^threads$/i })
        .first()
        .click({ timeout: 8000 });
      await page.waitForTimeout(700);
      return page.locator('body');
    },
    close: esc,
  },
  {
    id: 'inbox',
    open: async (page, env) => {
      await goChannel(page, env);
      await page
        .getByRole('button', { name: /^inbox/i })
        .first()
        .click({ timeout: 8000 });
      await page.waitForTimeout(700);
      return page.locator('body');
    },
    close: esc,
  },
  {
    id: 'switcher',
    open: async (page, env) => {
      await goChannel(page, env);
      await expect(async () => {
        await page.keyboard.press('ControlOrMeta+k');
        await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
          timeout: 1000,
        });
      }).toPass({ timeout: 20_000 });
      return page.getByRole('dialog', { name: 'Drawer', exact: true });
    },
    close: closeTool,
  },
  {
    id: 'forum',
    open: async (page, env) => {
      await visit(page, `/channels/${env.g}/${env.forum}`);
      await page.getByRole('searchbox', { name: /search posts/i }).waitFor({ timeout: 40_000 });
      return page.locator('body');
    },
  },
  {
    id: 'forum-new-post',
    open: async (page, env) => {
      await visit(page, `/channels/${env.g}/${env.forum}`);
      await page
        .getByRole('button', { name: /new post/i })
        .first()
        .click({ timeout: 15_000 });
      await page.waitForTimeout(700);
      return page.getByRole('dialog').first();
    },
    close: esc,
  },
  {
    id: 'inline-edit',
    open: async (page, env) => {
      await goChannel(page, env, true);
      const box = page.getByRole('textbox').last();
      await box.click();
      await page.keyboard.press('ArrowUp');
      await page.waitForTimeout(700);
      return page.locator('body');
    },
    close: esc,
  },
  {
    id: 'members-popout',
    open: async (page, env) => {
      await goChannel(page, env);
      await page
        .getByRole('button', { name: /^p106 fields message|member/i })
        .first()
        .click({ timeout: 8000 })
        .catch(() => undefined);
      await page.waitForTimeout(700);
      return page.locator('body');
    },
    close: esc,
  },
  {
    id: 'scrapbook',
    open: async (page, env) => {
      await visit(page, `/channels/${env.g}/scrapbook`);
      return page.locator('body');
    },
  },
  {
    id: 'user-settings',
    open: async (page) => {
      await visit(page, '/settings/user');
      return page.locator('body');
    },
  },
  {
    id: 'user-profile',
    open: async (page) => {
      await visit(page, '/settings/user/profile');
      return page.locator('body');
    },
  },
  {
    id: 'server-channels',
    open: async (page, env) => {
      await visit(page, `/settings/server/${env.g}/channels`);
      return page.locator('main').first();
    },
  },
  {
    id: 'server-members',
    open: async (page, env) => {
      await visit(page, `/settings/server/${env.g}/members`);
      return page.locator('main').first();
    },
  },
  {
    id: 'study',
    open: async (page) => {
      await visit(page, '/study');
      return page.locator('body');
    },
  },
];

const TOOL_STOPS: Stop[] = [
  tool('calculator', 'calculator', /^calculator/i),
  tool('programmer', 'programmer', /programmer calculator/i),
  tool('units', 'unit', /unit converter/i),
  tool('percent', 'percent', /percentage calculator/i),
  tool('timezone', 'time zones', /time zones/i),
  tool('invite', 'invite card', /invite card/i),
  tool('json', 'json', /^json/i),
  tool('diff', 'diff', /diff/i),
  tool('url', 'url encode', /url encode/i),
  tool('mdtable', 'markdown table', /markdown table/i),
  tool('base64', 'base64', /base64/i),
  tool('hash', 'hash', /hash/i),
  tool('jwt', 'jwt', /jwt/i),
  tool('regex', 'regex', /regex/i),
  tool('counter', 'count', /(word|character|text) count/i),
  tool('loot', 'loot', /loot tables/i),
  tool('dice', 'dice', /dice/i),
  tool('wheel', 'wheel', /wheel/i),
  tool('audio', 'audio', /audio/i),
  tool('poll', 'poll', /poll builder/i, true, async (d) => {
    await d.getByRole('button', { name: 'Start a new one' }).click();
    await d.getByRole('textbox').first().waitFor({ timeout: 15_000 });
  }),
  tool('tierlist', 'tier list', /tier list/i, true, async (d) => {
    await d.getByRole('button', { name: 'Start a new one' }).click();
    const rows = d.getByRole('button', { name: 'Add the rows S to D' });
    if (await rows.isVisible().catch(() => false)) await rows.click();
    await d.getByLabel('Item', { exact: true }).fill('p106 item');
    await d.getByRole('button', { name: 'Add item' }).click();
  }),
];

const SECTION_STOPS: Stop[] = [
  {
    id: 'library-browse',
    open: async (page) => {
      await visit(page, '/library/browse');
      return page.locator('body');
    },
  },
  {
    id: 'cinema',
    open: async (page) => {
      await visit(page, '/cinema');
      return page.locator('body');
    },
  },
  {
    id: 'cinema-search',
    open: async (page) => {
      await visit(page, '/cinema/search');
      return page.locator('body');
    },
  },
  {
    id: 'emulator',
    open: async (page) => {
      await visit(page, '/games/emulator');
      return page.locator('body');
    },
  },
  {
    id: 'developers',
    open: async (page) => {
      await visit(page, '/developers');
      return page.locator('body');
    },
  },
  {
    id: 'admin-donations',
    open: async (page) => {
      await visit(page, '/admin/donations');
      return page.locator('body');
    },
  },
];

test(`p106 fields: non-kit field boundary figures, part ${PART}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  const problems: string[] = [];
  const figs: Fig[] = [];
  const themeRead: Record<string, string | null> = {};
  let roomId = '';
  const env: Env = { g: w.guildId, c: w.channelId, room: '', forum: '' };
  let forumId = '';

  const plan: { cfg: Cfg; stops: Stop[]; states: boolean; hit: boolean }[] =
    PART === 1
      ? [
          { cfg: C_LIGHT, stops: LOUNGE_STOPS, states: true, hit: false },
          { cfg: C_DARK, stops: LOUNGE_STOPS, states: true, hit: false },
        ]
      : PART === 2
        ? [{ cfg: C_LIGHT, stops: TOOL_STOPS, states: true, hit: false }]
        : PART === 5
          ? [{ cfg: C_DARK, stops: TOOL_STOPS, states: true, hit: false }]
          : PART === 4
            ? [
                { cfg: C_SHIP, stops: SECTION_STOPS, states: true, hit: false },
                { cfg: C_SECL, stops: SECTION_STOPS, states: true, hit: false },
              ]
            : [
                { cfg: C_PHONE, stops: LOUNGE_STOPS, states: false, hit: true },
                {
                  cfg: C_PHONE,
                  stops: [
                    ...TOOL_STOPS.filter((t) =>
                      /tool-(calculator|units|timezone|invite|json|diff|loot|regex|poll|tierlist)$/.test(
                        t.id,
                      ),
                    ),
                    ...SECTION_STOPS,
                  ],
                  states: false,
                  hit: true,
                },
              ];

  try {
    const room = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p106-fields-${Date.now().toString(36)}`,
      type: 0,
    });
    roomId = room.id;
    env.room = room.id;
    await apiPost(owner.request, `/channels/${room.id}/messages`, {
      content: 'p106 fields message',
    });
    const forum = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p106-fields-forum-${Date.now().toString(36)}`,
      type: 15,
    });
    forumId = forum.id;
    env.forum = forum.id;

    for (const p of plan) {
      const { ctx, page } = await newPage(browser, owner, p.cfg);
      try {
        for (const s of p.stops) {
          const key = `${p.cfg.tag}|${s.id}`;
          try {
            const root = await s.open(page, env, p.cfg);
            themeRead[key] = await dataTheme(page);
            const got = await measure(page, root, p.cfg, s.id, p.states, p.hit);
            figs.push(...got);
            writeFileSync(
              resolve(OUT, `figures-part${PART}.json`),
              JSON.stringify({ fields: figs, dataThemeRead: themeRead, problems }, null, 1),
            );
            say(
              `${key}: ${got.length} fields (${got.filter((f) => f.kit).length} kit): ${got
                .map(
                  (f) =>
                    `${f.tag}${f.cls ? '.' + f.cls.split(' ')[0] : ''}/${f.ratios.borderOutside}`,
                )
                .join(' ')}`,
            );
            if (got.length) await shot(page, `${p.cfg.tag}-${s.id}`);
            else problems.push(`${key}: no field found`);
          } catch (e) {
            problems.push(`${key}: not reached: ${msg(e)}`);
          } finally {
            await (s.close ?? (async () => undefined))(page).catch(() => undefined);
          }
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    if (roomId) await owner.request.delete(`/api/v1/channels/${roomId}`).catch(() => undefined);
    if (forumId) await owner.request.delete(`/api/v1/channels/${forumId}`).catch(() => undefined);
    await apiPatch(owner.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100, sectionThemes: {} } },
    }).catch(() => undefined);
    writeFileSync(
      resolve(OUT, `figures-part${PART}.json`),
      JSON.stringify({ fields: figs, dataThemeRead: themeRead, problems }, null, 1),
    );
    for (const p of problems) say(`PROBLEM ${p}`);
    await owner.close();
  }
  // A stop that is not reached is a row of the report, not a failure of the census.
  expect(figs.length, 'fields measured').toBeGreaterThan(0);
});
