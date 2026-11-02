import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { repoRoot } from '../support/cli';

/**
 * Polish round (XC-VIS-002, media): click through the media flows like a member would, against the sweep world.
 * Opt-in like the sweep (SR_E2E_SWEEP=1); screenshots go to SR_SWEEP_OUT. It creates one Cinema draft (and deletes
 * it again when the page offers that); everything else only opens and closes things.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OUT = process.env.SR_SWEEP_OUT ?? resolve(repoRoot, 'e2e', 'test-results', 'polish-media');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;

test.skip(!ON, 'Opt-in: set SR_E2E_SWEEP=1.');

const world = () =>
  JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as {
    titleId: string;
    tvChannelId?: string;
  };

async function open(browser: import('@playwright/test').Browser, phone = false) {
  const context = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? { width: 375, height: 812 } : { width: 1280, height: 800 },
    colorScheme: 'dark',
    storageState: resolve(STATE, 'owner.json'),
  });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
  page.on('response', (r) => {
    if (r.status() >= 500) errors.push(`${r.status()} ${r.request().method()} ${r.url()}`);
  });
  return { context, page, errors };
}

const shot = (page: Page, name: string) =>
  page.screenshot({ path: resolve(OUT, `flow-${name}.png`), fullPage: true });

test('browse, filter and open a title, then the player and Watch together', async ({ browser }) => {
  const { context, page, errors } = await open(browser);
  const w = world();
  await page.goto('/cinema/movies');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await page.getByRole('button', { name: 'Drama', exact: true }).first().click();
  await expect(page).toHaveURL(/genres=/);
  await shot(page, 'movies-drama');
  await page.getByRole('button', { name: 'All', exact: true }).first().click();
  await expect(page).not.toHaveURL(/genres=/);

  await page.goto(`/cinema/t/${w.titleId}`);
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  for (const tab of ['Reviews', 'Comments', 'More like this']) {
    const t = page.getByRole('tab', { name: tab });
    if (await t.count()) {
      await t.click();
      await shot(page, `title-tab-${tab.replace(/\s+/g, '-').toLowerCase()}`);
    }
  }
  await page.getByRole('button', { name: 'Watch together' }).first().click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await page.waitForTimeout(500);
  await shot(page, 'watch-together-dialog');
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();

  await page
    .getByRole('link', { name: /^Play/ })
    .or(page.getByRole('button', { name: /^Play/ }))
    .first()
    .click();
  await expect(page).toHaveURL(/\/cinema\/watch\//);
  await page.waitForTimeout(1500);
  await shot(page, 'player');
  expect(errors).toEqual([]);
  await context.close();
});

test('submit a title draft', async ({ browser }) => {
  const { context, page, errors } = await open(browser);
  const name = `Polish draft ${Date.now().toString(36)}`;
  await page.goto('/cinema/submit');
  await page.getByRole('button', { name: 'New submission' }).click();
  const form = page.getByRole('form', { name: 'New submission' });
  await expect(form).toBeVisible();
  await shot(page, 'submit-form');
  await form.getByRole('textbox', { name: 'Title' }).fill(name);
  await form.getByRole('button', { name: 'Start the draft' }).click();
  await expect(page.getByRole('heading', { name })).toBeVisible();
  await shot(page, 'submit-draft');
  const del = page.getByRole('button', { name: /Delete|Discard|Withdraw/ }).first();
  if (await del.count()) {
    await del.click();
    const confirm = page
      .getByRole('dialog')
      .getByRole('button', { name: /Delete|Discard|Withdraw/ });
    if (await confirm.count()) await confirm.first().click();
    await page.waitForTimeout(800);
    await shot(page, 'submit-after-delete');
  }
  expect(errors).toEqual([]);
  await context.close();
});

test('TV guide and live channel, desktop and phone', async ({ browser }) => {
  for (const phone of [false, true]) {
    const { context, page, errors } = await open(browser, phone);
    await page.goto('/cinema/tv');
    await page.waitForTimeout(1200);
    await shot(page, `tv-${phone ? 'phone' : 'desktop'}`);
    const now = page.getByRole('button', { name: 'Now', exact: true });
    if (await now.count()) await now.first().click();
    expect(errors).toEqual([]);
    await context.close();
  }
});

test('library: browse, filter by format, shelves', async ({ browser }) => {
  const { context, page, errors } = await open(browser);
  await page.goto('/library/browse');
  await page.getByRole('button', { name: 'Manga', exact: true }).first().click();
  await page.waitForTimeout(600);
  await shot(page, 'library-browse-manga');
  await page.goto('/library/submissions');
  await page.waitForTimeout(600);
  await shot(page, 'library-submissions');
  expect(errors).toEqual([]);
  await context.close();
});

test('cinema shelves: list, filter, item details (faked shelf)', async ({ browser }) => {
  for (const phone of [false, true]) {
    const { context, page, errors } = await open(browser, phone);
    const src = { mount_id: '9501', label: 'Paper Boat Cinema', scope: 'guild' };
    const item = (id: string, kind: string, title: string, parts: number) => ({
      id,
      section: 'cinema',
      kind,
      title,
      subtitle: kind === 'movie' ? '' : `${parts} episodes`,
      year: 1999,
      source: src,
      tags: [],
      genres: ['Drama'],
      synopsis: 'A quiet story about a lighthouse keeper and the town that forgot him.',
      external_ids: {},
      overridden: [],
      parts,
      object_key: kind === 'movie' ? `Movies/${title}.mkv` : undefined,
      container: 'mkv',
      web_native: false,
    });
    const items = [
      item('s1', 'movie', 'The Quiet Lighthouse', 0),
      item('s2', 'series', 'Harbor Lights', 3),
      item('s3', 'anime', 'Paper Moons', 2),
    ];
    await page.route(/\/api\/v1\/shelves\/cinema(\?.*)?$/, (r) => {
      const kind = new URL(r.request().url()).searchParams.get('kind');
      const list = items.filter((i) => !kind || i.kind === kind);
      return r.fulfill({ json: { items: list, total: list.length, sources: [src] } });
    });
    await page.route(/\/api\/v1\/shelves\/cinema\/s2$/, (r) =>
      r.fulfill({
        json: {
          item: items[1],
          can_edit: true,
          parts: [1, 2, 3].map((n) => ({
            id: `p${n}`,
            season: 1,
            number: n,
            title: `Episode ${n}`,
            object_key: `Series/Harbor Lights/S01E0${n}.mkv`,
            container: 'mkv',
            web_native: n !== 3,
            size: 734_003_200,
          })),
        },
      }),
    );
    const v = phone ? 'phone' : 'desktop';
    await page.goto('/cinema/shelves');
    await expect(page.getByText('Harbor Lights').first()).toBeVisible();
    await shot(page, `shelves-list-${v}`);
    await page.getByRole('button', { name: 'Series', exact: true }).last().click();
    await page.waitForTimeout(500);
    await shot(page, `shelves-series-${v}`);
    await page.getByRole('link', { name: 'Harbor Lights' }).first().click();
    await expect(page).toHaveURL(/\/shelves\/s2/);
    await page.waitForTimeout(800);
    await shot(page, `shelves-item-${v}`);
    expect(errors).toEqual([]);
    await context.close();
  }
});
