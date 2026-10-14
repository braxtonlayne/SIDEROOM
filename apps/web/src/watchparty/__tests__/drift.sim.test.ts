// WP-FE-013: ten real sync controllers against one in-memory server, each with its own clock skew, media clock
// error, network latency (asymmetric, jittery) and staggered join. Fake timers drive a few simulated minutes.
// Nothing here is a real stream: the "player" is a clock that can run a little fast or slow, which is exactly
// the thing the drift correction exists for.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PartyController } from '../controller';
import { FakePlayer, FakeServer, deps } from './fakes';

const N = 10;

/** Small deterministic PRNG so a failure reproduces. */
function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

interface Client {
  id: number;
  c: PartyController;
  player: FakePlayer;
  skew: number;
}

let server: FakeServer;
let clients: Client[];
const rand = rng(12345);

const step = (ms: number) => vi.advanceTimersByTimeAsync(ms);

function makeClient(i: number): Client {
  const skew = Math.round((rand() - 0.5) * 6_000); // browser clock up to 3 s off
  const now = () => Date.now() + skew;
  const player = new FakePlayer(() => Date.now());
  // The media clock of a real <video> runs within a few hundredths of a percent of wall time.
  player.speedError = (rand() - 0.5) * 0.0008;
  const up = 10 + Math.round(rand() * 90);
  const down = 10 + Math.round(rand() * 90);
  const lag = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
  const base = server.api();
  const api = {
    ...base,
    // Server clock sample with asymmetric latency: the request takes `up`, the answer `down`.
    time: async () => {
      await lag(up);
      const t = server.now();
      await lag(down);
      return { server_time_ms: t };
    },
    heartbeat: async (id: string, b: Parameters<typeof base.heartbeat>[1]) => {
      await lag(up);
      const r = await base.heartbeat(id, b);
      await lag(down);
      return r;
    },
    command: async (id: string, cmd: Parameters<typeof base.command>[1]) => {
      await lag(up);
      const r = await base.command(id, cmd);
      await lag(down);
      return r;
    },
  };
  const d = deps(server, player, {
    api,
    now,
    me: () => String(i + 1),
    // Gateway events reach each client after its own latency.
    onDispatch: (fn) => {
      const l = (event: string, data: unknown) => void setTimeout(() => fn(event, data), down);
      server.listeners.add(l);
      return () => void server.listeners.delete(l);
    },
  });
  d.load.mockImplementation(async (_m: unknown, startMs: number) => {
    player.seekMs(startMs);
  });
  return { id: i, c: new PartyController(d), player, skew };
}

/** Largest deviation from the server's position, and the largest gap between any two viewers. */
function spread() {
  const truth = server.position();
  const pos = clients.map((x) => x.player.positionMs());
  return {
    maxDev: Math.max(...pos.map((p) => Math.abs(p - truth))),
    pairwise: Math.max(...pos) - Math.min(...pos),
  };
}

beforeEach(async () => {
  vi.useFakeTimers();
  vi.setSystemTime(1_700_000_000_000);
  server = new FakeServer(() => Date.now(), '1', { host: true });
  clients = Array.from({ length: N }, (_, i) => makeClient(i));
  // Staggered joins over 20 s while the film is already running.
  server.change({ playing: true });
  for (const x of clients) {
    const open = x.c.open('900');
    await step(2_000);
    await open;
  }
});

afterEach(async () => {
  for (const x of clients) await x.c.close();
  vi.useRealTimers();
});

describe('ten viewers on mixed networks', () => {
  it('all start playing and every browser clock offset is recovered to within the network asymmetry', () => {
    for (const x of clients) {
      expect(x.c.state.status).toBe('active');
      expect(x.player.paused()).toBe(false);
      // True offset between the server clock (Date.now) and this browser's clock is -skew.
      expect(Math.abs(x.c.state.offsetMs + x.skew)).toBeLessThanOrEqual(60);
    }
  });

  it('stay within 300 ms of the server position for ten minutes, and within 300 ms of each other', async () => {
    await step(60_000); // settle after the staggered joins
    let worstDev = 0;
    let worstPair = 0;
    for (let t = 0; t < 600; t++) {
      await step(1_000);
      const s = spread();
      worstDev = Math.max(worstDev, s.maxDev);
      worstPair = Math.max(worstPair, s.pairwise);
    }
    expect(worstDev).toBeLessThanOrEqual(300);
    expect(worstPair).toBeLessThanOrEqual(300);
  });

  it('follow a host seek within 3 seconds', async () => {
    await step(30_000);
    void clients[0]!.c.seek(3_000_000);
    await step(3_000);
    const s = spread();
    expect(s.maxDev).toBeLessThanOrEqual(400);
    expect(server.position()).toBeGreaterThan(3_000_000);
  });

  it('pause and resume together, then stay together', async () => {
    await step(30_000);
    void clients[0]!.c.toggle();
    await step(2_000);
    for (const x of clients) expect(x.player.paused()).toBe(true);
    const frozen = clients.map((x) => x.player.positionMs());
    expect(Math.max(...frozen) - Math.min(...frozen)).toBeLessThanOrEqual(300);
    await step(20_000);
    void clients[0]!.c.toggle();
    await step(5_000);
    for (const x of clients) expect(x.player.paused()).toBe(false);
    await step(30_000);
    expect(spread().maxDev).toBeLessThanOrEqual(300);
  });

  it('recover from short network stalls without a hard seek', async () => {
    await step(30_000);
    // Four viewers lose 400 to 900 ms of playback to a stall (shorter than the 1.5 s the server cares about).
    const hit = clients.slice(1, 5);
    for (const [i, x] of hit.entries()) {
      x.player.isWaiting = true;
      await step(400 + i * 170);
      x.player.isWaiting = false;
    }
    const before = hit.map((x) => x.player.seeks.length);
    let converged = -1;
    for (let t = 1; t <= 40; t++) {
      await step(1_000);
      if (spread().maxDev <= 300) {
        converged = t;
        break;
      }
    }
    expect(converged).toBeGreaterThan(0);
    expect(converged).toBeLessThanOrEqual(25);
    // Under 2 s of drift is corrected by nudging the rate only.
    expect(hit.map((x) => x.player.seeks.length)).toEqual(before);
  });

  it('bring a viewer who lost several seconds back with a single seek', async () => {
    await step(30_000);
    const late = clients[3]!;
    late.player.seekMs(late.player.positionMs() - 6_000);
    const before = late.player.seeks.length;
    await step(3_000);
    expect(late.player.seeks.length - before).toBe(1);
    expect(spread().maxDev).toBeLessThanOrEqual(400);
  });
});
