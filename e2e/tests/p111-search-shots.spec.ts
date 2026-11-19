import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPost, expect, test } from '../support/fixtures';

/**
 * Wave 111, search (OPS-DEP-001 / DEP-FU-001, D-1037): tantivy 0.27 opens the search folder that tantivy 0.26.2
 * wrote, read AND write, with no `search.reindex` and no admin action.
 *  (a) the channel search finds a word of a SEEDED message: that message sits in the index the cell's template
 *      wrote with 0.26.2 (the new server read it);
 *  (b) a message posted now (a word nobody uses, `p111s<stamp>`) is indexed by the new code into that same old
 *      index and is found (the old index, written by 0.27);
 *  (c) the Cinema search finds the seeded title (a 0.26.2 index of its own); the Library search answers 200 (the
 *      sweep world holds no Library series, so its index is empty: the spec says so and does not invent one).
 *  (d) no call to `search.reindex`, nothing under `/admin` is called.
 * Own data (D-942): one message in the seeded channel, which stays (a message cannot leave the index by this spec's
 * means; it is a made-up word and the world is dropped after the job). NEEDS THE SWEEP'S SEEDED WORLD
 * (`e2e/.state/sweep/`, given fresh by the job; never seeded or edited here).
 *
 * Switches: SR_P111_SEARCH_ON=1, SR_P111_SEARCH_OUT=<folder of the shots and figures-after.json>. One phase only.
 * The commander's command (the server binary is the new one, built in p111-search-bin):
 *   bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit search shoot base --bin C:/Users/braxt/Desktop/SIDEROOM_wt/p111-search-bin --env SR_P111_SEARCH_ON=1 --env SR_P111_SEARCH_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p111-search -- tests/p111-search-shots.spec.ts
 * For the integration tree: the same with `--wt lanes` and `--bin` the integration tree's binary directory.
 */
const ON = process.env.SR_P111_SEARCH_ON === '1';
const OUT =
  process.env.SR_P111_SEARCH_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p111-search';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const STAMP = Date.now().toString(36);
const WORD = `p111s${STAMP}`;
/** A word of the first seeded message ("Welcome in! **Bold** ..."), written to the index with 0.26.2. */
const SEEDED_WORD = 'Welcome';

test.skip(!ON, 'local visual check only');
test.setTimeout(300_000);

const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as {
    guildId: string;
    channelId: string;
    titleId: string;
  };

const say = (s: string) => console.log(`REPORT ${s}`);

/** A GET through the API with the session cookie; throws with the body on a non-2xx. */
async function apiGetJson<T>(request: APIRequestContext, path: string): Promise<T> {
  const res = await request.get(`/api/v1${path}`);
  if (!res.ok()) throw new Error(`GET ${path} -> ${res.status()}: ${await res.text()}`);
  return (await res.json()) as T;
}

/** The channel search through the API: the number of hits for a word (a few tries; the index reader reloads). */
async function channelHits(
  request: APIRequestContext,
  channelId: string,
  word: string,
): Promise<number> {
  let total = 0;
  for (let i = 0; i < 12; i++) {
    const r = await apiGetJson<{ total_results: number }>(
      request,
      `/channels/${channelId}/messages/search?content=${encodeURIComponent(word)}`,
    );
    total = r.total_results;
    if (total > 0) break;
    await new Promise((ok) => setTimeout(ok, 1000));
  }
  return total;
}

/** Type the word into the channel search box, as a member does, and wait for the results panel. */
async function uiSearch(page: Page, word: string) {
  const sb = page.getByRole('searchbox').first();
  await sb.waitFor({ timeout: 40_000 });
  const panel = page.getByRole('region', { name: 'Search results' });
  for (let i = 0; i < 8; i++) {
    await sb.fill(word);
    await sb.press('Enter');
    await panel.waitFor({ timeout: 25_000 });
    if (await panel.locator('.lng-search-hit').count()) break;
    await page.waitForTimeout(1000);
  }
  await page.waitForTimeout(700);
  return panel;
}

test('p111 search: the tantivy 0.27 server reads and writes the 0.26.2 index, no reindex', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const w = world();
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const figs: Record<string, unknown> = { word: WORD, seededWord: SEEDED_WORD };
  try {
    const r = owner.request;
    const url = `/channels/${w.guildId}/${w.channelId}`;
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width: 1280, height: 800 },
      storageState: await owner.storageState(),
    });
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    try {
      // (a) the old index, read by the new code.
      const seeded = await channelHits(r, w.channelId, SEEDED_WORD);
      figs.seededHits = seeded;
      if (seeded < 1)
        problems.push(`(a) the seeded word "${SEEDED_WORD}" is not found (${seeded})`);
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
      let panel = await uiSearch(page, SEEDED_WORD);
      figs.seededPanelHits = await panel.locator('.lng-search-hit').count();
      if (!figs.seededPanelHits)
        problems.push('(a) the results panel shows no hit for the seeded word');
      await page.screenshot({ path: resolve(OUT, 'channel-seeded-word.png') });

      // (b) the old index, written by the new code: a new message, found.
      for (let tries = 0; ; tries++) {
        try {
          await apiPost(r, `/channels/${w.channelId}/messages`, {
            content: `${WORD} written by 0.27`,
          });
          break;
        } catch (e) {
          if (tries > 2 || !String(e).includes('429')) throw e;
          await new Promise((ok) => setTimeout(ok, 5000));
        }
      }
      const fresh = await channelHits(r, w.channelId, WORD);
      figs.freshHits = fresh;
      if (fresh < 1) problems.push(`(b) the new word "${WORD}" is not found (${fresh})`);
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
      panel = await uiSearch(page, WORD);
      figs.freshPanelHits = await panel.locator('.lng-search-hit').count();
      if (!figs.freshPanelHits)
        problems.push('(b) the results panel shows no hit for the new word');
      await page.screenshot({ path: resolve(OUT, 'channel-new-word.png') });

      // (c) Cinema: the seeded title, found by the first word of its name.
      const t = await apiGetJson<{ title: { title: string } }>(r, `/cinema/titles/${w.titleId}`);
      const name = t.title.title;
      const first = name.split(/\s+/)[0] ?? name;
      figs.cinemaTitle = name;
      const cin = await apiGetJson<{ titles?: { id: string }[] }>(
        r,
        `/cinema/search?q=${encodeURIComponent(first)}`,
      );
      figs.cinemaHits = cin.titles?.length ?? 0;
      if (!cin.titles?.some((x) => x.id === w.titleId))
        problems.push(`(c) the Cinema search for "${first}" does not return the seeded title`);
      await page.goto(`/cinema/search?q=${encodeURIComponent(first)}`, {
        waitUntil: 'domcontentloaded',
        timeout: 30_000,
      });
      await page.getByRole('main').waitFor({ timeout: 30_000 });
      await page.waitForTimeout(2500);
      await page.screenshot({ path: resolve(OUT, 'cinema-search.png') });

      // (c) Library: the sweep world has no series; the route answers and the index (empty) opens.
      const lib = await r.get(`/api/v1/library/search?q=${encodeURIComponent('a')}`);
      figs.libraryStatus = lib.status();
      if (!lib.ok()) problems.push(`(c) the Library search answered ${lib.status()}`);
      else figs.libraryBody = await lib.json();
      await page.goto('/library/browse', { waitUntil: 'domcontentloaded', timeout: 30_000 });
      await page.getByRole('main').waitFor({ timeout: 30_000 });
      await page.waitForTimeout(2000);
      await page.screenshot({ path: resolve(OUT, 'library-browse.png') });
    } finally {
      await ctx.close();
    }
  } finally {
    await owner.close();
    figs.problems = problems;
    writeFileSync(resolve(OUT, 'figures-after.json'), JSON.stringify(figs, null, 1));
    for (const p of problems) say(`PROBLEM ${p}`);
    say(`figures ${JSON.stringify(figs)}`);
  }
  expect(problems, problems.join('\n')).toEqual([]);
});
