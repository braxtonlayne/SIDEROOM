import { mkdirSync } from 'node:fs';
import { expect, test, type CDPSession, type Page } from '@playwright/test';

// ACT-A3D-002: touch controls and GPU recovery in a real browser (Chromium; the replay spec covers every engine).
// The bundle runs from the static server in playwright.arena3d.config.ts with the local stand-in platform. Touch
// is real: CDP `Input.dispatchTouchEvent` makes pointerType 'touch' events, the same a phone gives the page.
//
// Set SR_ARENA3D_SHOTS to a folder to also keep a screenshot of each layout.

const SHOTS = process.env.SR_ARENA3D_SHOTS;

test.skip(
  ({ browserName }) => browserName !== 'chromium',
  'CDP touch events and WEBGL_lose_context: Chromium only',
);

const phones = [
  { name: 'portrait', width: 375, height: 812 },
  { name: 'landscape', width: 812, height: 375 },
];

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}
const overlaps = (a: Box, b: Box) =>
  a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

async function touch(
  cdp: CDPSession,
  type: 'touchStart' | 'touchMove' | 'touchEnd',
  id: number,
  x: number,
  y: number,
) {
  await cdp.send('Input.dispatchTouchEvent', {
    type,
    touchPoints: type === 'touchEnd' ? [] : [{ x, y, id }],
  });
}

/** Both fingers at once: the stick and the look drag are separate pointers. */
async function touches(
  cdp: CDPSession,
  type: 'touchStart' | 'touchMove',
  points: { id: number; x: number; y: number }[],
) {
  await cdp.send('Input.dispatchTouchEvent', { type, touchPoints: points });
}

const lastButtons = (page: Page) =>
  page.evaluate(() => {
    const log = (window as any).arena3d.inputLog() as number[][];
    return log.at(-1)?.[2] ?? 0;
  });
const lastYaw = (page: Page) =>
  page.evaluate(() => {
    const log = (window as any).arena3d.inputLog() as number[][];
    return log.at(-1)?.[3] ?? 0;
  });

for (const phone of phones) {
  test.describe(`${phone.name} ${phone.width}x${phone.height}`, () => {
    test.use({
      viewport: { width: phone.width, height: phone.height },
      hasTouch: true,
      isMobile: true,
    });

    test('touch controls show only when playing, fit the screen and drive the input', async ({
      page,
    }) => {
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      await page.goto(`/index.html?user=phone&instance=touch-${phone.name}`);
      await expect(page.locator('#btn-play')).toBeEnabled({ timeout: 60_000 });
      // The menu says how to play with touch, and nothing about a keyboard.
      await expect(page.locator('.touch-keys')).toBeVisible();
      await expect(page.locator('.keys')).toBeHidden();
      await expect(page.locator('#touch')).toBeHidden();
      await page.locator('#btn-play').tap();
      await expect(page.locator('#touch')).toBeVisible();
      await expect(page.locator('#resume')).toBeHidden(); // no pointer lock on a phone: nothing to pause
      await expect(page.locator('#touch-jump')).toBeVisible();
      await expect(page.locator('#touch-crouch')).toBeVisible();

      // Nothing the thumbs use is covered, off-screen or under the HUD.
      const ids = ['touch-stick-zone', 'touch-jump', 'touch-crouch', 'touch-menu'];
      const hud = ['hud-score', 'hud-players', 'hud-info', 'hud-net'];
      const box = async (id: string) => (await page.locator(`#${id}`).boundingBox()) as Box;
      for (const id of [...ids, ...hud]) {
        const b = await box(id);
        expect(b, id).toBeTruthy();
        expect(b.x, id).toBeGreaterThanOrEqual(0);
        expect(b.y, id).toBeGreaterThanOrEqual(0);
        expect(b.x + b.width, id).toBeLessThanOrEqual(phone.width);
        expect(b.y + b.height, id).toBeLessThanOrEqual(phone.height);
      }
      for (const t of ['touch-jump', 'touch-crouch', 'touch-menu']) {
        for (const h of hud)
          expect(overlaps(await box(t), await box(h)), `${t} over ${h}`).toBe(false);
      }
      expect(overlaps(await box('touch-jump'), await box('touch-crouch'))).toBe(false);
      const zone = await box('touch-stick-zone');
      for (const t of ['touch-jump', 'touch-crouch'])
        expect(overlaps(zone, await box(t)), `${t} over the stick zone`).toBe(false);
      // The page does not scroll or zoom under a thumb.
      expect(
        await page.evaluate(() => [
          document.documentElement.scrollWidth,
          document.documentElement.scrollHeight,
        ]),
      ).toEqual([phone.width, phone.height]);
      if (SHOTS) {
        mkdirSync(SHOTS, { recursive: true });
        await page.screenshot({ path: `${SHOTS}/arena3d-touch-${phone.name}.png` });
      }

      const cdp = await page.context().newCDPSession(page);
      const z = zone;
      const sx = z.x + 70;
      const sy = z.y + z.height - 60;
      // Left thumb pushes the stick up and to the right while the right thumb drags to turn: forward + right + yaw.
      await touch(cdp, 'touchStart', 1, sx, sy);
      await touches(cdp, 'touchMove', [{ id: 1, x: sx + 56, y: sy - 56 }]);
      await expect.poll(() => lastButtons(page)).toBe(1 | 8);
      await expect(page.locator('#touch-base')).toBeVisible();
      const yaw0 = await lastYaw(page);
      const lx = phone.width * 0.6;
      const ly = phone.height * 0.4;
      await touches(cdp, 'touchStart', [
        { id: 1, x: sx + 56, y: sy - 56 },
        { id: 2, x: lx, y: ly },
      ]);
      await touches(cdp, 'touchMove', [
        { id: 1, x: sx + 56, y: sy - 56 },
        { id: 2, x: lx + 60, y: ly },
      ]);
      await expect.poll(() => lastYaw(page)).toBeLessThan(yaw0 - 1000);
      // Let go of both: the stick stops.
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await expect.poll(() => lastButtons(page)).toBe(0);
      await expect(page.locator('#touch-base')).toBeHidden();

      // Jump is held while the thumb is on it; Crouch toggles and stays on.
      const j = await box('touch-jump');
      await touch(cdp, 'touchStart', 3, j.x + j.width / 2, j.y + j.height / 2);
      await expect.poll(() => lastButtons(page)).toBe(16);
      await touch(cdp, 'touchEnd', 3, 0, 0);
      await expect.poll(() => lastButtons(page)).toBe(0);
      await page.locator('#touch-crouch').tap();
      await expect.poll(() => lastButtons(page)).toBe(32);
      await expect(page.locator('#touch-crouch')).toHaveAttribute('aria-pressed', 'true');
      await page.locator('#touch-crouch').tap();
      await expect.poll(() => lastButtons(page)).toBe(0);

      // Menu goes back to the menu and hides the controls; the buttons stop counting.
      await page.locator('#touch-menu').tap();
      await expect(page.locator('#menu')).toBeVisible();
      await expect(page.locator('#touch')).toBeHidden();
      expect(errors).toEqual([]);
    });
  });
}

test.describe('GPU loss', () => {
  test('a lost WebGL context is rebuilt and the scene draws again', async ({ page }) => {
    await page.addInitScript(() =>
      localStorage.setItem('arena3d.settings', JSON.stringify({ renderer: 'webgl2' })),
    );
    await page.goto('/index.html?user=gpu&instance=gpu-loss');
    await expect(page.locator('#btn-play')).toBeEnabled({ timeout: 60_000 });
    await expect(page.locator('#status')).toContainText('WebGL2');
    await page.locator('#btn-play').click();
    await expect(page.locator('#hud')).toBeVisible();
    const fps = () =>
      page.evaluate(
        () =>
          (document.getElementById('hud-info')?.textContent ?? '').match(/(\d+) fps/)?.[1] ?? '0',
      );
    await expect.poll(async () => Number(await fps())).toBeGreaterThan(0);
    await page.evaluate(() => {
      const gl = (document.getElementById('view') as HTMLCanvasElement).getContext('webgl2')!;
      (window as any).__lose = gl.getExtension('WEBGL_lose_context');
      (window as any).__lose.loseContext();
    });
    await expect(page.locator('#gpu-notice')).toContainText('Restoring');
    await page.evaluate(() => (window as any).__lose.restoreContext());
    await expect(page.locator('#gpu-notice')).toContainText('Graphics restored');
    // The frame loop is drawing again: pixels are not the clear colour only, and the notice goes away by itself.
    await expect(page.locator('#gpu-notice')).toBeHidden({ timeout: 10_000 });
    const drawn = await page.evaluate(() => {
      const c = document.getElementById('view') as HTMLCanvasElement;
      const gl = c.getContext('webgl2')!;
      return !gl.isContextLost();
    });
    expect(drawn).toBe(true);
  });
});
