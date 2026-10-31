import { expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from '../support/world';

/**
 * ACT-EMU-001: the emulator in a real browser. Boots the NES core (FCEUmm, fetched by `fetch-emu-cores.sh` into
 * `apps/web/public/emu/cores`, never committed) with the test ROM this repository generates itself
 * (`scripts/make-test-rom.mjs`, committed at `e2e/fixtures/emu/sideroom-test.nes`; no ROM is ever downloaded), and
 * samples pixels of the canvas: a non-blank frame (red), then blue while player 1 holds A, back to red on release.
 * Saving a state PUTs to the member's own bucket under `emu/saves/fceumm/<rom-hash>/`.
 *
 * Signed in as the Owner on the full stack; the ROM list, the mount and the bucket are served by route handlers so no
 * real bucket is needed (the bucket is `https://bucket.test`). The case skips itself when the core is not installed.
 */
test.use({ bypassCSP: true });

const here = dirname(fileURLToPath(import.meta.url));
const ROM = readFileSync(resolve(here, '../fixtures/emu/sideroom-test.nes'));
const ROM_KEY = 'emu/roms/sideroom-test.nes';
const CORE_URL = '/emu/cores/fceumm/fceumm-legacy-wasm.data';
const BUCKET = 'https://bucket.test';

const mount = {
  id: '7',
  owner_kind: 'user',
  owner_id: '1',
  label: 'My games bucket',
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

/** The colour of the centre of the game canvas, read in the page right after a frame. */
const sample = (page: Page) =>
  page.evaluate(
    () =>
      new Promise<[number, number, number]>((done) => {
        const canvas = document.querySelector<HTMLCanvasElement>(
          'canvas[data-testid="emu-canvas"]',
        );
        requestAnimationFrame(() => {
          const copy = document.createElement('canvas');
          copy.width = 16;
          copy.height = 16;
          const ctx = copy.getContext('2d');
          if (!canvas || !ctx) return done([0, 0, 0]);
          ctx.drawImage(canvas, 0, 0, 16, 16);
          const d = ctx.getImageData(8, 8, 1, 1).data;
          done([d[0] ?? 0, d[1] ?? 0, d[2] ?? 0]);
        });
      }),
  );

const isRed = ([r, g, b]: number[]) => r! > 150 && g! < 90 && b! < 60;
const isBlue = ([r, , b]: number[]) => b! > 180 && r! < 90;

test('the NES core boots the test ROM, shows a non-blank frame, answers the keyboard and saves a state', async ({
  solo,
}) => {
  const page = solo.a.page;
  const core = await page.request.get(CORE_URL);
  const magic = core.ok() ? [...(await core.body()).subarray(0, 6)] : [];
  test.skip(
    magic.join() !== [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c].join(),
    'the FCEUmm core is not installed (run deploy/scripts/fetch-emu-cores.sh)',
  );

  const puts: Array<{ key: string; size: number }> = [];
  await page.route('**/api/v1/users/@me/games/roms', (route) =>
    route.fulfill({
      json: {
        roms: [
          {
            id: '501',
            mount_id: '7',
            key: ROM_KEY,
            size: ROM.length,
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
    const url = new URL(req.url());
    const key = decodeURI(url.pathname.replace(/^\/(GET|PUT)\//, ''));
    if (req.method() === 'PUT') {
      puts.push({ key, size: req.postDataBuffer()?.length ?? 0 });
      return route.fulfill({ status: 200, headers });
    }
    if (key === ROM_KEY) return route.fulfill({ status: 200, headers, body: ROM });
    return route.fulfill({ status: 404, headers });
  });

  await page.goto('/games');
  await expect(page.getByRole('heading', { name: 'Games', level: 1 })).toBeVisible();
  await expect(page.getByText('sideroom test')).toBeVisible();
  await page.getByRole('button', { name: 'Play' }).click();

  const canvas = page.locator('canvas[data-testid="emu-canvas"]');
  await expect(canvas).toHaveAttribute('data-phase', 'running', { timeout: 60_000 });

  // A non-blank frame: the ROM paints the whole screen red.
  await expect.poll(async () => isRed(await sample(page)), { timeout: 30_000 }).toBe(true);

  // Player 1 holds A (the X key in the default mapping): the screen turns blue, then red again.
  await canvas.focus();
  await page.keyboard.down('KeyX');
  await expect.poll(async () => isBlue(await sample(page)), { timeout: 30_000 }).toBe(true);
  await page.keyboard.up('KeyX');
  await expect.poll(async () => isRed(await sample(page)), { timeout: 30_000 }).toBe(true);

  // Save state 1 goes to the member's own bucket under emu/saves/<core>/<rom-hash>/.
  await page.getByRole('button', { name: 'Save state 1' }).click();
  await expect(page.getByTestId('emu-save-status')).toContainText('Saved');
  expect(puts.length).toBeGreaterThan(0);
  const state = puts.find((p) => p.key.endsWith('/state1.state'));
  expect(state?.key).toMatch(/^emu\/saves\/fceumm\/[0-9a-f]{40}\/state1\.state$/);
  expect(state?.size).toBeGreaterThan(1000);

  // The mapping screen opens and lists the controls.
  await page.getByRole('button', { name: 'Controls' }).click();
  await expect(page.getByRole('dialog', { name: 'Controls' })).toBeVisible();
  await expect(page.getByRole('cell', { name: 'KeyX' })).toBeVisible();
});

test('a member without a bucket of their own is told that saving is off, and the core is missing message is clear', async ({
  solo,
}) => {
  const page = solo.a.page;
  await page.route('**/api/v1/users/@me/games/roms', (route) =>
    route.fulfill({ json: { roms: [] } }),
  );
  await page.route('**/api/v1/users/@me/storage/mounts', (route) => route.fulfill({ json: [] }));
  await page.goto('/games');
  await expect(page.getByRole('heading', { name: 'No games yet' })).toBeVisible();
});
