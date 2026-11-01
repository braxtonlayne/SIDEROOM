import { expect } from '@playwright/test';
import { test } from '../support/world';

/**
 * ARC-ROOM-001: the Arcade room in a real browser. The Front Page is served by a route handler (the API contract is
 * covered by the Rust suites), so the case checks what the page does with it: Featured cards with their tier and
 * capability flags, a tip badge that opens the creator page only on an allow-listed host, a Bitcoin QR, the
 * Coming Soon "Notify me" toggle (it PUTs the choice), and that `/games/emulator` is still the emulator.
 * Written for the wave-42 integrator, who owns the Playwright ports; not run by the author.
 */

const flags = [
  { name: 'datachannel', status: 'active', summary: 'Shared state through the call.' },
  { name: 'guest_play', status: 'reserved', summary: 'Guest links.' },
];
const t1 = {
  browser_play: true,
  guest_links: true,
  featured: true,
  desktop_only: false,
  needs_base_game_hint: false,
};
const t2 = { ...t1, browser_play: false, guest_links: false, desktop_only: true };

const front = {
  featured: [
    {
      application_id: '900',
      name: 'Land.io',
      description: 'A roguelike for a call.',
      blurb: 'Our flagship',
      tier: 'TIER_1_ORIGINAL',
      eligibility: t1,
      capabilities: ['datachannel', 'guest_play'],
      tip_links: [
        { kind: 'kofi', url: 'https://ko-fi.com/landio' },
        { kind: 'btc', url: 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq' },
        { kind: 'patreon', url: 'https://evil.example/not-allowed' },
      ],
      bundle_sha256: 'a'.repeat(64),
      position: 0,
      featured_at_ms: 1,
    },
    {
      application_id: '901',
      name: 'Horizon',
      description: 'A clean-room engine.',
      blurb: null,
      tier: 'TIER_2_REIMPLEMENTATION',
      eligibility: t2,
      capabilities: [],
      tip_links: [],
      bundle_sha256: 'b'.repeat(64),
      position: 1,
      featured_at_ms: 2,
    },
  ],
  coming_soon: [
    {
      id: '55',
      title: 'OpenMW',
      summary: 'An open engine in the works.',
      tier: 'TIER_2_REIMPLEMENTATION',
      repo_url: 'https://github.com/OpenMW/openmw',
      notify: false,
      created_at_ms: 3,
    },
  ],
  flags,
};

test('the Arcade shows Featured, tips and Coming Soon, and the emulator moved to /games/emulator', async ({
  solo,
}) => {
  const page = solo.a.page;
  await page.route('**/api/v1/arcade', (route) => route.fulfill({ json: front }));
  const puts: unknown[] = [];
  await page.route('**/api/v1/arcade/coming-soon/55/notify', (route) => {
    puts.push(route.request().postDataJSON());
    return route.fulfill({ status: 204 });
  });

  await page.goto('/games');
  await expect(page.getByRole('heading', { name: 'The Arcade', level: 1 })).toBeVisible();

  const land = page.getByRole('article').filter({ hasText: 'Land.io' });
  await expect(land.getByText('Original')).toBeVisible();
  await expect(land.getByText('guest_play')).toBeVisible();
  await expect(land.getByRole('link', { name: 'Ko-fi' })).toHaveAttribute(
    'href',
    'https://ko-fi.com/landio',
  );
  // A tip host off the allow-list is never drawn.
  await expect(land.getByRole('link', { name: 'Patreon' })).toHaveCount(0);
  await land.getByRole('button', { name: 'Bitcoin' }).click();
  await expect(land.getByRole('img', { name: /Bitcoin QR code/ })).toBeVisible();

  const horizon = page.getByRole('article').filter({ hasText: 'Horizon' });
  await expect(horizon.getByText('Desktop app only')).toBeVisible();

  const soon = page.getByRole('article').filter({ hasText: 'OpenMW' });
  await expect(soon.getByRole('link', { name: 'Repository' })).toHaveAttribute(
    'href',
    'https://github.com/OpenMW/openmw',
  );
  await soon.getByRole('button', { name: 'Notify me' }).click();
  await expect.poll(() => puts.length).toBe(1);
  expect(puts[0]).toEqual({ notify: true });

  await page.route('**/api/v1/users/@me/games/roms', (route) =>
    route.fulfill({ json: { roms: [] } }),
  );
  await page.route('**/api/v1/users/@me/storage/mounts', (route) => route.fulfill({ json: [] }));
  await page.goto('/games/emulator');
  await expect(page.getByText('No games yet', { exact: true })).toBeVisible();
});
