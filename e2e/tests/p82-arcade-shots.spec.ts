import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-340 (wave 82; D-844): an Arcade hero card is a surface kept dark. On the Arcade home's two hero cards,
 * `sectionThemes.arcade` set to `lounge-editorial` (shipped), `lounge-light` and `lounge-dark` (each read back from the
 * room's root), at 1280x800 and 375x812, it logs, for the title, the description, the meta line and the kicker: the
 * text colour and the pixels behind the text's box (left, middle, right x top, middle, bottom, from a second shot with
 * the card's body hidden) with the contrast of each; a probe of every colour token the card reads (inside a hero card,
 * and inside one row card for the control); every card's box; the sideways scroll; a hash of each hero card's pixels.
 * `SR_P82_BASE=1` names the run `base` (run it with arcade.css held back); the run after asserts the fix and that the
 * shipped theme's probes, boxes and pixels equal the base's.
 * Local only (SR_P82_ARCADE=1); shots go to SR_P82_ARCADE_OUT. The spec logs; the shots are looked at by the builder.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P82_ARCADE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-arcade';
const MODE = process.env.SR_P82_BASE === '1' ? 'base' : 'after';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const THEMES = [
  ['editorial', 'lounge-editorial'],
  ['light', 'lounge-light'],
  ['dark', 'lounge-dark'],
] as const;

test.skip(process.env.SR_P82_ARCADE !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };

const log = (line: string) => {
  console.log(`REPORT ${line}`);
  appendFileSync(resolve(OUT, `report-${MODE}.txt`), `${line}\n`);
};

const json = (r: Route, body: unknown) =>
  r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

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

// ---- pixels: the viewport shot is decoded in a blank page with a canvas (no image library is installed)
let decoder: Page | null = null;
async function pixels(
  browser: Browser,
  png: Buffer,
  pts: [number, number][],
): Promise<[number, number, number][]> {
  if (!decoder) decoder = await browser.newPage();
  return decoder.evaluate(
    async ([b64, points]) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const cv = document.createElement('canvas');
      cv.width = img.width;
      cv.height = img.height;
      const cx = cv.getContext('2d')!;
      cx.drawImage(img, 0, 0);
      return (points as [number, number][]).map(([x, y]) => {
        const d = cx.getImageData(Math.round(x), Math.round(y), 1, 1).data;
        return [d[0]!, d[1]!, d[2]!] as [number, number, number];
      });
    },
    [png.toString('base64'), pts] as const,
  );
}

const parse = (c: string): [number, number, number] => {
  const m = /rgba?\(([^)]+)\)/.exec(c);
  const p = (m?.[1] ?? '0,0,0').split(/[ ,/]+/).map(Number);
  return [p[0]!, p[1]!, p[2]!];
};
const lum = ([r, g, b]: [number, number, number]) => {
  const f = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
};
const contrast = (a: [number, number, number], b: [number, number, number]) => {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
};

/** Every colour token a hero card, its wash, its text, its tag, its flags line, its tips and the kit's button read. */
const TOKENS = [
  '--ui-bg',
  '--ui-bg-raised',
  '--ui-bg-overlay',
  '--ui-bg-hover',
  '--ui-border',
  '--ui-text',
  '--ui-text-muted',
  '--ui-text-faint',
  '--ui-accent',
  '--ui-accent-hover',
  '--ui-accent-ink',
  '--ui-focus',
  '--ui-danger',
  '--ui-danger-ink',
  '--ui-danger-text',
  '--ui-success',
  '--ui-warning',
  '--ui-info',
  '--ui-status-online',
  '--ui-status-idle',
  '--ui-status-dnd',
  '--ui-status-offline',
  '--text-link',
];

async function probeTokens(page: Page, host: string): Promise<Record<string, string> | null> {
  return page.evaluate(
    ([sel, toks]) => {
      const h = document.querySelector<HTMLElement>(sel!);
      if (!h) return null;
      const out: Record<string, string> = {};
      for (const t of toks as string[]) {
        const d = document.createElement('i');
        d.style.cssText = `position:absolute;width:0;height:0;color:var(${t});`;
        h.appendChild(d);
        out[t] = getComputedStyle(d).color;
        d.remove();
      }
      out.cardColor = getComputedStyle(h).color;
      out.cardBorder = getComputedStyle(h).borderTopColor;
      out.colorScheme = getComputedStyle(h).colorScheme;
      return out;
    },
    [host, TOKENS] as const,
  );
}

interface Item {
  name: string;
  card: string;
  rect: number[];
  color: string;
}

/** The four texts of each hero card, by name: text colour and the box of the text itself. */
async function textItems(page: Page): Promise<Item[]> {
  return page.evaluate(() => {
    const out: { name: string; card: string; rect: number[]; color: string }[] = [];
    const cards = Array.from(document.querySelectorAll<HTMLElement>(".arc-card[data-size='hero']"));
    cards.forEach((card, i) => {
      for (const [cls, name] of [
        ['.arc-card-title', 'title'],
        ['.arc-desc', 'description'],
        ['.arc-meta', 'meta'],
        ['.arc-blurb:not([data-tone])', 'kicker'],
      ] as const) {
        const e = card.querySelector<HTMLElement>(cls);
        if (!e) continue;
        const rg = document.createRange();
        rg.selectNodeContents(e);
        const r = rg.getBoundingClientRect();
        out.push({
          name,
          card: `card${i}`,
          rect: [r.left, r.top, r.width, r.height],
          color: getComputedStyle(e).color,
        });
      }
    });
    return out;
  });
}

async function boxes(page: Page): Promise<Record<string, number[]>> {
  return page.evaluate(() => {
    const out: Record<string, number[]> = {};
    document.querySelectorAll<HTMLElement>('.arc-card').forEach((c, i) => {
      const r = c.getBoundingClientRect();
      const t = c.querySelector<HTMLElement>('.arc-card-title');
      const tr = t?.getBoundingClientRect();
      out[`card${i}-${c.dataset.size}`] = [r.left, r.top, r.width, r.height].map(
        (n) => Math.round(n * 10) / 10,
      );
      if (tr)
        out[`title${i}`] = [tr.left, tr.top, tr.width, tr.height].map(
          (n) => Math.round(n * 10) / 10,
        );
    });
    return out;
  });
}

const readJson = (p: string) => (existsSync(p) ? JSON.parse(readFileSync(p, 'utf8')) : null);

test('SEC-AUD-340: an Arcade hero card is kept dark', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });

  const front = {
    featured: [
      {
        application_id: '900',
        name: 'Land.io',
        description:
          'A roguelike for a call, with a longer line so the description wraps to a second line.',
        blurb: 'Our flagship',
        tier: 'TIER_1_ORIGINAL',
        eligibility: {
          browser_play: true,
          guest_links: true,
          featured: true,
          desktop_only: false,
          needs_base_game_hint: false,
        },
        capabilities: ['datachannel', 'guest_play'],
        tip_links: [],
        bundle_sha256: 'a'.repeat(64),
        position: 0,
        featured_at_ms: 1,
      },
    ],
    coming_soon: [],
    flags: [],
  };
  const fake = async (c: BrowserContext) => {
    await c.route('**/api/v1/arcade', (r) => json(r, front));
  };

  let shots = 0;
  const failures: string[] = [];
  for (const [variant, section] of THEMES) {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: section },
    };
    await setPrefs(prefs);
    for (const [sizeName, size] of Object.entries(SIZES)) {
      const tag = `${variant}-${sizeName}`;
      const c = await context(browser, octx, size, prefs);
      await fake(c);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      await page.goto('/games');
      await expect(page.locator('.arc-root')).toBeVisible({ timeout: 60_000 });
      const theme = await page
        .locator('[data-theme]:has(.arc-root)')
        .last()
        .getAttribute('data-theme');
      expect(theme, 'data-theme read back from the room root').toBe(section);
      await expect(page.locator(".arc-card[data-size='hero']")).toHaveCount(2, { timeout: 30_000 });
      await page.waitForTimeout(1000);
      await page.evaluate(() => window.scrollTo(0, 0));

      // sideways scroll
      const sw = await page.evaluate(() => [
        document.documentElement.scrollWidth,
        document.documentElement.clientWidth,
      ]);
      log(`${tag} sideways=${sw[0]}/${sw[1]} data-theme=${theme}`);
      if (MODE === 'after') expect(sw[0]).toBeLessThanOrEqual(sw[1]!);

      // the probes: inside the first hero card, and inside one row card (the control)
      const hero = await probeTokens(page, ".arc-card[data-size='hero']");
      const row = await probeTokens(page, ".arc-card[data-size='row']");
      log(`${tag} PROBE hero ${JSON.stringify(hero)}`);
      log(`${tag} PROBE row ${JSON.stringify(row)}`);
      const bx = await boxes(page);
      log(`${tag} BOXES ${JSON.stringify(bx)}`);

      // the home's top, as the member sees it
      const wantShot =
        MODE === 'after' ||
        (sizeName === 'desktop' && (variant === 'light' || variant === 'editorial'));
      if (wantShot) {
        await page.screenshot({ path: resolve(OUT, `${MODE}-${tag}-top.png`) });
        shots++;
      }
      if (MODE === 'after' && sizeName === 'phone') {
        await page.locator(".arc-card[data-size='hero']").first().scrollIntoViewIfNeeded();
        await page.screenshot({ path: resolve(OUT, `${MODE}-${tag}-cards.png`) });
        shots++;
        await page.evaluate(() => window.scrollTo(0, 0));
      }

      // the pixels behind the text: a second shot with the card bodies hidden, sampled left/mid/right x top/mid/bottom
      // (desktop: the viewport holds both cards; phone: each card is scrolled to and shot by itself)
      const cardsN = await page.locator(".arc-card[data-size='hero']").count();
      const hashes: string[] = [];
      const grids: Record<string, unknown> = {};
      let worst = 99;
      for (let i = 0; i < cardsN; i++) {
        const card = page.locator(".arc-card[data-size='hero']").nth(i);
        await card.scrollIntoViewIfNeeded();
        await page.waitForTimeout(150);
        const items = (await textItems(page)).filter((it) => it.card === `card${i}`);
        hashes.push(
          createHash('sha1')
            .update(await card.screenshot())
            .digest('hex'),
        );
        const sheet = await page.addStyleTag({
          content: `.arc-card[data-size='hero'] .arc-card-body { visibility: hidden !important; }`,
        });
        const png = await page.screenshot();
        await sheet.evaluate((n) => (n as Element).remove());
        for (const it of items) {
          const [x, y, w, h] = it.rect as [number, number, number, number];
          const pts: [number, number][] = [];
          for (const fx of [0.02, 0.5, 0.98])
            for (const fy of [0.15, 0.5, 0.85]) pts.push([x + w * fx, y + h * fy]);
          const px = await pixels(browser, png, pts);
          const text = parse(it.color);
          const cs = px.map((p) => Math.round(contrast(text, p) * 100) / 100);
          const min = Math.min(...cs);
          worst = Math.min(worst, min);
          grids[`${it.card}-${it.name}`] = {
            text: it.color,
            pixels: px.map((p) => p.join(',')),
            contrast: cs,
          };
          log(
            `${tag} ${it.card} ${it.name}: text ${it.color}, samples ${px.map((p) => `rgb(${p.join(',')})`).join(' ')}, contrast ${cs.join(' ')}, min ${min}`,
          );
          if (min < 4.5) failures.push(`${tag} ${it.card} ${it.name} min ${min}`);
        }
        await page.evaluate(() => window.scrollTo(0, 0));
      }
      log(`${tag} worst contrast ${worst}; card hashes ${hashes.join(' ')}`);
      const result = { hero, row, boxes: bx, hashes, grids, worst, sw };
      writeFileSync(resolve(OUT, `data-${MODE}-${tag}.json`), JSON.stringify(result, null, 2));

      if (MODE === 'after') {
        const base = readJson(resolve(OUT, `data-base-${tag}.json`));
        // after: light and dark hero probes equal the Editorial row
        const ed = readJson(resolve(OUT, `data-after-editorial-${sizeName}.json`));
        if (variant !== 'editorial' && ed) {
          expect(hero, `${tag}: hero probes equal the Editorial row`).toEqual(ed.hero);
          expect(worst, `${tag}: worst contrast`).toBeGreaterThanOrEqual(4.5);
        }
        if (base) {
          // the row card is the member's own theme: unchanged in every theme
          expect(row, `${tag}: row card probes equal the base`).toEqual(base.row);
          // no box changed
          expect(bx, `${tag}: no card's box changed`).toEqual(base.boxes);
          if (variant === 'editorial') {
            expect(hero, `${tag}: shipped hero probes equal the base`).toEqual(base.hero);
            expect(hashes, `${tag}: shipped hero pixels equal the base`).toEqual(base.hashes);
            expect(grids, `${tag}: shipped sample grid equals the base`).toEqual(base.grids);
          }
        }
      }
      expect(errors, 'no page errors').toEqual([]);
      await c.close();
    }
  }
  log(`FAILURES(<4.5) ${failures.length ? failures.join('; ') : 'none'}`);

  // ---- the pointer and the keyboard focus on a hero card's Play button, light 1280; then the click-through
  if (MODE === 'after') {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'lounge-light' },
    };
    await setPrefs(prefs);
    for (const [sizeName, size] of Object.entries(SIZES)) {
      const tag = `light-${sizeName}`;
      const c = await context(browser, octx, size, prefs);
      await fake(c);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      await page.goto('/games');
      await expect(page.locator(".arc-card[data-size='hero']")).toHaveCount(2, { timeout: 60_000 });
      await page.waitForTimeout(800);
      const play = page
        .locator(".arc-card[data-size='hero']")
        .first()
        .getByRole('button', { name: /^Play/ });
      await play.scrollIntoViewIfNeeded();
      const css = await play.evaluate((b) => {
        const s = getComputedStyle(b);
        return `${s.backgroundColor} on ${s.color}`;
      });
      log(`${tag} Play button (rest): ${css}`);
      if (sizeName === 'desktop') {
        await play.hover();
        await page.waitForTimeout(300);
        log(
          `${tag} Play button (pointer): ${await play.evaluate((b) => getComputedStyle(b).backgroundColor)}`,
        );
        await page.screenshot({ path: resolve(OUT, `${MODE}-${tag}-play-pointer.png`) });
        await page.mouse.move(0, 0);
        await play.focus();
        await page.keyboard.press('Tab');
        await page.keyboard.press('Shift+Tab');
        await page.waitForTimeout(300);
        log(
          `${tag} Play button (keyboard focus): outline ${await play.evaluate((b) => {
            const s = getComputedStyle(b);
            return `${s.outlineColor} ${s.outlineStyle} ${s.outlineWidth}`;
          })}`,
        );
        await page.screenshot({ path: resolve(OUT, `${MODE}-${tag}-play-focus.png`) });
        shots += 2;
      }
      await play.click();
      await page.waitForTimeout(700);
      log(`${tag} click-through: dialogs after Play = ${await page.getByRole('dialog').count()}`);
      await page.screenshot({ path: resolve(OUT, `${MODE}-${tag}-play-pressed.png`) });
      shots++;
      await page.keyboard.press('Escape');
      await page.waitForTimeout(500);
      log(`${tag} click-through: dialogs after Escape = ${await page.getByRole('dialog').count()}`);
      await c.close();
    }
  }
  log(`SUMMARY shots=${shots}`);
  await setPrefs({
    theme: 'lounge-dark',
    zoom: 100,
    sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'lounge-editorial' },
  });
  await octx.close();
});
