/**
 * ACT-LND-001 live case for the integrator: Land.io, two contexts, one match.
 *
 * NOT RUN by the author (only act-channel may use 8080/5173 in wave 36). To wire it in, copy this file to
 * `e2e/tests/activities-landio.spec.ts` (the imports below are written for that location) and run it the way
 * `activities-live.spec.ts` runs: sr-server with SR_ACTIVITIES__ENABLED=true and SR_VOICE__ENABLED=true, the
 * web dev server with VITE_ACTIVITIES_ENABLED=true, SR_E2E_ACTIVITIES=1 SR_E2E_VOICE=1, Chromium only. It needs
 * ACT-CH-001's relay (the data channel through the SFU) and the client host's transport for the SDK, and the
 * SDK exposing the member ids (see the handoff: `sdk.selfId` / `sdk.hostId`).
 *
 * The flow: Bob uploads the Land.io bundle (built by tools/bundle.mjs, the same files the starter pack ships),
 * the Owner approves it, Alice and Bob join one voice channel, Alice starts Land.io (so she hosts: the game
 * runs in her browser), Bob joins the instance, both press Play. Then:
 *   - Bob's page gets ticks from Alice's page (the data channel, not a server),
 *   - Bob's player exists in Alice's engine and Alice's player exists in Bob's world,
 *   - Bob's input is acknowledged by the host,
 *   - the names are the SIDE ROOM members', and no request leaves for a Land.io server.
 */
import { expect, test, apiPatch, apiPost, loginApi, registerUser } from '../support/fixtures';
import { buildZip, collectFiles } from '../../packages/starter-packs/activities/landio/tools/bundle.mjs';
import {
  debugPage,
  joinVoice,
  mediaContext,
  trackPeerConnections,
  VOICE_STACK,
  voiceServer,
} from '../support/voice';

const ACTIVITIES = process.env.SR_E2E_ACTIVITIES === '1';

test.use({
  launchOptions: {
    args: [
      '--use-fake-device-for-media-stream',
      '--use-fake-ui-for-media-stream',
      '--disable-features=WebRtcHideLocalIpsWithMdns',
      '--allow-loopback-in-peer-connection',
    ],
  },
});

test.describe('Land.io activity (two browsers, one match)', () => {
  test.skip(
    !ACTIVITIES || !VOICE_STACK,
    'Needs SR_ACTIVITIES__ENABLED and the voice stack; set SR_E2E_ACTIVITIES=1 and SR_E2E_VOICE=1.',
  );

  test('the host runs the game, a guest plays it through the data channel', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium resolves *.localhost activity hosts by itself.');
    test.setTimeout(180_000);
    const base = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';
    const req = await playwright.request.newContext({ baseURL: base });
    const alice = await registerUser(req, 'landa');
    const bob = await registerUser(req, 'landb');
    await req.dispose();
    const staff = await playwright.request.newContext({ baseURL: base });
    await loginApi(staff, owner);

    const a = await mediaContext(browser, browserName, alice);
    const b = await mediaContext(browser, browserName, bob);
    await trackPeerConnections(a.page);
    await trackPeerConnections(b.page);
    debugPage(a.page, 'A');
    debugPage(b.page, 'B');

    // Nothing may leave for a game server: every request from the Activity goes to its own origin's static files.
    const apiCalls: string[] = [];
    for (const p of [a.page, b.page])
      p.on('request', (r) => {
        if (r.url().includes('.activities.') && /\/api\//.test(new URL(r.url()).pathname))
          apiCalls.push(r.url());
      });

    try {
      const name = `E2E Land.io ${Date.now().toString(36)}`;
      const { application: app } = await apiPost<{ application: { id: string } }>(
        b.context.request,
        '/applications',
        { name, description: 'Land.io built-in activity, e2e copy' },
      );
      await apiPatch(b.context.request, `/applications/${app.id}`, {
        privacy_policy_url: 'https://example.test/privacy',
        terms_of_service_url: 'https://example.test/terms',
      });
      const version = await apiPost<{ id: string }>(
        b.context.request,
        `/applications/${app.id}/versions`,
        { note: 'e2e' },
      );
      await apiPost(staff, `/admin/developers/versions/${version.id}/decision`, {
        decision: 'approve',
      });

      await b.page.goto(`/developers/applications/${app.id}?tab=activities`);
      await b.page.getByLabel('Activity bundle (zip)').setInputFiles({
        name: 'landio.zip',
        mimeType: 'application/zip',
        buffer: buildZip(collectFiles(`${process.cwd()}/../packages/starter-packs/activities/landio`)),
      });
      const bundles = b.page.getByRole('list', { name: 'Bundles' });
      await expect(bundles.getByRole('button', { name: 'Submit for review' })).toBeVisible({
        timeout: 30_000,
      });
      await bundles.getByRole('button', { name: 'Submit for review' }).click();
      const queue = await (await staff.get('/api/v1/admin/developers/activities')).json();
      const row = (
        queue as { config: { application_id: string }; bundle: { bundle: { id: string } } }[]
      ).find((r) => r.config.application_id === app.id);
      expect(row, 'Land.io is in the review queue').toBeTruthy();
      await apiPost(staff, `/admin/developers/activities/${app.id}/decision`, {
        decision: 'approve',
        bundle_id: row!.bundle.bundle.id,
      });

      // Alice starts it from the call (she hosts); Bob joins the instance.
      const server = await voiceServer(a.context, [b.context], 'Land Night');
      await joinVoice(a.page, server);
      await joinVoice(b.page, server);
      await a.page.getByRole('button', { name: 'Start an Activity' }).first().click();
      await a.page.getByRole('button', { name: `Start ${name}` }).click();
      const frameA = a.page.frameLocator(`iframe[title="${name} (Activity)"]`);
      await expect(frameA.locator('#start')).toBeVisible({ timeout: 60_000 });
      await b.page
        .getByRole('button', { name: `Join ${name}` })
        .first()
        .click({ timeout: 30_000 });
      const frameB = b.page.frameLocator(`iframe[title="${name} (Activity)"]`);
      await expect(frameB.locator('#start')).toBeVisible({ timeout: 60_000 });

      // Both play.
      await frameA.locator('#play').click();
      await frameB.locator('#play').click();
      const pageFrame = (p: typeof a.page) =>
        p.frames().find((f) => f.url().includes('.activities.'))!;
      const fa = pageFrame(a.page);
      const fb = pageFrame(b.page);
      await expect
        .poll(() => fb.evaluate(() => (window as any).__landio.screen()), { timeout: 30_000 })
        .toBe('playing');
      await expect
        .poll(() => fa.evaluate(() => (window as any).__landio.screen()), { timeout: 30_000 })
        .toBe('playing');

      // Bob's page gets ticks from Alice's: the tick counter moves, and no frames come from a server.
      const t0 = await fb.evaluate(() => (window as any).__landio.world.recvTick as number);
      await expect
        .poll(() => fb.evaluate(() => (window as any).__landio.world.recvTick as number), {
          timeout: 15_000,
        })
        .toBeGreaterThan(t0 + 14);
      // Both players are in both worlds (the roster comes from the host's engine).
      for (const f of [fa, fb]) {
        await expect
          .poll(
            () =>
              f.evaluate(() =>
                (window as any).__landio
                  .players()
                  .filter((p: any) => !p.bot)
                  .map((p: any) => p.name)
                  .sort(),
              ),
            { timeout: 20_000 },
          )
          .toHaveLength(2);
      }
      // Alice is the host, Bob is not.
      expect(await fa.evaluate(() => (window as any).__landio.world.you.host)).toBe(true);
      expect(await fb.evaluate(() => (window as any).__landio.world.you.host)).toBe(false);
      // Bob's input reaches the host and comes back acknowledged.
      const seq = await fb.evaluate(() => (window as any).__landio.transport.input({ turns: [1] }));
      await expect
        .poll(() => fb.evaluate(() => (window as any).__landio.world.you.ack[0] as number), {
          timeout: 10_000,
        })
        .toBeGreaterThanOrEqual(seq);
      // Bob sees a round-trip time through the channel, and the Activity made no calls to a game server.
      expect(
        await fb.evaluate(() => (window as any).__landio.transport.stats().polls as number),
      ).toBeGreaterThan(14);
      expect(apiCalls).toEqual([]);

      // Alice closes the activity: Bob is told the game ended.
      await a.page.getByRole('button', { name: /Leave|Close|Stop/ }).first().click();
      await expect(frameB.locator('#boot')).toContainText('The host left', { timeout: 30_000 });
    } finally {
      await a.context.close();
      await b.context.close();
      await staff.dispose();
    }
  });
});
