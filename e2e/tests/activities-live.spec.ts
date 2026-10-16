import { expect, test, apiPost, registerUser } from '../support/fixtures';
import { testActivityBundle } from '../support/activityBundle';
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
    const bob = await registerUser(req, 'actb');
    await req.dispose();

    const a = await mediaContext(browser, browserName, owner);
    const b = await mediaContext(browser, browserName, bob);
    await trackPeerConnections(a.page);
    await trackPeerConnections(b.page);
    debugPage(a.page, 'A');
    debugPage(b.page, 'B');

    try {
      // --- An application with an approved version (the proxy serves only released applications).
      const name = `E2E Activity ${Date.now().toString(36)}`;
      const app = await apiPost<{ id: string }>(a.context.request, '/applications', {
        name,
        description: 'Browser test Activity',
      });
      const version = await apiPost<{ id: string }>(
        a.context.request,
        `/applications/${app.id}/versions`,
        { note: 'e2e' },
      );
      await apiPost(a.context.request, `/admin/developers/versions/${version.id}/decision`, {
        decision: 'approve',
      });

      // --- Developer portal: upload the bundle and submit it.
      await a.page.goto(`/developers/applications/${app.id}?tab=activities`);
      await a.page.getByLabel('Activity bundle (zip)').setInputFiles({
        name: 'activity.zip',
        mimeType: 'application/zip',
        buffer: testActivityBundle(),
      });
      const bundles = a.page.getByRole('list', { name: 'Bundles' });
      await expect(bundles.getByRole('listitem')).toHaveCount(1, { timeout: 30_000 });
      await bundles.getByRole('button', { name: 'Submit for review' }).click();
      await expect(bundles.getByText('In review')).toBeVisible();

      // --- Staff: approve it in Review Queues, Apps.
      await a.page.goto('/admin/review');
      await a.page.getByRole('tab', { name: 'Apps' }).click();
      await a.page.getByRole('button', { name: `Approve the Activity of ${name}` }).click();
      await a.page.getByRole('dialog').getByRole('button', { name: 'Confirm' }).click();
      await expect(a.page.getByText('Decision recorded.')).toBeVisible();

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
    } finally {
      await a.context.close();
      await b.context.close();
    }
  });
});
