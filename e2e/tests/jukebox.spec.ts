import { execFileSync } from 'node:child_process';
import { expect, test, type Page } from '@playwright/test';
import { PUBLIC_URL } from '../support/cli';
import { loginApi, type SeededUser } from '../support/fixtures';
import { serveTone } from '../support/playerAudio';
import { voiceServer } from '../support/voice';
import { readFileSync } from 'node:fs';
import { STATE_FILE, type BootstrapState } from '../global-setup';

/**
 * MUS-JB-002: synced local playback. One member opens the same voice channel in two browser contexts (two
 * listeners), queues a track from their library, and both contexts must hold the same position, within 250 ms,
 * after five seconds. The audio is the generated tone of the player specs: the presign answer is replaced in
 * the browser, so no bucket is needed, but the library rows are real (psql seeds a mount, a file row and an
 * index row for the member; the server still decides who may queue and who is `playable`).
 *
 * Needs the stack of the other specs plus `psql` and SR_TEST_DATABASE_URL (the seeding). Ports 8081/5174 are
 * jb-queue's in wave 31 (SR_E2E_BASE_URL / SR_SERVER__PUBLIC_URL as in docs/dev/e2e). Not run in the wave
 * (no database password on the build machine): CI runs it.
 */
const DB = process.env.SR_TEST_DATABASE_URL ?? '';
const psql = (q: string) =>
  execFileSync('psql', [DB, '-X', '-A', '-t', '-c', q], { encoding: 'utf8' }).trim();
const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

test.describe('Voice jukebox', () => {
  test.skip(!DB, 'Needs SR_TEST_DATABASE_URL (and psql) to seed the member library.');

  async function listener(
    browser: import('@playwright/test').Browser,
    user: SeededUser,
    base: { guildId: string; textChannelId: string; voiceName: string },
  ): Promise<Page> {
    const context = await browser.newContext({ baseURL: PUBLIC_URL });
    await loginApi(context.request, user);
    const page = await context.newPage();
    await serveTone(page);
    // The bucket is not real: hand out the tone for whatever the server decided is playable.
    await page.route('**/api/v1/channels/*/jukebox/presign', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ url: `${PUBLIC_URL}/e2e-tone-jukebox.wav`, expires_at: '' }),
      }),
    );
    await page.goto(`/channels/${base.guildId}/${base.textChannelId}`);
    await page
      .getByTestId('channel-sidebar')
      .getByRole('button', { name: new RegExp(base.voiceName) })
      .first()
      .click();
    return page;
  }

  const audioPosition = (page: Page) =>
    page.evaluate(() => {
      const a = document.querySelector<HTMLAudioElement>('audio[data-testid="jukebox-audio"]');
      return a ? a.currentTime * 1000 : -1;
    });

  test('two listeners hold the same position within 250 ms after five seconds', async ({
    browser,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Autoplay and media timing are asserted on Chromium.');
    const { owner } = state();
    const seedCtx = await browser.newContext({ baseURL: PUBLIC_URL });
    await loginApi(seedCtx.request, owner);
    const me = await (await seedCtx.request.get('/api/v1/users/@me')).json();
    const base = await voiceServer(seedCtx, [], 'Jukebox Room');

    // The member's own bucket with one 40 s track (rows only; the bytes are the tone served above).
    const stamp = Date.now() % 1_000_000_000;
    const mount = stamp * 10 + 1;
    const object = stamp * 10 + 2;
    const track = stamp * 10 + 3;
    psql(
      `INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc, cors_ok_at) ` +
        `VALUES (${mount}, 'user', ${me.id}, 'Jukebox test', 's3', 'http://127.0.0.1:1', 'b', 'k', '\\x00', now())`,
    );
    psql(
      `INSERT INTO storage_objects (id, mount_id, key, kind, class, container, web_native) ` +
        `VALUES (${object}, ${mount}, 'music/tone.wav', 'music', 'track', 'wav', true)`,
    );
    psql(
      `INSERT INTO music_tracks (id, mount_id, mount_object_id, title, artist, duration_ms) ` +
        `VALUES (${track}, ${mount}, ${object}, 'Test tone', 'Generator', 40000)`,
    );

    const a = await listener(browser, owner, base);
    const b = await listener(browser, owner, base);
    for (const p of [a, b]) {
      await p.getByRole('button', { name: 'Listen along' }).click();
    }
    // Queue the track from A's library picker.
    await a.getByRole('button', { name: 'Add tracks' }).click();
    await a.getByRole('button', { name: 'Queue Test tone' }).click();
    for (const p of [a, b]) {
      await expect(p.getByTestId('jukebox-status')).toHaveText('In sync', { timeout: 20_000 });
    }

    await a.waitForTimeout(5_000);
    const [pa, pb] = await Promise.all([audioPosition(a), audioPosition(b)]);
    expect(pa).toBeGreaterThan(4_000);
    expect(pb).toBeGreaterThan(4_000);
    expect(Math.abs(pa - pb)).toBeLessThan(250);

    // Cleanup of the seeded rows (the queue goes with the server).
    psql(`DELETE FROM storage_mounts WHERE id = ${mount}`);
  });
});
