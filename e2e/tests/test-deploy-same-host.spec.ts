import type { APIRequestContext, Page, Request } from '@playwright/test';
import { expect, test, apiPatch, apiPost, apiPostBytes, loginApi, registerUser } from '../support/fixtures';
import { builtInActivityBundle, sdkModules, zip } from '../support/activityBundle';
import { PUBLIC_URL } from '../support/cli';

/**
 * OPS-TDEP-001 (D-450 to D-452, threat model T49): the test-only `same_host_sandboxed` activity origin mode, end to
 * end in Chromium against the BUILT web app served by sr-server (SR_E2E_WEB=server), as a quick-tunnel test
 * deployment runs it. No voice: the Activity runs in a text channel.
 *
 * Needs sr-server with SR_ACTIVITIES__ENABLED=true and SR_ACTIVITIES__ORIGIN_MODE=same_host_sandboxed (and, like
 * every server-mode run, SR_WEB__ROOT=apps/web/dist, SR_WEB__SERVE_PUBLIC=true). Set SR_E2E_SAME_HOST=1.
 *
 * Proves: the frame is `/_activity/<id>/` on the main host with the sandbox attribute; the document carries the
 * activity policy (`sandbox` first, `frame-ancestors` the shell) and no Set-Cookie; the game's origin is opaque, it
 * cannot read cookies or storage, cannot reach the API (connect-src is its own path only), and its own credentialed
 * fetch carries no session cookie; the main origin cannot load a game file as its own; the real SDK finishes its
 * handshake from the path form; a built-in Activity (Whiteboard) derives its id from the path too; the signed-in
 * page shows the "Test deployment" notice.
 */
const ON = process.env.SR_E2E_SAME_HOST === '1';

/** A probe Activity: it tries what a hostile game would, and writes each result into `#log`. */
function probeBundle(): Buffer {
  const main = `import { SideRoomSDK, applicationIdFromLocation } from './sdk/index.js';
const log = (step, value) => {
  const li = document.createElement('li');
  li.dataset.step = step;
  li.dataset.value = value;
  li.textContent = step + ': ' + value;
  document.getElementById('log').append(li);
};
log('path', location.pathname);
log('origin', String(self.origin));
try {
  const jar = document['coo' + 'kie'];
  log('cookie', 'readable ' + jar.length);
} catch (e) {
  log('cookie', 'blocked');
}
try {
  window['local' + 'Storage'].getItem('x');
  log('storage', 'readable');
} catch (e) {
  log('storage', 'blocked');
}
try {
  await fetch('/api/v1/users/@me', { credentials: 'include' });
  log('api', 'reachable');
} catch (e) {
  log('api', 'blocked');
}
try {
  const r = await fetch('data.json', { credentials: 'include' });
  log('own', String(r.status));
} catch (e) {
  log('own', 'error');
}
const sdk = new SideRoomSDK(applicationIdFromLocation(location), { timeoutMs: 90000 });
try {
  const cfg = await sdk.ready();
  log('ready', cfg.instance_id);
} catch (e) {
  log('error', e.code || String(e));
}
`;
  const html = `<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Same-host probe</title></head>
<body>
<h1>Same-host probe</h1>
<ul id="log"></ul>
<script type="module" src="main.js"></script>
</body>
</html>
`;
  return zip({
    'index.html': html,
    'main.js': main,
    'data.json': '{"ok":true}\n',
    // "self" is the Activity's own path in this mode, never the API.
    'sideroom.json': JSON.stringify({ capabilities: [], connect: ['self'] }),
    ...sdkModules(),
  });
}

/** Bob's application with an approved version and an approved bundle (the server installs it like a built-in). */
async function installActivity(
  dev: APIRequestContext,
  staff: APIRequestContext,
  bundle: Buffer,
  label: string,
): Promise<{ appId: string; name: string }> {
  const name = `E2E ${label} ${Date.now().toString(36)}`;
  const { application: app } = await apiPost<{ application: { id: string } }>(dev, '/applications', {
    name,
    description: `Same-host test (${label})`,
  });
  await apiPatch(dev, `/applications/${app.id}`, {
    privacy_policy_url: 'https://example.test/privacy',
    terms_of_service_url: 'https://example.test/terms',
  });
  const version = await apiPost<{ id: string }>(dev, `/applications/${app.id}/versions`, { note: 'e2e' });
  await apiPost(staff, `/admin/developers/versions/${version.id}/decision`, { decision: 'approve' });
  const up = await apiPostBytes<{ bundle: { id: string } }>(
    dev,
    `/applications/${app.id}/activity/bundles`,
    bundle,
    'application/zip',
  );
  await apiPost(dev, `/applications/${app.id}/activity/submit`, { bundle_id: up.bundle.id });
  await apiPost(staff, `/admin/developers/activities/${app.id}/decision`, {
    decision: 'approve',
    bundle_id: up.bundle.id,
  });
  return { appId: app.id, name };
}

/** A new server owned by `req`'s account; returns its id and the text channel. */
async function textServer(req: APIRequestContext): Promise<{ guildId: string; channelId: string }> {
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(req, '/guilds', {
    name: `Same host ${Date.now().toString(36)}`,
  });
  const text = guild.channels.find((c) => c.type === 0);
  if (!text) throw new Error('new server has no text channel');
  return { guildId: guild.id, channelId: text.id };
}

const step = (page: Page, name: string, s: string) =>
  page.frameLocator(`iframe[title="${name} (Activity)"]`).locator(`li[data-step="${s}"]`);

test.describe('Test deployment: same-host Activities (real browser)', () => {
  test.skip(!ON, 'Needs sr-server with SR_ACTIVITIES__ORIGIN_MODE=same_host_sandboxed; set SR_E2E_SAME_HOST=1.');

  test('a sandboxed Activity runs from /_activity/<id>/ with the activity policy and no cookie', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium leg of the test deployment.');
    const req = await playwright.request.newContext({ baseURL: PUBLIC_URL });
    const bob = await registerUser(req, 'tdepdev');
    const alice = await registerUser(req, 'tdepa');
    await req.dispose();
    const staff = await playwright.request.newContext({ baseURL: PUBLIC_URL });
    await loginApi(staff, owner);
    const devCtx = await playwright.request.newContext({ baseURL: PUBLIC_URL });
    await loginApi(devCtx, bob);
    const context = await browser.newContext({ baseURL: PUBLIC_URL });
    await loginApi(context.request, alice);
    const page = await context.newPage();
    await page.setViewportSize({ width: 1280, height: 1000 });

    try {
      const { appId, name } = await installActivity(devCtx, staff, probeBundle(), 'probe');
      const base = `/_activity/${appId}/`;
      const frameUrl = new URL(base, PUBLIC_URL).href;

      // Every request and response under the activity path, for the cookie checks.
      const activityRequests: Request[] = [];
      const setCookies: string[] = [];
      page.on('request', (r) => {
        if (new URL(r.url()).pathname.startsWith(base)) activityRequests.push(r);
      });
      page.on('response', async (r) => {
        if (!new URL(r.url()).pathname.startsWith('/_activity/')) return;
        const sc = await r.headerValue('set-cookie');
        if (sc) setCookies.push(`${r.url()}: ${sc}`);
      });

      const { guildId, channelId } = await textServer(context.request);
      await apiPost(context.request, `/channels/${channelId}/activity`, { application_id: appId });
      const doc = page.waitForResponse((r) => r.url() === frameUrl, { timeout: 45_000 });
      await page.goto(`/channels/${guildId}/${channelId}`);

      // The signed-in app says this is a test deployment.
      await expect(page.getByTestId('test-deployment-notice')).toBeVisible({ timeout: 30_000 });
      await expect(page.getByTestId('test-deployment-notice')).toContainText('Test deployment');

      const iframe = page.locator(`iframe[title="${name} (Activity)"]`);
      await iframe.waitFor({ timeout: 45_000 });
      await expect(iframe).toHaveAttribute('src', frameUrl);
      await expect(iframe).toHaveAttribute('sandbox', 'allow-scripts allow-pointer-lock');

      const res = await doc;
      expect(res.status()).toBe(200);
      const csp = (await res.headerValue('content-security-policy')) ?? '';
      expect(csp.startsWith('sandbox allow-scripts allow-pointer-lock;'), csp).toBe(true);
      expect(csp).not.toContain('allow-same-origin');
      expect(csp).toContain(`frame-ancestors ${new URL(PUBLIC_URL).origin}`);
      expect(csp).toContain(`connect-src ${new URL(base, PUBLIC_URL).href};`);
      expect(await res.headerValue('set-cookie')).toBeNull();

      // Inside the frame: an opaque origin with no cookie, no storage and no way to the API; the real SDK finishes
      // its handshake with the id it read from the path.
      await expect(step(page, name, 'ready')).toBeVisible({ timeout: 45_000 });
      await expect(step(page, name, 'path')).toHaveAttribute('data-value', base);
      await expect(step(page, name, 'origin')).toHaveAttribute('data-value', 'null');
      await expect(step(page, name, 'cookie')).toHaveAttribute('data-value', 'blocked');
      await expect(step(page, name, 'storage')).toHaveAttribute('data-value', 'blocked');
      await expect(step(page, name, 'api')).toHaveAttribute('data-value', 'blocked');
      await expect(step(page, name, 'own')).toHaveAttribute('data-value', '200');

      // The game's own credentialed fetch went out without the session cookie (opaque origin: cross-site).
      const own = activityRequests.find((r) => r.url().endsWith(`${base}data.json`));
      expect(own, 'the game fetched its own file').toBeTruthy();
      const ownHeaders = await own!.allHeaders();
      expect(ownHeaders.cookie ?? '').not.toContain('sr_session');
      expect(ownHeaders['sec-fetch-site']).toBe('cross-site');
      expect(ownHeaders.origin).toBe('null');
      expect(setCookies, 'no activity response sets a cookie').toEqual([]);

      // D-452: the app's own origin cannot pull a game file in as its own (a fetch here is same-origin, not a frame).
      const status = await page.evaluate(
        async (u) => (await fetch(u, { credentials: 'include' })).status,
        `${base}main.js`,
      );
      expect(status).toBe(403);
    } finally {
      await context.close();
      await devCtx.dispose();
      await staff.dispose();
    }
  });

  test('a built-in Activity (Whiteboard) finds its application id on the same-host path', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium leg of the test deployment.');
    const req = await playwright.request.newContext({ baseURL: PUBLIC_URL });
    const bob = await registerUser(req, 'tdepwbd');
    const alice = await registerUser(req, 'tdepwba');
    await req.dispose();
    const staff = await playwright.request.newContext({ baseURL: PUBLIC_URL });
    await loginApi(staff, owner);
    const devCtx = await playwright.request.newContext({ baseURL: PUBLIC_URL });
    await loginApi(devCtx, bob);
    const context = await browser.newContext({ baseURL: PUBLIC_URL });
    await loginApi(context.request, alice);
    const page = await context.newPage();
    await page.setViewportSize({ width: 1280, height: 1000 });
    try {
      const { appId, name } = await installActivity(
        devCtx,
        staff,
        builtInActivityBundle('whiteboard'),
        'whiteboard',
      );
      const { guildId, channelId } = await textServer(context.request);
      const instance = await apiPost<{ id: string }>(context.request, `/channels/${channelId}/activity`, {
        application_id: appId,
      });
      // The host answers a handshake only when the game names this application; then it mints the proxy token.
      const handshake = page.waitForResponse(
        (r) =>
          r.url().endsWith(`/activity-instances/${instance.id}/proxy-token`) &&
          r.request().method() === 'POST',
        { timeout: 60_000 },
      );
      await page.goto(`/channels/${guildId}/${channelId}`);
      const iframe = page.locator(`iframe[title="${name} (Activity)"]`);
      await iframe.waitFor({ timeout: 45_000 });
      await expect(iframe).toHaveAttribute('src', new URL(`/_activity/${appId}/`, PUBLIC_URL).href);
      expect((await handshake).ok()).toBe(true);
    } finally {
      await context.close();
      await devCtx.dispose();
      await staff.dispose();
    }
  });
});
