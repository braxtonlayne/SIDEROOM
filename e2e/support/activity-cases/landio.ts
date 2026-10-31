// ACT-LND-001 live case: Land.io, two contexts, one match. Alice started it (she hosts: the game runs in her
// browser), Bob joined the instance, both press Play. Then:
//   - Bob's page gets ticks from Alice's page (the data channel, not a server),
//   - Bob's player exists in Alice's engine and Alice's player exists in Bob's world,
//   - Bob's input is acknowledged by the host,
//   - no request leaves for a Land.io server,
//   - Alice leaving ends the instance for Bob too (P36-ACT-1: no host handover in v1).

import { expect, type Page } from '@playwright/test';
import { activityPanel, type ActivitySession } from '../activities';

export async function landioCase(s: ActivitySession) {
  const { a, b, name } = s;
  const frameA = a.frame;
  const frameB = b.frame;
  // Nothing may leave for a game server: every request from the Activity goes to its own origin's static files.
  const apiCalls: string[] = [];
  for (const p of [a.page, b.page])
    p.on('request', (r) => {
      if (r.url().includes('.activities.') && /\/api\//.test(new URL(r.url()).pathname))
        apiCalls.push(r.url());
    });
  await expect(frameA.locator('#start')).toBeVisible({ timeout: 60_000 });
  await expect(frameB.locator('#start')).toBeVisible({ timeout: 60_000 });

  // Both play.
  await frameA.locator('#play').click();
  await frameB.locator('#play').click();
  const pageFrame = (p: Page) => p.frames().find((f) => f.url().includes('.activities.'))!;
  const fa = pageFrame(a.page);
  const fb = pageFrame(b.page);
  await expect
    .poll(() => fb.evaluate(() => (window as any).__landio.screen()), { timeout: 30_000 })
    .toBe('playing');
  await expect
    .poll(() => fa.evaluate(() => (window as any).__landio.screen()), { timeout: 30_000 })
    .toBe('playing');

  // Bob's page gets ticks from Alice's: the tick counter moves, and no frames come from a server.
  const t0 = await fb.evaluate(() => (window as any).__landio.world.recvTick as number);
  await expect
    .poll(() => fb.evaluate(() => (window as any).__landio.world.recvTick as number), {
      timeout: 15_000,
    })
    .toBeGreaterThan(t0 + 14);
  // Both players are in both worlds (the roster comes from the host's engine).
  for (const f of [fa, fb]) {
    await expect
      .poll(
        () =>
          f.evaluate(() =>
            (window as any).__landio
              .players()
              .filter((p: any) => !p.bot)
              .map((p: any) => p.name)
              .sort(),
          ),
        { timeout: 20_000 },
      )
      .toHaveLength(2);
  }
  // Alice is the host, Bob is not.
  expect(await fa.evaluate(() => (window as any).__landio.world.you.host)).toBe(true);
  expect(await fb.evaluate(() => (window as any).__landio.world.you.host)).toBe(false);
  // Bob's input reaches the host and comes back acknowledged.
  const seq = await fb.evaluate(() => (window as any).__landio.transport.input({ turns: [1] }));
  await expect
    .poll(() => fb.evaluate(() => (window as any).__landio.world.you.ack[0] as number), {
      timeout: 10_000,
    })
    .toBeGreaterThanOrEqual(seq);
  // Bob sees a round-trip time through the channel, and the Activity made no calls to a game server.
  expect(
    await fb.evaluate(() => (window as any).__landio.transport.stats().polls as number),
  ).toBeGreaterThan(14);
  expect(apiCalls).toEqual([]);

  // Alice leaves the Activity: as its host she ends it for Bob too.
  await activityPanel(a.page, name).getByRole('button', { name: 'Leave', exact: true }).click();
  await expect(b.page.locator(`iframe[title="${name} (Activity)"]`)).toHaveCount(0, {
    timeout: 30_000,
  });
}
