import { expect, type Page, type Route } from '@playwright/test';
import { test } from '../support/fixtures';

/**
 * Wave 51 visual check (AGENTS.md section 4), off unless SR_P51_SHOTS=<folder>: the shelf picker with a search in the
 * profile showcase editor, and the watch party page after the member lapsed (the Rejoin button) and came back.
 * The shelf and the party are played by routes in the page (the e2e stack has no storage mount or Cinema title).
 */
const SHOTS = process.env.SR_P51_SHOTS ?? '';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

const hash = (n: number) => n.toString(16).padStart(64, '0');
const FILES = [
  ['Music/Autumn Leaves.mp3', 'Autumn Leaves', 4_200_000],
  ['Music/Blue in Green.flac', '', 21_000_000],
  ['Music/Cool Jazz Mix.mp3', 'Cool Jazz Mix (long set)', 88_000_000],
  ['Films/Autumn trailer.mp4', '', 52_000_000],
  ['Films/Night shift.mkv', 'Night shift', 700_000_000],
  ['Podcasts/Episode 12.mp3', '', 31_000_000],
].map(([path, title, size], i) => ({
  object_hash: hash(i + 1),
  path,
  title,
  size,
  note: '',
  cover: '',
  updated_ms: 1,
}));

async function playShelf(page: Page) {
  await page.route(/\/api\/v1\/shelves\/visible$/, (r: Route) =>
    r.fulfill({
      json: {
        shelves: [
          {
            shelf_id: '55',
            label: 'Home bucket',
            via: 'own',
            owner_id: '1',
            owner_handle: 'owner',
            guild_id: null,
            guild_name: null,
            object_count: FILES.length,
            can_manage: true,
          },
        ],
      },
    }),
  );
  await page.route(/\/api\/v1\/shelves\/55\/objects(\?.*)?$/, (r: Route) => {
    const q = new URL(r.request().url()).searchParams.get('q')?.toLowerCase() ?? '';
    const objects = FILES.filter(
      (f) => !q || f.path.toLowerCase().includes(q) || f.title.toLowerCase().includes(q),
    );
    void r.fulfill({ json: { objects, next_cursor: null, can_edit: true } });
  });
}

for (const theme of THEMES) {
  for (const [vp, size] of Object.entries(VIEWPORTS)) {
    test(`p51 picker search in the showcase editor: ${theme} ${vp}`, async ({
      browser,
      owner,
      signedIn,
    }) => {
      test.skip(!SHOTS, 'SR_P51_SHOTS=<folder> turns the visual check on.');
      const { page, context } = await signedIn(owner);
      await context.addInitScript((t) => {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme: t } }, version: 2 }),
        );
      }, theme);
      void browser;
      await page.setViewportSize(size);
      await playShelf(page);
      await page.goto('/profile-page/edit');
      await page.getByRole('combobox').first().click();
      await page.getByRole('option', { name: 'Showcase' }).click();
      await page.getByRole('button', { name: 'Add block', exact: true }).click();
      await page.getByRole('button', { name: 'Add from my shelf' }).click();
      const dialog = page.getByRole('dialog');
      await expect(dialog.getByText('Autumn Leaves').first()).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/${theme}-${vp}-1-picker.png` });
      await dialog.getByRole('textbox', { name: 'Search files' }).fill('autumn');
      await expect(dialog.getByText('Cool Jazz Mix (long set)')).toHaveCount(0);
      await page.screenshot({ path: `${SHOTS}/${theme}-${vp}-2-search.png` });
      await dialog.getByRole('textbox', { name: 'Search files' }).fill('zzzz');
      await expect(dialog.getByText('No file matches that search.')).toBeVisible();
      await page.screenshot({ path: `${SHOTS}/${theme}-${vp}-3-nomatch.png` });
    });
  }
}

for (const theme of THEMES) {
  for (const [vp, size] of Object.entries(VIEWPORTS)) {
    test(`p51 party page after a lapse and a rejoin: ${theme} ${vp}`, async ({
      owner,
      signedIn,
    }) => {
      test.skip(!SHOTS, 'SR_P51_SHOTS=<folder> turns the visual check on.');
      test.setTimeout(90_000);
      const { page, context } = await signedIn(owner);
      await context.addInitScript((t) => {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme: t } }, version: 2 }),
        );
      }, theme);
      await page.setViewportSize(size);
      const me = ((await (await context.request.get('/api/v1/users/@me')).json()) as { id: string })
        .id;
      const now = () => Date.now();
      const playback = () => ({
        version: 1,
        playing: false,
        position_ms: 0,
        rate: 1,
        at_ms: now(),
        server_time_ms: now(),
        expected_position_ms: 0,
        auto_paused: false,
        duration_ms: null,
        live: false,
      });
      const party = (member: boolean) => ({
        id: '900',
        host_id: me,
        binding: 'private',
        guild_id: null,
        channel_id: null,
        invite_code: 'abc',
        media: null,
        playback: playback(),
        settings: {
          shared_control: 'host',
          queue_policy: 'controllers',
          auto_pause_buffering: true,
          max_wait_ms: 10_000,
          persistent: false,
          max_participants: 50,
        },
        participants: member
          ? [
              {
                user_id: me,
                username: 'owner',
                display_name: null,
                joined_at_ms: 1,
                is_host: true,
                is_controller: false,
                ready: false,
                buffering: false,
                rtt_ms: null,
              },
            ]
          : [],
        queue: [],
        created_at_ms: now(),
        ended: false,
        you: { member, host: member, can_control: member, can_queue: member, can_manage: member },
      });
      let lapsed = false;
      await page.route(/\/api\/v1\/watch-parties\/time$/, (r) =>
        r.fulfill({ json: { server_time_ms: now() } }),
      );
      await page.route(/\/api\/v1\/watch-parties\/900$/, (r) =>
        r.fulfill({ json: party(!lapsed) }),
      );
      await page.route(/\/api\/v1\/watch-parties\/900\/heartbeat$/, (r) =>
        r.fulfill({ json: { server_time_ms: now(), playback: playback(), member: !lapsed } }),
      );
      await page.route(/\/api\/v1\/watch-parties\/900\/join$/, (r) => {
        lapsed = false;
        void r.fulfill({ json: party(true) });
      });
      await page.goto('/cinema/party/900');
      await expect(page.locator('.wp-root').first()).toBeVisible();
      await page.waitForTimeout(1500);
      await page.screenshot({ path: `${SHOTS}/party-${theme}-${vp}-1-in.png` });
      lapsed = true;
      const rejoin = page.getByRole('button', { name: 'Rejoin the party' });
      await expect(rejoin).toBeVisible({ timeout: 40_000 });
      await page.screenshot({ path: `${SHOTS}/party-${theme}-${vp}-2-lapsed.png` });
      await rejoin.click();
      await expect(rejoin).toHaveCount(0);
      await page.waitForTimeout(1500);
      await page.screenshot({ path: `${SHOTS}/party-${theme}-${vp}-3-rejoined.png` });
    });
  }
}
