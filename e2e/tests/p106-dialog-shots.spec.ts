import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';

/**
 * D-1017 (wave 106, dialog): a kit field in a Lounge dialog shows the kit's own hover and focus.
 * Figures: the Create Channel name field in four states (focused as the dialog opens, at rest with focus elsewhere,
 * hovered, focused by Tab): border-color, outline, box-shadow, :focus-visible, aria-invalid, and the rules of every
 * sheet that the field matches and that set outline or border-color. The same for one field of Create Event and of
 * Invite, and the reference kit field on Server settings > Overview. Lounge Light and Lounge Dark at 1024; Light at 375.
 * Data: none made. Needs the sweep world.
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_DIALOG_ON=1 --env SR_P106_DIALOG_PHASE=after --env SR_P106_DIALOG_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-dialog -- tests/p106-dialog-shots.spec.ts
 */
const ON = process.env.SR_P106_DIALOG_ON === '1';
const PHASE = process.env.SR_P106_DIALOG_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P106_DIALOG_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-dialog';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
type World = { guildId: string; channelId: string };
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 220);

interface Fig {
  borderColor: string;
  outlineStyle: string;
  outlineWidth: string;
  outlineColor: string;
  outlineOffset: string;
  boxShadow: string;
  focusVisible: boolean;
  hover: boolean;
  focused: boolean;
  invalid: string | null;
  active: string;
  rules: string[];
}

/** Reads the field's figures in the page; `rules` lists matching rules that set outline or border colour. */
const READ = (el: Element): Fig => {
  const cs = getComputedStyle(el);
  const rules: string[] = [];
  const walk = (list: CSSRuleList, sheet: string) => {
    for (const r of Array.from(list)) {
      const g = r as CSSGroupingRule & CSSStyleRule;
      if (g.selectorText !== undefined && g.style) {
        let hit = false;
        for (const part of g.selectorText.split(',')) {
          try {
            if (el.matches(part.trim())) hit = true;
          } catch {
            /* a selector this engine cannot parse */
          }
        }
        if (hit) {
          const st = g.style;
          const props = [
            'outline',
            'outline-style',
            'outline-width',
            'outline-color',
            'outline-offset',
            'border-color',
            'border',
            'box-shadow',
          ]
            .filter((p) => st.getPropertyValue(p) !== '')
            .map((p) => `${p}:${st.getPropertyValue(p)}`);
          if (props.length) rules.push(`${sheet} | ${g.selectorText} { ${props.join('; ')} }`);
        }
      }
      if (g.cssRules && g.selectorText === undefined) walk(g.cssRules, sheet);
    }
  };
  for (const s of Array.from(document.styleSheets)) {
    try {
      walk(s.cssRules, (s.href ?? 'inline').split('/').pop() ?? 'inline');
    } catch {
      /* cross-origin sheet */
    }
  }
  return {
    borderColor: cs.borderTopColor,
    outlineStyle: cs.outlineStyle,
    outlineWidth: cs.outlineWidth,
    outlineColor: cs.outlineColor,
    outlineOffset: cs.outlineOffset,
    boxShadow: cs.boxShadow,
    focusVisible: el.matches(':focus-visible'),
    hover: el.matches(':hover'),
    focused: document.activeElement === el,
    invalid: el.getAttribute('aria-invalid'),
    active: `${document.activeElement?.tagName}#${document.activeElement?.id}:${document.activeElement?.getAttribute('aria-label') ?? ''}`,
    rules,
  };
};

const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const sideways = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  if (sideways > 0) say(`${name}: the page scrolls sideways by ${sideways}px`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

async function newPage(
  browser: Browser,
  owner: BrowserContext,
  theme: string,
  size: { width: number; height: number },
  touch: boolean,
) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100, sectionThemes: {} } },
  });
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    hasTouch: touch,
    storageState: await owner.storageState(),
    bypassCSP: true,
  });
  await ctx.addInitScript((t) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({
          state: { prefs: { theme: t, zoom: 100, sectionThemes: {} } },
          version: 2,
        }),
      );
    } catch {
      /* no storage */
    }
  }, theme);
  return { ctx, page: await ctx.newPage() };
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const figs: Record<string, any> = {};
const problems: string[] = [];

/** The four states of one field. `open` is read first, before anything moves, when `readOpen`. */
async function fourStates(
  page: Page,
  key: string,
  field: Locator,
  other: Locator,
  readOpen: boolean,
) {
  const out: Record<string, Fig> = {};
  const h = await field.elementHandle();
  if (!h) throw new Error(`${key}: field not found`);
  if (readOpen) out.open = await h.evaluate(READ);
  await other.focus();
  await page.mouse.move(2, 2);
  await page.waitForTimeout(150);
  out.rest = await h.evaluate(READ);
  const box = await field.boundingBox();
  if (box) await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForTimeout(150);
  out.hover = await h.evaluate(READ);
  await page.mouse.move(2, 2);
  await field.focus();
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Tab');
  await page.waitForTimeout(150);
  out.tab = await h.evaluate(READ);
  say(
    `${key}: ` +
      Object.entries(out)
        .map(
          ([s, f]) =>
            `${s}[border ${f.borderColor}; outline ${f.outlineStyle} ${f.outlineWidth} ${f.outlineColor}; fv ${f.focusVisible}; hover ${f.hover}; focused ${f.focused}; invalid ${f.invalid}; active ${f.active}]`,
        )
        .join(' '),
  );
  figs[key] = out;
}

/** Focus ring of another control and the boxes of the dialog (must not change). */
async function boxesAndRings(page: Page, key: string, dlg: Locator) {
  const boxes = await dlg.evaluate((d) => {
    const r = (e: Element) => {
      const b = e.getBoundingClientRect();
      return [b.x, b.y, b.width, b.height].map((n) => Math.round(n * 10) / 10);
    };
    return {
      dialog: r(d),
      controls: Array.from(
        d.querySelectorAll('input, button, select, textarea, label, fieldset'),
      ).map((e) => `${e.tagName.toLowerCase()}:${e.getAttribute('type') ?? ''}:${r(e).join(',')}`),
    };
  });
  const rings: string[] = [];
  const ctl = dlg.locator('input[type=radio], button').first();
  if (await ctl.count()) {
    await ctl.focus();
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Tab');
    rings.push(
      await ctl.evaluate((e) => {
        const c = getComputedStyle(e);
        return `${e.tagName}: ${c.outlineStyle} ${c.outlineWidth} ${c.outlineColor} ${c.outlineOffset}`;
      }),
    );
  }
  figs[`${key}:boxes`] = { boxes, rings };
}

async function openCreateChannel(page: Page, w: World, phone: boolean) {
  await page.goto(phone ? `/channels/${w.guildId}` : `/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 30_000,
  });
  await settled(page);
  await page
    .getByRole('button', { name: /^Create Channel in / })
    .first()
    .click({ timeout: 10_000 });
  const dlg = page.getByRole('dialog', { name: 'Create Channel' });
  await dlg.waitFor({ timeout: 10_000 });
  await page.waitForTimeout(600);
  return dlg;
}

async function openFromGuildMenu(page: Page, w: World, item: string, dlgName: RegExp) {
  const guild = (await (await page.request.get(`/api/v1/guilds/${w.guildId}`)).json()) as {
    name: string;
  };
  await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 30_000,
  });
  await settled(page);
  await page
    .getByTestId('channel-sidebar')
    .first()
    .getByRole('button', { name: guild.name })
    .first()
    .click();
  await page.getByRole('menuitem', { name: item }).click();
  const dlg = page.getByRole('dialog', { name: dlgName });
  await dlg.waitFor({ timeout: 15_000 });
  await settled(page);
  return dlg;
}

test(`p106 dialog: kit field hover and focus in Lounge dialogs, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    for (const theme of ['lounge-light', 'lounge-dark'] as const) {
      const tn = theme === 'lounge-light' ? 'light' : 'dark';
      const { ctx, page } = await newPage(
        browser,
        owner,
        theme,
        { width: 1024, height: 768 },
        false,
      );
      try {
        try {
          const dlg = await openCreateChannel(page, w, false);
          await shot(page, `dialog-open-1024-${tn}`);
          await boxesAndRings(page, `${theme}:createChannel`, dlg);
          await page.keyboard.press('Escape');
          const dlg2 = await openCreateChannel(page, w, false);
          const name = dlg2.getByRole('textbox', { name: /channel name/i }).first();
          await fourStates(
            page,
            `${theme}:createChannel`,
            name,
            dlg2.getByRole('button').first(),
            true,
          );
          if (theme === 'lounge-light') {
            await page.mouse.move(2, 2);
            await name.focus();
            await page.keyboard.press('Shift+Tab');
            await page.keyboard.press('Tab');
            await shot(page, 'dialog-tab-focus-1024-light');
            await dlg2.getByRole('button').first().focus();
            const b = await name.boundingBox();
            if (b) await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
            await page.waitForTimeout(200);
            await shot(page, 'dialog-hover-1024-light');
          }
          await page.keyboard.press('Escape');
        } catch (e) {
          problems.push(`${theme} createChannel: ${msg(e)}`);
        }
        try {
          const dlg = await openFromGuildMenu(page, w, 'Create Event', /Create Event/);
          const f = dlg.locator('input[type=text], input:not([type])').first();
          await fourStates(page, `${theme}:createEvent`, f, dlg.getByRole('button').first(), true);
          if (theme === 'lounge-light') await shot(page, 'event-open-1024-light');
          await page.keyboard.press('Escape');
        } catch (e) {
          problems.push(`${theme} createEvent: ${msg(e)}`);
        }
        try {
          const dlg = await openFromGuildMenu(page, w, 'Invite to Server', /Invite people to/);
          const f = dlg.locator('input[type=text], input:not([type]), input[type=number]').first();
          if (await f.count())
            await fourStates(page, `${theme}:invite`, f, dlg.getByRole('button').first(), true);
          else say(`${theme}:invite has no text field`);
          await page.keyboard.press('Escape');
        } catch (e) {
          problems.push(`${theme} invite: ${msg(e)}`);
        }
        try {
          await page.goto(`/settings/server/${w.guildId}`, {
            waitUntil: 'domcontentloaded',
            timeout: 30_000,
          });
          await settled(page);
          const f = page.locator('main input[type=text], main input:not([type])').first();
          await f.waitFor({ timeout: 15_000 });
          await fourStates(
            page,
            `${theme}:settingsRef`,
            f,
            page.locator('main button').first(),
            false,
          );
        } catch (e) {
          problems.push(`${theme} settingsRef: ${msg(e)}`);
        }
      } finally {
        await ctx.close();
      }
    }
    {
      const { ctx, page } = await newPage(
        browser,
        owner,
        'lounge-light',
        { width: 375, height: 812 },
        true,
      );
      try {
        const dlg = await openCreateChannel(page, w, true);
        await boxesAndRings(page, 'lounge-light:createChannel375', dlg);
        await shot(page, 'dialog-open-375-light');
      } catch (e) {
        problems.push(`375 createChannel: ${msg(e)}`);
      } finally {
        await ctx.close();
      }
    }
  } finally {
    await apiPatch(owner.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100, sectionThemes: {} } },
    }).catch(() => undefined);
    writeFileSync(
      resolve(OUT, `figures-${PHASE}.json`),
      JSON.stringify({ figs, problems }, null, 1),
    );
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as { figs: Record<string, any> };
      const noRules = (f: Fig) => JSON.stringify({ ...f, rules: 0 });
      for (const [k, v] of Object.entries(figs)) {
        const b = base.figs[k];
        if (!b) continue;
        if (k.endsWith(':boxes')) {
          if (JSON.stringify(b.boxes) !== JSON.stringify(v.boxes))
            problems.push(`${k}: a box moved`);
          if (JSON.stringify(b.rings) !== JSON.stringify(v.rings))
            problems.push(`${k}: another control's ring changed ${b.rings} -> ${v.rings}`);
        } else if (k.endsWith('settingsRef')) {
          for (const s of Object.keys(b))
            if (noRules(b[s]) !== noRules(v[s]))
              problems.push(`${k} ${s}: the reference field changed`);
        } else if (b.rest && noRules(b.rest) !== noRules(v.rest)) {
          problems.push(`${k}: the field at rest changed`);
        }
      }
    }
    for (const p of problems) say(`PROBLEM ${p}`);
    await owner.close();
  }
  expect(problems, 'problems').toEqual([]);
});
