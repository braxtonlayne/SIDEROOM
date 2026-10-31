import type { Page } from '@playwright/test';
import { expect, test, registerUser } from '../support/fixtures';
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
 *  - SR_E2E_COOP_ROM=1: the host starts the repo's self-written test ROM through the loader (ACT-EMU-001) and
 *    watches the loader's canvas. The ROM must read the SECOND controller and draw a different background
 *    while its A button is held (an NES ROM reading $4017). `startTestRom` below is the one place to adapt to
 *    the loader's UI once it has landed.
 *
 * Needs sr-server with `SR_VOICE__ENABLED=true` (SR_E2E_VOICE=1). Chromium only (fake display capture).
 */
const ROM_MODE = process.env.SR_E2E_COOP_ROM === '1';
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

/** ROM mode: open the self-written test ROM in the loader, IN-APP (a reload would drop the call). TODO(integrator): match emu-shell's UI. */
async function startTestRom(page: Page) {
  await page.getByRole('link', { name: /games/i }).first().click();
  await page
    .getByRole('button', { name: /pad test/i })
    .first()
    .click();
  await page.locator('canvas[data-testid="emu-canvas"]').waitFor({ timeout: 30_000 });
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
  return close(RED) ? 'red' : close(GREEN) ? 'green' : 'other';
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
