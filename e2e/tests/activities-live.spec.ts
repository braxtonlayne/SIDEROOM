import { expect, test, apiPatch, apiPost, loginApi, registerUser } from '../support/fixtures';
import {
  builtInActivityBundle,
  testActivityBundle,
  type BuiltInActivity,
} from '../support/activityBundle';
import { startActivityForTwo, type ActivitySession } from '../support/activities';
import { holdemCase } from '../support/activity-cases/holdem';
import { landioCase } from '../support/activity-cases/landio';
import { tabletopCase } from '../support/activity-cases/tabletop';
import { whiteboardCase } from '../support/activity-cases/whiteboard';
import {
  debugPage,
  joinVoice,
  mediaContext,
  trackPeerConnections,
  VOICE_STACK,
  voiceServer,
} from '../support/voice';

/**
 * Activities end to end (DEV-ACT-001..004, J-12): the Owner uploads a bundle in the developer portal and
 * submits it, approves it as staff, starts it from a voice call; the iframe loads from the sandboxed
 * `<id>.activities.localhost:5173` origin with the Activity CSP, the real Embedded App SDK finishes its
 * handshake and `authorize`, and a second person in the call sees the instance and joins it.
 *
 * Needs sr-server with `SR_ACTIVITIES__ENABLED=true` and `SR_VOICE__ENABLED=true`, the web dev server with
 * `VITE_ACTIVITIES_ENABLED=true` (forwards the activity hosts to sr-server, as Caddy does in production).
 * Set SR_E2E_ACTIVITIES=1. Chromium only: it resolves `*.localhost` to loopback by itself.
 */
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

test.describe('Activities (real browser)', () => {
  test.skip(
    !ACTIVITIES || !VOICE_STACK,
    'Needs SR_ACTIVITIES__ENABLED and the voice stack; set SR_E2E_ACTIVITIES=1 and SR_E2E_VOICE=1.',
  );

  test('upload, approve, launch in a voice channel, handshake and a second participant', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(
      browserName !== 'chromium',
      'Chromium resolves *.localhost activity hosts by itself.',
    );
    const base = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';
    const req = await playwright.request.newContext({ baseURL: base });
    // Fresh accounts for the browser pages (the bootstrapped Owner is in every server earlier specs made,
    // and its READY is slow); the Owner only reviews, over the API.
    const alice = await registerUser(req, 'acta');
    const bob = await registerUser(req, 'actb');
    await req.dispose();
    const staff = await playwright.request.newContext({ baseURL: base });
    await loginApi(staff, owner);

    const a = await mediaContext(browser, browserName, alice);
    const b = await mediaContext(browser, browserName, bob);
    await trackPeerConnections(a.page);
    await trackPeerConnections(b.page);
    debugPage(a.page, 'A');
    debugPage(b.page, 'B');

    try {
      // --- Bob's application with a version the Owner approved (nobody reviews their own; the proxy
      // serves only released applications).
      const name = `E2E Activity ${Date.now().toString(36)}`;
      const { application: app } = await apiPost<{ application: { id: string } }>(
        b.context.request,
        '/applications',
        {
          name,
          description: 'Browser test Activity',
        },
      );
      // A version needs the policy links (doc 11).
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

      // --- Developer portal (Bob): upload the bundle and submit it.
      await b.page.goto(`/developers/applications/${app.id}?tab=activities`);
      await b.page.getByLabel('Activity bundle (zip)').setInputFiles({
        name: 'activity.zip',
        mimeType: 'application/zip',
        buffer: testActivityBundle(),
      });
      const bundles = b.page.getByRole('list', { name: 'Bundles' });
      await expect(bundles.getByRole('button', { name: 'Submit for review' })).toBeVisible({
        timeout: 30_000,
      });
      await bundles.getByRole('button', { name: 'Submit for review' }).click();
      await expect(bundles.getByText('In review')).toBeVisible();

      // --- Staff: the review queue lists it; approve that bundle.
      const queue = await (await staff.get('/api/v1/admin/developers/activities')).json();
      const row = (
        queue as { config: { application_id: string }; bundle: { bundle: { id: string } } }[]
      ).find((r) => r.config.application_id === app.id);
      expect(row, 'the Activity is in the review queue').toBeTruthy();
      await apiPost(staff, `/admin/developers/activities/${app.id}/decision`, {
        decision: 'approve',
        bundle_id: row!.bundle.bundle.id,
      });

      // --- Both in one voice channel; A starts the Activity from the call bar.
      const server = await voiceServer(a.context, [b.context], 'Game Night');
      await joinVoice(a.page, server);
      await joinVoice(b.page, server);

      const activityHost = `${app.id}.activities.localhost`;
      const entry = a.page.waitForResponse(
        (r) => new URL(r.url()).hostname === activityHost && new URL(r.url()).pathname === '/',
      );
      await a.page.getByRole('button', { name: 'Start an Activity' }).first().click();
      await a.page.getByRole('button', { name: `Start ${name}` }).click();

      // The frame: own origin, sandbox, the Activity CSP with frame-ancestors = the app.
      const res = await entry;
      expect(res.status()).toBe(200);
      const csp = (await res.allHeaders())['content-security-policy'] ?? '';
      expect(csp).toContain("default-src 'self'");
      expect(csp).toContain("script-src 'self'");
      expect(csp).toContain('frame-ancestors http://localhost:5173');
      expect((await res.allHeaders())['x-content-type-options']).toBe('nosniff');
      const iframe = a.page.locator(`iframe[title="${name} (Activity)"]`);
      await expect(iframe).toHaveAttribute(
        'src',
        new RegExp(`^http://${activityHost.replace(/\./g, '\\.')}:5173/`),
      );
      await expect(iframe).toHaveAttribute('sandbox', /allow-scripts/);
      expect(await iframe.getAttribute('sandbox')).not.toContain('allow-top-navigation');

      // The SDK handshake and commands.
      const frameA = a.page.frameLocator(`iframe[title="${name} (Activity)"]`);
      await expect(frameA.locator('[data-step="ready"]')).toBeVisible({ timeout: 30_000 });
      await expect(frameA.locator('[data-step="channel"]')).toHaveText('channel: Game Night');
      await expect(frameA.locator('[data-step="participants"]')).toHaveText('participants: 1');
      await expect(frameA.locator('[data-step="subscribed"]')).toBeVisible();
      await expect(frameA.locator('[data-step="error"]')).toHaveCount(0);
      // The Activity cannot reach the app's origin: its document is on another origin.
      const frameOrigin = await a.page
        .frames()
        .find((f) => f.url().includes(activityHost))
        ?.evaluate(() => location.origin);
      expect(frameOrigin).toBe(`http://${activityHost}:5173`);

      // authorize: the host asks the person, then hands the Activity an OAuth2 code.
      await frameA.getByRole('button', { name: 'Authorize' }).click();
      await a.page.getByRole('button', { name: 'Allow', exact: true }).click();
      await expect(frameA.locator('[data-step="authorize"]')).toHaveText('authorize: code', {
        timeout: 20_000,
      });

      // --- B sees the running instance and joins it.
      await expect(b.page.getByRole('button', { name: `Join ${name}` }).first()).toBeVisible({
        timeout: 20_000,
      });
      await b.page
        .getByRole('button', { name: `Join ${name}` })
        .first()
        .click();
      const frameB = b.page.frameLocator(`iframe[title="${name} (Activity)"]`);
      await expect(frameB.locator('[data-step="ready"]')).toBeVisible({ timeout: 30_000 });
      await expect(frameB.locator('[data-step="participants"]')).toHaveText('participants: 2');
      await expect(
        frameA.locator('[data-step="participants-update"]').filter({ hasText: '2' }).first(),
      ).toBeVisible({ timeout: 20_000 });

      // --- ACT-CH-001: the two Activities share state through the SFU, sealed with the call's keys. A is the
      // host (it started the instance). Each writes a key of its own; the other one sees it.
      await expect(frameA.locator('[data-step="self"]')).toContainText('host', { timeout: 20_000 });
      await expect(frameB.locator('[data-step="self"]')).toContainText('host', { timeout: 20_000 });
      await frameA.getByRole('button', { name: 'Set state' }).click();
      await expect(
        frameB.locator('[data-step="state"]').filter({ hasText: '=hi' }).first(),
      ).toBeVisible({
        timeout: 30_000,
      });
      await frameB.getByRole('button', { name: 'Set state' }).click();
      await expect(
        frameA.locator('[data-step="state"]').filter({ hasText: '=hi' }).first(),
      ).toBeVisible({ timeout: 30_000 });
      await expect(frameA.locator('[data-step="state-error"]')).toHaveCount(0);
      await expect(frameB.locator('[data-step="state-error"]')).toHaveCount(0);
    } finally {
      await a.context.close();
      await b.context.close();
      await staff.dispose();
    }
  });
});

/**
 * The built-in Activities of wave 36, each installed from its real bundle (scripts/build-activities.mjs, the zip
 * a server's developer uploads) and run by two people in one call: everything they share goes through the SFU's
 * data channel, sealed with the call's keys.
 */
test.describe('Built-in activities through the data channel (real browser)', () => {
  test.skip(
    !ACTIVITIES || !VOICE_STACK,
    'Needs SR_ACTIVITIES__ENABLED and the voice stack; set SR_E2E_ACTIVITIES=1 and SR_E2E_VOICE=1.',
  );
  test.setTimeout(240_000);

  const cases: [BuiltInActivity, (s: ActivitySession) => Promise<void>][] = [
    ['whiteboard', whiteboardCase],
    ['holdem', holdemCase],
    ['tabletop', tabletopCase],
    ['landio', landioCase],
  ];
  for (const [activity, run] of cases) {
    test(`${activity}: two people share it through the SFU`, async ({
      browser,
      browserName,
      owner,
      playwright,
    }) => {
      test.skip(
        browserName !== 'chromium',
        'Chromium resolves *.localhost activity hosts by itself.',
      );
      const session = await startActivityForTwo(
        { browser, browserName, owner, playwright },
        builtInActivityBundle(activity),
        activity,
      );
      try {
        await run(session);
      } finally {
        await session.close();
      }
    });
  }
});
