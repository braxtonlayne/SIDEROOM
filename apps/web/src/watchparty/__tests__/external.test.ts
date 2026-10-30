// Watch Together over a provider frame and a media element (PRF-UNF-001): the real sync controller drives a
// `FramePlayer` through the external host, against the same in-memory server the Cinema tests use. The provider
// is a fake: it answers the documented postMessage commands and reports its time like YouTube does.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PartyController, TICK_MS } from '../controller';
import { ExternalHost } from '../externalHost';
import { FramePlayer } from '../externalPlayers';
import type { PartyMedia } from '../types';
import { FakeServer, deps } from './fakes';

const EMBED: PartyMedia = {
  title_id: '0',
  episode_id: '0',
  variant_id: null,
  asset_id: '0',
  title: 'A song',
  episode_label: null,
  duration_ms: null,
  age_restricted: false,
  live: false,
  kind: 'embed',
  url: 'https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ',
  provider: 'youtube',
};

/** A fake YouTube iframe: obeys the command messages, reports `infoDelivery` with its own media clock. */
class FakeYouTube {
  playing = false;
  pos = 0;
  at = Date.now();
  commands: { func: string; args: unknown[] }[] = [];
  player: FramePlayer;
  /** How much faster the media clock runs than real time, to force drift. */
  drift = 0;

  constructor(readonly host: ExternalHost) {
    this.player = new FramePlayer('youtube', {
      post: (raw) => this.receive(JSON.parse(raw)),
      now: () => Date.now(),
      sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
    });
    host.attach(this.player);
  }

  private settle() {
    const t = Date.now();
    if (this.playing) this.pos += (t - this.at) * (1 + this.drift);
    this.at = t;
  }

  now() {
    this.settle();
    return this.pos;
  }

  private receive(m: { event?: string; func?: string; args?: unknown[] }) {
    if (m.event !== 'command' || !m.func) return;
    this.commands.push({ func: m.func, args: m.args ?? [] });
    this.settle();
    if (m.func === 'playVideo') this.playing = true;
    if (m.func === 'pauseVideo') this.playing = false;
    if (m.func === 'seekTo') this.pos = (m.args?.[0] as number) * 1000;
    // The provider answers a moment later, like a real iframe.
    setTimeout(() => this.report(), 50);
  }

  report() {
    this.settle();
    this.player.onMessage({
      event: 'infoDelivery',
      info: { currentTime: this.pos / 1000, playerState: this.playing ? 1 : 2, playbackRate: 1 },
    });
  }
}

const SKEW = 3_000;
let server: FakeServer;
let host: ExternalHost;
let c: PartyController;

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(1_700_000_000_000);
  server = new FakeServer(() => Date.now() + SKEW, '1', { host: true, media: EMBED });
  host = new ExternalHost();
  c = new PartyController(
    deps(server, host.player, {
      // The real runtime loads the media into the host; the stage attaches the player once allowed.
      load: async (m: PartyMedia) => host.setMedia(m),
    }),
  );
});
afterEach(async () => {
  await c.close();
  host.reset();
  vi.useRealTimers();
});

const tick = (ms: number) => vi.advanceTimersByTimeAsync(ms);

describe('the host and the gate', () => {
  it('loading the party media registers it with the host but allows nothing', async () => {
    await c.open('900');
    expect(host.store.getState()).toMatchObject({
      media: { kind: 'embed', url: EMBED.url },
      allowed: false,
      attached: false,
    });
    // Without a backend the player is inert and asks for a click, which the theatre already shows.
    expect(host.player.paused()).toBe(true);
    expect(await host.player.play()).toBe(false);
  });

  it('allow() is the click; a media change closes the gate again and releases the frame', async () => {
    await c.open('900');
    host.allow();
    expect(host.store.getState().allowed).toBe(true);
    const yt = new FakeYouTube(host);
    expect(host.store.getState().attached).toBe(true);
    host.setMedia({ ...EMBED, url: 'https://www.youtube-nocookie.com/embed/aaaaaaaaaaa' });
    expect(host.store.getState()).toMatchObject({ allowed: false, attached: false });
    yt.player.pause();
    expect(yt.commands).toEqual([]);
  });
});

describe('synchronised playback through the frame', () => {
  async function playing() {
    await c.open('900');
    host.allow();
    const yt = new FakeYouTube(host);
    await c.reapply();
    server.change({ playing: true });
    server.emitPlayback('play', '1');
    await tick(200);
    return yt;
  }

  it('the party play reaches the frame as playVideo and the frame lands at the shared position', async () => {
    server.change({ position_ms: 20_000 });
    const yt = await playing();
    expect(yt.commands.map((x) => x.func)).toContain('playVideo');
    expect(yt.commands.find((x) => x.func === 'seekTo')?.args[0]).toBeCloseTo(20, 0);
    expect(yt.playing).toBe(true);
    expect(c.state.needsGesture).toBe(false);
    expect(Math.abs(yt.now() - server.position())).toBeLessThan(500);
  });

  it('a pause from the controls pauses the frame', async () => {
    const yt = await playing();
    server.change({ playing: false });
    server.emitPlayback('pause', '1');
    await tick(200);
    expect(yt.commands.at(-1)?.func).toBe('pauseVideo');
    expect(yt.playing).toBe(false);
  });

  it('a seek is applied once, as seekTo', async () => {
    const yt = await playing();
    const before = yt.commands.length;
    server.change({ position_ms: 90_000 });
    server.emitPlayback('seek', '1');
    await tick(400);
    const seeks = yt.commands.slice(before).filter((x) => x.func === 'seekTo');
    expect(seeks).toHaveLength(1);
    expect(seeks[0]!.args[0]).toBeCloseTo(90, 0);
  });

  it('YouTube cannot nudge its rate, so a few hundred ms of drift is corrected with a hop', async () => {
    const yt = await playing();
    expect(host.player.coarseRate).toBe(true);
    // The frame’s clock runs 3 % fast: it gains 30 ms per second.
    yt.drift = 0.03;
    const before = yt.commands.filter((x) => x.func === 'seekTo').length;
    for (let i = 0; i < 40; i++) {
      yt.report();
      await tick(TICK_MS);
    }
    const hops = yt.commands.filter((x) => x.func === 'seekTo').length - before;
    expect(hops).toBeGreaterThanOrEqual(1);
    expect(yt.commands.some((x) => x.func === 'setPlaybackRate')).toBe(false);
    expect(Math.abs(yt.now() - server.position())).toBeLessThan(900);
  });

  it('two clients of one party end up within the drift tolerance of each other', async () => {
    const hostB = new ExternalHost();
    const cB = new PartyController({
      ...deps(server, hostB.player, { load: async (m: PartyMedia) => hostB.setMedia(m) }),
      me: () => '2',
    });
    try {
      const a = await playing();
      await cB.open('900');
      hostB.allow();
      const b = new FakeYouTube(hostB);
      void cB.reapply();
      await tick(1_000);
      for (let i = 0; i < 10; i++) {
        a.report();
        b.report();
        await tick(TICK_MS);
      }
      expect(Math.abs(a.now() - b.now())).toBeLessThan(600);
      expect(a.playing && b.playing).toBe(true);
    } finally {
      await cB.close();
      hostB.reset();
    }
  });

  it('a stall of the frame is reported as buffering for the whole party', async () => {
    const yt = await playing();
    yt.player.onMessage({ event: 'onStateChange', info: 3 });
    await tick(2_000);
    expect(c.state.localBuffering).toBe(true);
    yt.player.onMessage({ event: 'onStateChange', info: 1 });
    await tick(100);
    expect(c.state.localBuffering).toBe(false);
  });
});
