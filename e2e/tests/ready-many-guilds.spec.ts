import { test, expect, apiPost } from '../support/fixtures';

/**
 * K-22 (P15-BRW-2): with an account in about 60 servers, the time from the gateway READY frame to the first
 * paint of the open channel's messages. Target: under 2 s on the dev build. Creating 60 servers is slow, so
 * the spec runs only with SR_E2E_K22=1; it prints the numbers and fails above the target.
 */
const GUILDS = Number(process.env.SR_E2E_K22_GUILDS ?? 60);

test.skip(!process.env.SR_E2E_K22, 'set SR_E2E_K22=1 to measure READY with many servers (K-22)');
test.setTimeout(600_000);

test('first channel paint after READY with many servers (K-22)', async ({ seedUser, signedIn }) => {
  const s = await signedIn(await seedUser('k22'));
  let target: { guild: string; channel: string } | null = null;
  for (let i = 0; i < GUILDS; i++) {
    const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
      s.context.request,
      '/guilds',
      { name: `K22 ${i}` },
    );
    const text = guild.channels.find((c) => c.type === 0);
    if (!text) throw new Error('new server has no text channel');
    if (i === Math.floor(GUILDS / 2)) target = { guild: guild.id, channel: text.id };
  }
  if (!target) throw new Error('no target channel');
  await apiPost(s.context.request, `/channels/${target.channel}/messages`, {
    content: 'k22 paint marker',
  });

  // Timestamps on the page's own clock: READY as the socket delivers it, and the first frame in which the
  // channel's message is in the DOM.
  await s.page.addInitScript(() => {
    const w = window as unknown as { __k22: { ready?: number; paint?: number } };
    w.__k22 = {};
    const Native = window.WebSocket;
    class Timed extends Native {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        this.addEventListener('message', (e: MessageEvent) => {
          if (
            w.__k22.ready === undefined &&
            typeof e.data === 'string' &&
            e.data.includes('"t":"READY"')
          )
            w.__k22.ready = performance.now();
        });
      }
    }
    window.WebSocket = Timed as typeof WebSocket;
    const watch = () => {
      if (w.__k22.paint !== undefined) return;
      const hit = [...document.querySelectorAll('[role="log"] article')].some((a) =>
        a.textContent?.includes('k22 paint marker'),
      );
      if (hit) w.__k22.paint = performance.now();
      else requestAnimationFrame(watch);
    };
    requestAnimationFrame(watch);
  });

  const runs: number[] = [];
  for (let run = 0; run < 3; run++) {
    await s.page.goto(`/channels/${target.guild}/${target.channel}`);
    await expect(s.page.getByText('k22 paint marker')).toBeVisible({ timeout: 60_000 });
    const t = await s.page.evaluate(
      () => (window as unknown as { __k22: { ready?: number; paint?: number } }).__k22,
    );
    if (t.ready === undefined || t.paint === undefined)
      throw new Error(`no timings: ${JSON.stringify(t)}`);
    runs.push(Math.max(0, t.paint - t.ready));
  }
  console.log(
    `K-22: ${GUILDS} servers, READY -> first channel paint (ms): ${runs.map((r) => r.toFixed(0)).join(', ')}`,
  );
  const median = [...runs].sort((a, b) => a - b)[1]!;
  expect(median).toBeLessThan(2000);
});
