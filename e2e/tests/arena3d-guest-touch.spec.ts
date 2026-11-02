import { existsSync, mkdirSync, readFileSync, statSync } from 'node:fs';
import { extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, type CDPSession, type WebSocketRoute } from '@playwright/test';

/**
 * ACT-A3D-002: Arena 3D as a guest on a /play link, on a phone. The guest runner (the real page of apps/web), the join
 * route and the guest socket are played by routes in this file, as in guest-play.spec.ts part 1 ("relay mocked"); the
 * game's frame is the real built bundle (packages/starter-packs/activities/arena3d/dist, `node tools/bundle.mjs`)
 * served with the Activity CSP (crates/sr-activities sandbox.rs `csp()`) into the real sandboxed iframe
 * (`allow-scripts allow-pointer-lock`, opaque origin). Proves: the touch controls show up inside that sandbox, take
 * real touch events, and nothing the game does is refused by the CSP. There is no host behind the mocked relay, so the
 * game stays at "joining the host"; the full match over a real relay is the voice-stack spec's job.
 *
 * Needs only the web server (SR_E2E_BASE_URL). Chromium only (CDP touch events).
 */

const TOKEN = 'ab12cd34'.repeat(8);
const APP_ID = '777';
const GUEST_TOKEN = `srg_${'0f'.repeat(32)}`;
const dist = fileURLToPath(
  new URL('../../packages/starter-packs/activities/arena3d/dist', import.meta.url),
);
const SHOTS = process.env.SR_ARENA3D_SHOTS;

const TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.txt': 'text/plain; charset=utf-8',
};

const newKey = () => Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString('base64url');

test.skip(
  ({ browserName }) => browserName !== 'chromium' || !existsSync(join(dist, 'index.html')),
  'Chromium only, and needs the built arena3d bundle (node packages/starter-packs/activities/arena3d/tools/bundle.mjs)',
);

for (const phone of [
  { name: 'portrait', width: 375, height: 812 },
  { name: 'landscape', width: 812, height: 375 },
]) {
  test.describe(`guest on a phone, ${phone.name}`, () => {
    test('Arena 3D shows its touch controls in the guest sandbox and takes touch', async ({
      browser,
    }) => {
      const ctx = await browser.newContext({
        viewport: { width: phone.width, height: phone.height },
        hasTouch: true,
        isMobile: true,
      });
      const key = newKey();
      const cspViolations: string[] = [];
      const consoleErrors: string[] = [];

      await ctx.route('**/api/v1/play/*/join', (route) =>
        route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            guest_token: GUEST_TOKEN,
            callsign: 'Amber Otter 42',
            expires_at: '2099-01-01T00:00:00Z',
            topic: `game:${TOKEN}`,
            socket_path: '/gateway/guest',
            application_id: APP_ID,
            application_name: 'Arena 3D',
            capabilities: ['datachannel', 'guest_play', 'lobby_channel', 'host_handoff'],
          }),
        }),
      );
      await ctx.route(/^http:\/\/\d+\.activities\.localhost/, async (route) => {
        const path = decodeURIComponent(new URL(route.request().url()).pathname);
        const file = normalize(join(dist, path.endsWith('/') ? `${path}index.html` : path));
        if (
          !resolve(file).startsWith(resolve(dist) + sep) ||
          !existsSync(file) ||
          !statSync(file).isFile()
        ) {
          return route.fulfill({ status: 404, body: '' });
        }
        return route.fulfill({
          status: 200,
          body: readFileSync(file),
          headers: {
            'content-type': TYPES[extname(file)] ?? 'application/octet-stream',
            // The Activity CSP the server sends (sandbox.rs `csp()`), with connect: ["self"] from the manifest.
            'content-security-policy':
              "sandbox allow-scripts allow-pointer-lock; default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; " +
              "style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; " +
              "connect-src 'self'; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'; " +
              `frame-ancestors ${process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173'}`,
          },
        });
      });
      await ctx.routeWebSocket(/\/gateway\/guest/, (ws: WebSocketRoute) => {
        ws.onMessage((m) => {
          const f = JSON.parse(String(m)) as Record<string, unknown>;
          if (f.op === 'hello') {
            ws.send(
              JSON.stringify({
                op: 'ready',
                seat: 2,
                role: 'guest',
                callsign: 'Amber Otter 42',
                key_id: 'k',
                epoch: 0,
                scope: { data: true, media: false },
                peers: [{ seat: 1, role: 'member', callsign: 'Host' }],
              }),
            );
          }
          if (f.op === 'subscribe') ws.send(JSON.stringify({ op: 'subscribed', topic: f.topic }));
        });
      });

      const page = await ctx.newPage();
      // Only what the GAME frame logs (the app shell's own dev-server report-only CSP is not under test here).
      page.on('console', (m) => {
        if (!m.location().url.includes('.activities.')) return;
        if (m.type() === 'error') consoleErrors.push(m.text());
        if (/Content Security Policy|violates the following/i.test(m.text()))
          cspViolations.push(m.text());
      });
      await page.goto(`/play/${TOKEN}#key=${key}`);
      await page.getByRole('button', { name: 'Join the game' }).click();
      const iframe = page.locator('iframe[title="Game: Arena 3D"]');
      await expect(iframe).toBeVisible();
      expect(await iframe.getAttribute('sandbox')).toBe('allow-scripts allow-pointer-lock');
      const game = page.frameLocator('iframe[title="Game: Arena 3D"]');

      // The game boots in the sandbox, as a guest, on a touch screen.
      await expect(game.locator('#btn-play')).toBeEnabled({ timeout: 90_000 });
      await expect(game.locator('#status')).toContainText('guest');
      await expect(game.locator('.touch-keys')).toBeVisible();
      await game.locator('#btn-play').tap();
      await expect(game.locator('#touch')).toBeVisible();
      await expect(game.locator('#touch-jump')).toBeVisible();
      await expect(game.locator('#touch-crouch')).toBeVisible();
      await expect(game.locator('#touch-menu')).toBeVisible();

      // Everything the thumbs need is on screen.
      const outer = { w: phone.width, h: phone.height };
      for (const id of ['touch-stick-zone', 'touch-jump', 'touch-crouch', 'touch-menu']) {
        const b = await game.locator(`#${id}`).boundingBox();
        expect(b, id).toBeTruthy();
        expect(b!.x, id).toBeGreaterThanOrEqual(0);
        expect(b!.y, id).toBeGreaterThanOrEqual(0);
        expect(b!.x + b!.width, id).toBeLessThanOrEqual(outer.w + 1);
        expect(b!.y + b!.height, id).toBeLessThanOrEqual(outer.h + 1);
      }

      // A real touch on the stick zone reaches the game through the sandbox: the floating stick appears under the thumb.
      const cdp: CDPSession = await ctx.newCDPSession(page);
      const z = (await game.locator('#touch-stick-zone').boundingBox())!;
      const tx = z.x + 80;
      const ty = z.y + z.height - 70;
      await cdp.send('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: [{ x: tx, y: ty, id: 1 }],
      });
      await cdp.send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: [{ x: tx, y: ty - 50, id: 1 }],
      });
      await expect(game.locator('#touch-base')).toBeVisible();
      if (SHOTS) {
        mkdirSync(SHOTS, { recursive: true });
        await page.screenshot({ path: `${SHOTS}/arena3d-guest-touch-${phone.name}.png` });
      }
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await expect(game.locator('#touch-base')).toBeHidden();
      // Crouch toggles in the sandbox too.
      await game.locator('#touch-crouch').tap();
      await expect(game.locator('#touch-crouch')).toHaveAttribute('aria-pressed', 'true');

      expect(cspViolations).toEqual([]);
      expect(consoleErrors.filter((e) => !/favicon/.test(e))).toEqual([]);
      await ctx.close();
    });
  });
}
