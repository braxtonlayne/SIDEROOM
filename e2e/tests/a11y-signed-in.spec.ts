import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { test, expect } from '../support/fixtures';
import { blocking, describe, runAxe, settle, type Finding } from '../support/a11y';
import { buildWorld, type World } from '../support/world';

/**
 * RDY-A11Y-001: axe in a real browser over the signed-in routes, on a fixture world built through the API
 * (support/world.ts). Colour contrast and landmarks are included (jsdom cannot check them). Serious and
 * critical findings fail the test; moderate and minor ones are attached to the report and collected in
 * `test-results/a11y-moderate.json` (the handoff lists them with an owner per file).
 *
 * Runs on Chromium, Firefox and WebKit in the `playwright` CI job like every spec in this folder.
 */
test.use({ bypassCSP: true });

interface Row {
  name: string;
  path: (w: World) => string;
  /** Role or text that proves the page rendered. */
  ready: (page: import('@playwright/test').Page) => import('@playwright/test').Locator;
  /** When false the row is only audited if the data exists (Library needs object storage). */
  needs?: 'library';
}

const SETTINGS = [
  'account',
  'profile',
  'privacy',
  'activity',
  'apps',
  'modules',
  'devices',
  'connections',
  'invites',
  'support',
  'appearance',
  'accessibility',
  'text',
  'notifications',
  'voice',
  'keybinds',
  'language',
  'streamer',
  'advanced',
];

const ROWS: Row[] = [
  { name: 'lounge channel', path: (w) => `/channels/${w.guildId}/${w.channelId}`, ready: (p) => p.getByRole('log').first() },
  { name: 'lounge thread', path: (w) => `/channels/${w.guildId}/${w.channelId}/threads/${w.threadId}`, ready: (p) => p.getByRole('main') },
  { name: 'lounge home', path: () => '/channels/@me', ready: (p) => p.getByRole('main') },
  ...SETTINGS.map((s): Row => ({ name: `settings ${s}`, path: () => `/settings/user/${s}`, ready: (p) => p.getByRole('main') })),
  { name: 'server settings', path: (w) => `/settings/server/${w.guildId}`, ready: (p) => p.getByRole('main') },
  { name: 'library home', path: () => '/library', ready: (p) => p.getByRole('main') },
  { name: 'library browse', path: () => '/library/browse', ready: (p) => p.getByRole('main') },
  { name: 'cinema home', path: () => '/cinema', ready: (p) => p.getByRole('main') },
  { name: 'cinema movies', path: () => '/cinema/movies', ready: (p) => p.getByRole('main') },
  { name: 'cinema title', path: (w) => `/cinema/t/${w.titleId}`, ready: (p) => p.getByRole('main') },
  { name: 'cinema player', path: (w) => `/cinema/watch/${w.titleId}`, ready: (p) => p.locator('body') },
  { name: 'cinema watch party', path: (w) => `/cinema/party/${w.partyId}`, ready: (p) => p.getByRole('tab', { name: 'Chat' }) },
  { name: 'cinema tv guide', path: () => '/cinema/tv', ready: (p) => p.getByRole('main') },
  { name: 'cinema my list', path: () => '/cinema/my-list', ready: (p) => p.getByRole('main') },
  { name: 'admin home', path: () => '/admin', ready: (p) => p.getByRole('main') },
  { name: 'developers', path: () => '/developers', ready: (p) => p.getByRole('main') },
  { name: 'developers docs', path: () => '/developers/docs/getting-started', ready: (p) => p.getByRole('main') },
  { name: 'support', path: () => '/support', ready: (p) => p.getByRole('main') },
];

const OUT = '.state/a11y';
const WORLD_FILE = `${OUT}/world.json`;
const OWNER_STATE = `${OUT}/owner.json`;
const slug = (n: string) => n.replace(/[^a-z0-9]+/gi, '-').toLowerCase();
// The worker restarts after a failed test, so the world lives on disk, not in module state.
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

test('build the fixture world', async ({ scenario }) => {
  const s = await scenario.ownerAndMember();
  const world = await buildWorld(s);
  mkdirSync(OUT, { recursive: true });
  writeFileSync(WORLD_FILE, JSON.stringify(world));
  await s.a.context.storageState({ path: OWNER_STATE });
});

test.describe('axe on signed-in routes', () => {
  test.use({ storageState: OWNER_STATE });
  for (const row of ROWS) {
    test(row.name, async ({ page }) => {
      const world = loadWorld();
      test.skip(row.name.includes('watch party') && !world.partyId, 'no watch party in this world');
      await page.goto(row.path(world));
      await expect(row.ready(page).first()).toBeVisible({ timeout: 20_000 });
      await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
      await settle(page);
      const found = await runAxe(page);
      const rest = found.filter((v) => v.impact !== 'serious' && v.impact !== 'critical');
      writeFileSync(`${OUT}/${slug(row.name)}.json`, JSON.stringify({ route: row.name, found }, null, 1));
      if (rest.length) {
        test.info().annotations.push({ type: 'moderate', description: describe(rest).join(String.fromCharCode(10)) });
      }
      expect(describe(blocking(found)), row.name).toEqual([]);
    });
  }
});
