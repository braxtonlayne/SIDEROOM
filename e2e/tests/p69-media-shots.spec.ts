import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * Wave 69, media (SEC-AUD-226; D-709, D-710, D-719): the title editor's help line under "Subtitle languages" and the
 * subtitle panel's example placeholder. The first test is on the real stack: a title is created, `eng, fin` is typed
 * into the field and saved, and `en, fi` comes back. The second fakes a linked video's tracks with `page.route`
 * (a fresh stack has no asset) and types `POR-br` into a track's language. Editorial, Lounge Light and Lounge Dark,
 * desktop 1280x800 and phone 375x812; at 375 px the page has no sideways scroll. Local only (SR_P69_MEDIA=1); shots go
 * to SR_P69_MEDIA_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P69_MEDIA_OUT ?? resolve('test-results/p69-media');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P69_MEDIA !== '1', 'local visual check only');
test.setTimeout(1_200_000);

const json = (r: Route, body: unknown, status = 200) =>
  r.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const pause = (page: Page) => page.waitForTimeout(400);

const noSidewaysScroll = async (page: Page) => {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
};

const newTitle = (name: string) => ({
  kind: 'movie',
  title: name,
  original_title: null,
  alt_titles: [],
  year: 1988,
  runtime_min: 124,
  content_rating: 'R',
  synopsis: 'Neo-Tokyo, thirty-one years after.',
  tagline: null,
  genres: ['Anime'],
  tags: [],
  languages: ['ja'],
  subtitle_languages: ['en'],
  cast: [],
  directors: [],
  writers: [],
  studios: [],
  crew: [],
  age_restricted: false,
  poster_key: null,
  backdrop_key: null,
  logo_key: null,
  external_ids: {},
});

test('the title editor says where subtitle languages come from, and shows the list as saved', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const made = await apiPost<{ id: string }>(
    octx.request,
    '/admin/cinema/titles',
    newTitle(`Akira ${Date.now()}`),
  );
  try {
    for (const theme of THEMES) {
      await apiPatch(octx.request, '/users/@me/settings', {
        settings: { user_prefs: { theme, zoom: 100 } },
      });
      for (const [vp, size] of Object.entries(VIEWPORTS)) {
        const tag = `${vp}-${theme}`;
        const c = await context(browser, octx, size, theme);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        await page.goto(`/admin/cinema-catalog?title=${made.id}`);
        let field = page.getByRole('textbox', { name: /^Subtitle languages/ });
        if (!(await field.isVisible().catch(() => false))) {
          await page.getByRole('button', { name: /^Edit Akira/ }).click();
          field = page.getByRole('textbox', { name: /^Subtitle languages/ });
        }
        await expect(field).toBeVisible();
        await field.scrollIntoViewIfNeeded();
        const hint = page.getByText(/added here automatically/);
        await expect(hint).toBeVisible();
        await expect(hint).toContainText('hide its track');
        // The line is the field's description, so a screen reader reads it with the field.
        await expect(field).toHaveAccessibleDescription(/hide its track/);
        await noSidewaysScroll(page);
        await pause(page);
        await page.screenshot({ path: resolve(OUT, `editor-hint-${tag}.png`) });

        // Type three-letter codes; the server stores the short form and the form shows what was saved.
        await field.fill('eng, fin');
        await page.getByRole('button', { name: 'Save changes' }).click();
        await expect(page.getByRole('textbox', { name: /^Subtitle languages/ })).toHaveValue(
          'en, fi',
        );
        await pause(page);
        await page.screenshot({ path: resolve(OUT, `editor-saved-${tag}.png`) });
        // Put the list back for the next round.
        await page.getByRole('textbox', { name: /^Subtitle languages/ }).fill('en');
        await page.getByRole('button', { name: 'Save changes' }).click();
        await expect(page.getByRole('textbox', { name: /^Subtitle languages/ })).toHaveValue('en');
        await c.close();
      }
    }
  } finally {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
    });
  }
});

test('the subtitle panel shows its example and the language as saved', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const made = await apiPost<{ id: string }>(
    octx.request,
    '/admin/cinema/titles',
    newTitle(`Paprika ${Date.now()}`),
  );
  const track = {
    id: '1',
    label: 'English',
    language: 'en',
    forced: false,
    hearing_impaired: false,
    is_default: true,
    hidden: false,
    source: 'embedded',
    styled: false,
  };
  try {
    for (const theme of THEMES) {
      await apiPatch(octx.request, '/users/@me/settings', {
        settings: { user_prefs: { theme, zoom: 100 } },
      });
      for (const [vp, size] of Object.entries(VIEWPORTS)) {
        const tag = `${vp}-${theme}`;
        const c = await context(browser, octx, size, theme);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        // The title answers with a linked video (asset 777) whose one track is fake; the rest is the real stack's.
        await page.route(new RegExp(`/api/v1/admin/cinema/titles/${made.id}$`), async (r) => {
          const real = await r.fetch();
          const body = (await real.json()) as { title: Record<string, unknown> };
          body.title.asset_id = '777';
          await json(r, body);
        });
        await page.route(/\/api\/v1\/admin\/cinema\/assets\/777\/subtitles$/, (r) =>
          json(r, { tracks: [track] }),
        );
        await page.route(/\/api\/v1\/admin\/cinema\/assets\/777\/subtitles\/1$/, (r) =>
          json(r, { ...track, language: 'pt-BR' }),
        );
        await page.route(/\/api\/v1\/admin\/cinema\/assets\/777(\/[a-z-]+)?(\?.*)?$/, (r) =>
          r.request().url().includes('/subtitles')
            ? r.fallback()
            : json(r, { error: 'not here' }, 404),
        );
        await page.goto(`/admin/cinema-catalog?title=${made.id}`);
        if (
          !(await page
            .getByRole('tab', { name: 'Video and transcoding' })
            .isVisible()
            .catch(() => false))
        ) {
          await page.getByRole('button', { name: /^Edit Paprika/ }).click();
        }
        await page.getByRole('tab', { name: 'Video and transcoding' }).click();
        await page.getByRole('button', { name: /^Show the subtitles of/ }).click();
        const block = page.getByRole('region', { name: 'Subtitles' });
        const language = block.getByLabel('Language of track 1');
        await expect(language).toHaveAttribute('placeholder', 'en, pt-BR');
        await expect(block.getByLabel('Language of the new track')).toHaveAttribute(
          'placeholder',
          'en, pt-BR',
        );
        await block.getByLabel('Language of the new track').scrollIntoViewIfNeeded();
        await noSidewaysScroll(page);
        await pause(page);
        await page.screenshot({ path: resolve(OUT, `panel-${tag}.png`) });
        await language.fill('POR-br');
        await language.press('Enter');
        await expect(language).toHaveValue('pt-BR');
        await pause(page);
        await page.screenshot({ path: resolve(OUT, `panel-saved-${tag}.png`) });
        await c.close();
      }
    }
  } finally {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
    });
  }
});
