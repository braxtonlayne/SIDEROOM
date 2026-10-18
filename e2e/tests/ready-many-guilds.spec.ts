import { test, expect, apiPost } from '../support/fixtures';
import { PUBLIC_URL } from '../support/cli';

/**
 * K-22 (P15-BRW-2): with an account in about 60 servers, the time from the gateway READY frame to the first
 * paint of the open channel's messages. Target: under 2 s on the dev build. Creating 60 servers is slow, so
 * the spec runs only with SR_E2E_K22=1; it prints the numbers and fails above the target.
 */
const GUILDS = Number(process.env.SR_E2E_K22_GUILDS ?? 60);

test.skip(!process.env.SR_E2E_K22, 'set SR_E2E_K22=1 to measure READY with many servers (K-22)');
// Server creation is rate limited per account (about 2 a minute after a burst), so 60 take about 25 minutes.
test.setTimeout(2_700_000);

test('first channel paint after READY with many servers (K-22)', async ({ seedUser, signedIn }) => {
  // SR_E2E_K22_USER reuses an account an earlier run filled (seeded password), which saves the slow setup.
  const reuse = process.env.SR_E2E_K22_USER;
  const s = await signedIn(
    reuse
      ? { id: '', username: reuse, email: '', password: 'Seeded-Passw0rd-9!' }
      : await seedUser('k22'),
  );
  type Created = { id: string; channels: { id: string; type: number }[] };
  // Server creation is rate limited: wait out `retry_after` instead of failing.
  const create = async (name: string): Promise<Created> => {
    for (;;) {
      try {
        return await apiPost<Created>(s.context.request, '/guilds', { name });
      } catch (e) {
        const wait = /-> 429:.*"retry_after":([d.]+)/.exec(String(e))?.[1];
        if (!wait) throw e;
        await new Promise((r) => setTimeout(r, Number(wait) * 1000 + 250));
      }
    }
  };
  const mine = (await (
    await s.context.request.get('/api/v1/users/@me/guilds', {
      headers: { Origin: new URL(PUBLIC_URL).origin },
    })
  ).json()) as { id: string }[];
  for (let i = mine.length; i < GUILDS; i++) mine.push(await create(`K22 ${i}`));
  const targetGuild = mine[Math.floor(GUILDS / 2)]!.id;
  const channels = (await (
    await s.context.request.get(`/api/v1/guilds/${targetGuild}/channels`, {
      headers: { Origin: new URL(PUBLIC_URL).origin },
    })
  ).json()) as { id: string; type: number }[];
  const text = channels.find((c) => c.type === 0);
  if (!text) throw new Error('the target server has no text channel');
  const target = { guild: targetGuild, channel: text.id };
  await apiPost(s.context.request, `/channels/${target.channel}/messages`, {
    content: 'k22 paint marker',
  });

  // Timestamps on the test's clock (node): READY when the gateway frame arrives at the page (the socket's
  // frame event), the channel's marker message in the DOM, and the first animation frame the page answers
  // after that (the main thread is free again). An init script was tried first; Playwright's transform of a
  // function-valued script broke it silently, and the frame event needs no code in the page.
  let readyAt: number | undefined;
  s.page.on('websocket', (ws) => {
    if (!ws.url().includes('/gateway')) return;
    ws.on('framereceived', (f) => {
      if (
        readyAt === undefined &&
        typeof f.payload === 'string' &&
        f.payload.includes('"t":"READY"')
      )
        readyAt = Date.now();
    });
  });
  const marker = s.page.locator('[role="log"] article', { hasText: 'k22 paint marker' }).first();

  // One warm-up load (the dev server transforms the Lounge on first use), then three measured loads.
  const loads: { ready: number; paint: number; idle: number }[] = [];
  for (let run = 0; run < 4; run++) {
    readyAt = undefined;
    const start = Date.now();
    await s.page.goto(`/channels/${target.guild}/${target.channel}`);
    await marker.waitFor({ timeout: 60_000 });
    const paint = Date.now();
    await expect.poll(() => readyAt, { timeout: 30_000 }).toBeDefined();
    // A frame the page renders after READY: it waits for the READY / GUILD_CREATE handling to finish.
    await s.page.evaluate(
      () => new Promise((r) => setTimeout(() => requestAnimationFrame(() => r(null)), 40)),
    );
    const idle = Date.now();
    if (run > 0) loads.push({ ready: readyAt! - start, paint: paint - start, idle: idle - start });
  }
  // READY to the first channel paint; when the channel painted from REST before READY, READY to the first
  // frame the page renders after READY (40 ms in, past the 30 ms burst) shows it is not stuck behind the READY handling.
  const ms = loads.map((t) => Math.max(t.paint, t.idle) - t.ready);
  const fmt = (xs: number[]) => xs.map((x) => x.toFixed(0)).join(', ');
  console.log(
    `K-22: ${mine.length} servers (${s.user.username}); from navigation: READY ${fmt(loads.map((t) => t.ready))} ms, ` +
      `channel paint ${fmt(loads.map((t) => t.paint))} ms, frame after READY ${fmt(loads.map((t) => t.idle))} ms; ` +
      `READY -> first channel paint ${fmt(ms)} ms`,
  );
  const median = [...ms].sort((x, y) => x - y)[1]!;
  expect(median).toBeLessThan(2000);
});
