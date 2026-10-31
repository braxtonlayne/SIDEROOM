/**
 * ACT-TT-001 live-spec case for the integrator: "move a token in two contexts".
 *
 * Not run by this wave (nobody but act-channel may run Playwright on 5173/8080). To wire it in, copy this file to
 * `e2e/tests/support/tabletopCase.ts` (adjust the imports) and call `tabletopMoveTokenCase` from
 * `activities-live.spec.ts` after A started the Activity and B joined it, exactly as the existing test does, but
 * with `tabletopBundle()` as the uploaded bundle instead of `testActivityBundle()`:
 *
 *   await frameA.locator('[data-testid=role]').waitFor();                 // the Activity booted on both seats
 *   await tabletopMoveTokenCase(a.page, frameA, b.page, frameB);
 *
 * What it proves: the host (A) sets a map, the guest (B) adds their token and drags it two cells, and A's board
 * shows the token on the new cell, with the roll record reaching both seats with the same seed.
 */
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { expect, type FrameLocator, type Page } from '@playwright/test';

const repoRoot = join(__dirname, '..', '..', '..', '..', '..');
const tabletopDir = join(repoRoot, 'packages', 'starter-packs', 'activities', 'tabletop');

/** The built bundle's files, ready for `zip()` of e2e/support/activityBundle.ts. */
export function tabletopBundleFiles(): Record<string, Buffer> {
  const out = join(mkdtempSync(join(tmpdir(), 'tabletop-')), 'dist');
  execFileSync('node', [join(tabletopDir, 'build.mjs'), out], { stdio: 'ignore' });
  const files: Record<string, Buffer> = {};
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const p = join(dir, name);
      if (statSync(p).isDirectory()) walk(p);
      else files[relative(out, p).split('\\').join('/')] = readFileSync(p);
    }
  };
  walk(out);
  return files;
}

/** A 600 x 400 PNG made in the page (no encoder needed here). */
async function mapPng(page: Page): Promise<number[]> {
  return page.evaluate(async () => {
    const c = document.createElement('canvas');
    c.width = 600;
    c.height = 400;
    const x = c.getContext('2d')!;
    x.fillStyle = '#4a6b3a';
    x.fillRect(0, 0, 600, 400);
    x.fillStyle = '#334455';
    x.fillRect(200, 100, 200, 200);
    const blob = await new Promise<Blob>((r) => c.toBlob((b) => r(b!), 'image/png'));
    return [...new Uint8Array(await blob.arrayBuffer())];
  });
}

interface TokenLayout {
  id: string;
  c: number;
  r: number;
  x: number;
  y: number;
}

async function tokens(frame: FrameLocator): Promise<TokenLayout[]> {
  const raw = await frame.locator('[data-testid=board]').getAttribute('data-tokens');
  return raw ? (JSON.parse(raw) as TokenLayout[]) : [];
}

export async function tabletopMoveTokenCase(
  pageA: Page,
  hostFrame: FrameLocator,
  pageB: Page,
  guestFrame: FrameLocator,
) {
  // Host: a map (the file input takes the PNG; the grid defaults are 50 px squares).
  const png = await mapPng(pageA);
  await hostFrame.locator('[data-testid=map-file]').setInputFiles({
    name: 'map.png',
    mimeType: 'image/png',
    buffer: Buffer.from(png),
  });
  await hostFrame.locator('[data-testid=map-apply]').click();

  // Guest: the map arrives (shared state through the SFU), then they add their token.
  await expect(guestFrame.locator('[data-testid=add-mine]')).toBeVisible({ timeout: 30_000 });
  await guestFrame.locator('[data-testid=add-mine]').click();
  await expect.poll(async () => (await tokens(guestFrame)).length, { timeout: 20_000 }).toBe(1);
  await expect.poll(async () => (await tokens(hostFrame)).length, { timeout: 20_000 }).toBe(1);
  const before = (await tokens(guestFrame))[0]!;

  // Guest drags it two cells to the right and one down.
  const board = guestFrame.locator('[data-testid=board]');
  const box = (await board.boundingBox())!;
  const cell = Number(await board.getAttribute('data-cell'));
  const from = { x: box.x + before.x, y: box.y + before.y };
  const to = { x: from.x + 2 * cell, y: from.y + cell };
  await pageB.mouse.move(from.x, from.y);
  await pageB.mouse.down();
  await pageB.mouse.move((from.x + to.x) / 2, (from.y + to.y) / 2, { steps: 4 });
  await pageB.mouse.move(to.x, to.y, { steps: 4 });
  await pageB.mouse.up();

  // Both boards show it on the new cell (the guest's intent went to the host, the host wrote host:tok:<id>).
  for (const frame of [hostFrame, guestFrame]) {
    await expect
      .poll(async () => (await tokens(frame))[0] && { c: (await tokens(frame))[0]!.c, r: (await tokens(frame))[0]!.r }, {
        timeout: 20_000,
      })
      .toEqual({ c: before.c + 2, r: before.r + 1 });
  }

  // Dice: the guest rolls; both seats show the same total and the same seed, and the check passes.
  await guestFrame.locator('[data-testid=dice-expr]').fill('4d6dl1');
  await guestFrame.locator('[data-testid=dice-roll]').click();
  await expect(hostFrame.locator('[data-testid=roll-seed]').first()).toHaveText(/^[0-9a-f]{16}$/, {
    timeout: 20_000,
  });
  const seedA = await hostFrame.locator('[data-testid=roll-seed]').first().textContent();
  const seedB = await guestFrame.locator('[data-testid=roll-seed]').first().textContent();
  expect(seedA).toBe(seedB);
  expect(await hostFrame.locator('[data-testid=roll-total]').first().textContent()).toBe(
    await guestFrame.locator('[data-testid=roll-total]').first().textContent(),
  );
  await expect(hostFrame.locator('.roll .good').first()).toBeVisible();
}
