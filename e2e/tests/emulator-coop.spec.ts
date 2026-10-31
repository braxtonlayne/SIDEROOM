import type { Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { apiPut, expect, test, registerUser } from '../support/fixtures';
import {
  joinVoice,
  mediaContext,
  trackPeerConnections,
  VOICE_STACK,
  voiceServer,
} from '../support/voice';

/**
 * Co-op by input forwarding (ACT-EMU-002), two browser contexts in one voice channel.
 *
 * The host runs a core and shares its canvas through the screen share (voice video path, E2EE). The guest
 * takes player 2, presses a button, and the host's frame changes: the pad travelled guest -> encrypted
 * activity data channel -> SFU -> host -> core port 1. The test also reads the colour of the picture the guest
 * receives, so the whole loop (input out, frame back) is proven.
 *
 * Two modes:
 *  - default ("probe"): the host offers a tiny in-page core (a canvas that is red, and green while player 2
 *    holds A) through the e2e hook `window.__srCoopOffer` (VITE_SR_E2E=1, set by the Playwright dev server).
 *    It needs nothing from the emulator loader, so it proves the co-op path on its own.
 *  - SR_E2E_COOP_ROM=1: the host opens Games from the section bar (in-app, so the call stays up), plays the repo's
 *    self-written test ROM (`e2e/fixtures/emu/sideroom-test.nes`: red, green while controller 2 holds A, read from
 *    $4017) through the real loader and the FCEUmm core, and hosts it from the Games page's call strip. The ROM list,
 *    the mount and the bucket are route handlers (as in emulator.spec.ts). Skips when the core is not installed.
 *
 * The seat rule (owner default): only members holding `view_games` on the server take a seat. The guest is refused
 * first, then the server owner grants `view_games` to @everyone and the guest is seated. Guests never get the ROM.
 *
 * Needs sr-server with `SR_VOICE__ENABLED=true` (SR_E2E_VOICE=1). Chromium only (fake display capture).
 */
const ROM_MODE = process.env.SR_E2E_COOP_ROM === '1';
const here = dirname(fileURLToPath(import.meta.url));
const ROM_KEY = 'emu/roms/sideroom-test.nes';
const CORE_URL = '/emu/cores/fceumm/fceumm-legacy-wasm.data';
const BUCKET = 'https://bucket.test';
const RED = [200, 40, 40] as const;
const GREEN = [40, 200, 40] as const;

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

/** Host side, before the page loads: "the screen" is the game canvas, so the share needs no OS picker. */
async function shareCanvasInsteadOfScreen(page: Page, canvasSelector: string) {
  await page.addInitScript((sel) => {
    const md = navigator.mediaDevices;
    md.getDisplayMedia = async () => {
      const canvas = document.querySelector<HTMLCanvasElement>(sel);
      if (!canvas) throw new DOMException('no game canvas', 'NotFoundError');
      return canvas.captureStream(30);
    };
  }, canvasSelector);
}

/** Probe mode: a canvas that is red, and green while player 2 (port 1) holds A (RetroPad button 8). */
async function startProbeCore(page: Page) {
  await page.evaluate(() => {
    const canvas = document.createElement('canvas');
    canvas.id = 'coop-probe';
    canvas.width = 160;
    canvas.height = 120;
    canvas.style.cssText = 'position:fixed;inset-block-end:8px;inset-inline-start:8px;z-index:5;';
    document.body.append(canvas);
    const ctx = canvas.getContext('2d')!;
    let held = false;
    const paint = () => {
      ctx.fillStyle = held ? 'rgb(40,200,40)' : 'rgb(200,40,40)';
      ctx.fillRect(0, 0, 160, 120);
      requestAnimationFrame(paint);
    };
    paint();
    const offer = (window as unknown as { __srCoopOffer: (g: unknown) => void }).__srCoopOffer;
    offer({
      title: 'Probe',
      setInput: (port: number, button: number, value: number) => {
        if (port === 1 && button === 8) held = value === 1;
      },
    });
  });
}

/** ROM mode: is the FCEUmm core served (a 7z archive)? */
async function coreInstalled(page: Page): Promise<boolean> {
  const core = await page.request.get(CORE_URL);
  const magic = core.ok() ? [...(await core.body()).subarray(0, 6)] : [];
  return magic.join() === [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c].join();
}

/** ROM mode: the ROM list, the member's mount and the bucket, served by route handlers (no real bucket). */
async function routeTestRom(page: Page) {
  const rom = readFileSync(resolve(here, '../fixtures/emu/sideroom-test.nes'));
  const mount = {
    id: '7',
    owner_kind: 'user',
    owner_id: '1',
    label: 'Test bucket',
    provider: 's3',
    endpoint: null,
    region: 'us-east-1',
    bucket: 'games',
    prefix: '',
    path_style: false,
    access_key_id: 'AKIAEXAMPLE',
    key_hint: 'MPLE',
    status: 'active',
    cors_ok_at: '2026-10-30T00:00:00Z',
    last_test_at: null,
    last_test: null,
    created_at: '2026-10-30T00:00:00Z',
  };
  await page.route('**/api/v1/users/@me/games/roms', (route) =>
    route.fulfill({
      json: {
        roms: [
          {
            id: '501',
            mount_id: '7',
            key: ROM_KEY,
            size: rom.length,
            title: 'sideroom test',
            platform: 'nes',
            core: 'fceumm',
            scope: 'own',
            mount_label: mount.label,
          },
        ],
      },
    }),
  );
  await page.route('**/api/v1/users/@me/storage/mounts', (route) =>
    route.fulfill({ json: [mount] }),
  );
  await page.route('**/api/v1/storage/mounts/7/objects?*', (route) =>
    route.fulfill({ json: { folders: [], objects: [], next_cursor: null } }),
  );
  await page.route('**/api/v1/storage/mounts/7/presign', async (route) => {
    const body = route.request().postDataJSON() as { key: string; method: string };
    await route.fulfill({
      json: {
        url: `${BUCKET}/${body.method}/${encodeURI(body.key)}`,
        expires_at: '2099-01-01T00:00:00Z',
      },
    });
  });
  await page.route(`${BUCKET}/**`, async (route) => {
    const req = route.request();
    const headers = { 'access-control-allow-origin': '*', 'access-control-allow-methods': '*' };
    if (req.method() === 'OPTIONS') return route.fulfill({ status: 204, headers });
    const key = decodeURI(new URL(req.url()).pathname.replace(/^\/(GET|PUT)\//, ''));
    if (req.method() === 'GET' && key === ROM_KEY) {
      return route.fulfill({ status: 200, headers, body: rom });
    }
    return route.fulfill({ status: req.method() === 'PUT' ? 200 : 404, headers });
  });
}

/** ROM mode: open Games IN-APP (a reload would drop the call) and play the test ROM until it runs. */
async function startTestRom(page: Page) {
  await page.getByRole('link', { name: 'Games', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Games', level: 1 })).toBeVisible();
  await page.getByRole('button', { name: 'Play' }).first().click();
  await expect(page.locator('canvas[data-testid="emu-canvas"]')).toHaveAttribute(
    'data-phase',
    'running',
    { timeout: 60_000 },
  );
}

const CANVAS = ROM_MODE ? 'canvas[data-testid="emu-canvas"]' : '#coop-probe';

/** Mean colour of the middle of the canvas (host) or of the largest video (guest). */
async function sample(
  page: Page,
  source: 'canvas' | 'video',
): Promise<[number, number, number] | null> {
  return page.evaluate(
    ({ source, sel }) => {
      let el: CanvasImageSource | null = null;
      let w = 0;
      let h = 0;
      if (source === 'canvas') {
        const c = document.querySelector<HTMLCanvasElement>(sel);
        if (c) [el, w, h] = [c, c.width, c.height];
      } else {
        const vids = Array.from(document.querySelectorAll('video')).filter((v) => v.videoWidth > 0);
        vids.sort((a, b) => b.videoWidth * b.videoHeight - a.videoWidth * a.videoHeight);
        const v = vids[0];
        if (v) [el, w, h] = [v, v.videoWidth, v.videoHeight];
      }
      if (!el || !w || !h) return null;
      const probe = document.createElement('canvas');
      probe.width = probe.height = 8;
      const ctx = probe.getContext('2d', { willReadFrequently: true })!;
      ctx.drawImage(el, w * 0.4, h * 0.4, w * 0.2, h * 0.2, 0, 0, 8, 8);
      const d = ctx.getImageData(0, 0, 8, 8).data;
      let r = 0;
      let g = 0;
      let b = 0;
      for (let i = 0; i < d.length; i += 4) [r, g, b] = [r + d[i]!, g + d[i + 1]!, b + d[i + 2]!];
      const n = d.length / 4;
      return [r / n, g / n, b / n] as [number, number, number];
    },
    { source, sel: CANVAS },
  );
}

const colourName = (got: [number, number, number] | null): string | null => {
  if (got === null) return null;
  const close = (want: readonly [number, number, number]) =>
    got.every((v, i) => Math.abs(v - want[i]!) < 60);
  const [r, g, b] = got;
  // The probe paints RED / GREEN exactly; the NES palette's red ($16) and green ($2A) are near them, and a video
  // frame shifts colours a little, so a clearly dominant channel counts too.
  if (close(RED) || (r > 140 && g < 110 && b < 110)) return 'red';
  if (close(GREEN) || (g > 140 && r < 120 && b < 110)) return 'green';
  return 'other';
};

test.describe('Co-op play (real browser)', () => {
  test.skip(!VOICE_STACK, 'Needs the voice stack; set SR_E2E_VOICE=1.');

  test('the guest takes player 2 and its button changes the host frame', async ({
    browser,
    browserName,
    playwright,
  }) => {
    test.skip(browserName !== 'chromium', 'Chromium only: canvas capture and fake devices.');
    const base = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';
    const req = await playwright.request.newContext({ baseURL: base });
    const host = await registerUser(req, 'coophost');
    const guest = await registerUser(req, 'coopgst');
    await req.dispose();

    const h = await mediaContext(browser, browserName, host);
    const g = await mediaContext(browser, browserName, guest);
    await trackPeerConnections(h.page);
    await trackPeerConnections(g.page);
    await shareCanvasInsteadOfScreen(h.page, CANVAS);

    if (ROM_MODE) {
      test.skip(
        !(await coreInstalled(h.page)),
        'the FCEUmm core is not installed (run deploy/scripts/fetch-emu-cores.sh)',
      );
      await routeTestRom(h.page);
    }

    try {
      const server = await voiceServer(h.context, [g.context], 'Couch');
      await joinVoice(h.page, server);
      await joinVoice(g.page, server);

      // --- Host: the game runs, then "Play together" > host it.
      if (ROM_MODE) {
        await startTestRom(h.page);
      } else {
        await startProbeCore(h.page);
      }
      const hostPanel = h.page.getByRole('region', { name: 'Voice connection' });
      await hostPanel.getByRole('button', { name: 'Play together' }).click();
      const hostDialog = h.page.getByTestId('coop-dialog');
      await expect(hostDialog.getByTestId('coop-no-rollback')).toContainText('no netplay rollback');
      await hostDialog.getByRole('button', { name: /^Host / }).click();
      await expect(hostDialog.getByText(/You are hosting/)).toBeVisible();

      // Share the game canvas through the existing screen share (the voice video path).
      await hostDialog.getByRole('button', { name: 'Share screen' }).click();
      await h.page.getByRole('button', { name: 'Go Live' }).click();
      await expect(hostPanel.getByRole('button', { name: 'Stop sharing' })).toBeVisible({
        timeout: 20_000,
      });
      await expect(h.page.getByTestId('coop-status')).toContainText('0 players');

      // --- Guest: finds the host, takes a seat.
      const guestPanel = g.page.getByRole('region', { name: 'Voice connection' });
      await guestPanel.getByRole('button', { name: 'Play together' }).click();
      const row = g.page.getByTestId('coop-host-row');
      await expect(row).toContainText('is hosting', { timeout: 20_000 });
      // The owner default: no `view_games` on this server, no seat (the host's machine checks it).
      await row.getByRole('button', { name: 'Join as a player' }).click();
      await expect(
        g.page.getByText(/Only members who may play the games on this server/),
      ).toBeVisible({ timeout: 20_000 });
      await expect(h.page.getByTestId('coop-status')).toContainText('0 players');
      // The server owner grants view_games to @everyone (its role id is the guild id); the next JOIN is seated.
      await apiPut(h.context.request, `/guilds/${server.guildId}/storage/grants`, {
        role_id: server.guildId,
        grants: ['view_games'],
      });
      await row.getByRole('button', { name: 'Join as a player' }).click();
      await g.page.keyboard.press('Escape'); // the dialog closes; the status chip is the guest's handle
      const status = g.page.getByTestId('coop-status');
      await expect(status).toContainText('Player 2', { timeout: 20_000 });
      await expect(h.page.getByTestId('coop-status')).toContainText('1 player', {
        timeout: 20_000,
      });
      // The round trip over the data channel is measured and shown.
      await expect(g.page.getByTestId('coop-latency')).toHaveText(/Input round trip \d+ ms/, {
        timeout: 20_000,
      });

      // --- Baseline: the host canvas is red, and so is the picture the guest receives.
      await expect.poll(async () => colourName(await sample(h.page, 'canvas'))).toBe('red');
      await expect
        .poll(async () => colourName(await sample(g.page, 'video')), { timeout: 30_000 })
        .toBe('red');

      // --- The guest presses A (X on the keyboard): the host frame turns green, and the guest sees it.
      await g.page.keyboard.down('KeyX');
      await expect
        .poll(async () => colourName(await sample(h.page, 'canvas')), { timeout: 10_000 })
        .toBe('green');
      await expect
        .poll(async () => colourName(await sample(g.page, 'video')), { timeout: 20_000 })
        .toBe('green');

      // Released: back to red (no stuck button).
      await g.page.keyboard.up('KeyX');
      await expect
        .poll(async () => colourName(await sample(h.page, 'canvas')), { timeout: 10_000 })
        .toBe('red');

      // --- Leaving frees the seat.
      await status.getByRole('button', { name: 'Leave the game' }).click();
      await expect(h.page.getByTestId('coop-status')).toContainText('0 players', {
        timeout: 20_000,
      });
    } finally {
      await h.context.close();
      await g.context.close();
    }
  });
});
