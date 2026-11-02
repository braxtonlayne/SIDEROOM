import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';

// ACT-A3D-001, the wave 46 gate proof: the same recorded inputs (replay/script.js, 900 ticks, two players, a box
// pyramid knocked over) give the same Jolt physics state hash in this browser as in Node. The spec runs once per
// engine (Chromium, Firefox, WebKit); every engine is compared with the same Node result, so all three agree with
// each other only when each passes. The npm Jolt build has no CROSS_PLATFORM_DETERMINISTIC option: if an engine
// disagrees, that is a finding for the owner (roadmap change 11), never a reason to switch engines here.

const arena3d = fileURLToPath(
  new URL('../../packages/starter-packs/activities/arena3d/', import.meta.url),
);

interface NodeReplay {
  final: string;
  finalExact: string;
  ticks: number;
}

let node: NodeReplay;

test.beforeAll(() => {
  const out = execFileSync(process.execPath, ['replay/run-node.mjs', '--json'], {
    cwd: arena3d,
    encoding: 'utf8',
  });
  node = JSON.parse(out) as NodeReplay;
  expect(node.final).toMatch(/^[0-9a-f]{16}$/);
});

test('the replay hash in this browser equals Node', async ({ page, browserName }, info) => {
  await page.goto('/replay/replay.html');
  const hash = page.locator('#hash');
  await expect(hash).toHaveAttribute('data-state', /^(done|error)$/, { timeout: 90_000 });
  const state = await hash.getAttribute('data-state');
  const final = (await hash.textContent())?.trim() ?? '';
  const exact = (await page.locator('#exact').textContent())?.trim() ?? '';
  const ticks = Number((await page.locator('#ticks').textContent())?.trim());
  await info.attach(`replay-${browserName}.json`, {
    body: JSON.stringify({ browserName, final, exact, ticks, node }, null, 2),
    contentType: 'application/json',
  });
  console.log(
    `arena3d replay ${browserName}: final ${final} exact ${exact} (node ${node.final} / ${node.finalExact})`,
  );
  expect(state, final).toBe('done');
  expect(ticks).toBe(node.ticks);
  expect(final).toBe(node.final);
  expect(exact).toBe(node.finalExact);
});
