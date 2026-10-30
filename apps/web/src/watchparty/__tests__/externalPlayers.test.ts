import { FramePlayer, MediaElementPlayer, PLAY_CONFIRM_MS } from '../externalPlayers';

function rig(provider: 'youtube' | 'vimeo') {
  let now = 1_000;
  const sent: unknown[] = [];
  let readyCalls = 0;
  const p = new FramePlayer(provider, {
    post: (m) => sent.push(JSON.parse(m)),
    now: () => now,
    sleep: async (ms) => {
      now += ms;
    },
    ready: () => readyCalls++,
  });
  return {
    p,
    sent,
    tick: (ms: number) => (now += ms),
    ready: () => readyCalls,
    yt: (info: Record<string, unknown>) => p.onMessage({ event: 'infoDelivery', info }),
    vm: (event: string, data: Record<string, unknown> = {}) => p.onMessage({ event, data }),
  };
}

describe('FramePlayer (YouTube)', () => {
  it('extrapolates the position between reports and stops when paused', () => {
    const r = rig('youtube');
    r.yt({ currentTime: 10, playerState: 1 });
    expect(r.p.paused()).toBe(false);
    r.tick(2_000);
    expect(r.p.positionMs()).toBe(12_000);
    r.yt({ currentTime: 12.1, playerState: 2 });
    r.tick(5_000);
    expect(r.p.positionMs()).toBeCloseTo(12_100);
    expect(r.p.paused()).toBe(true);
  });

  it('announces itself once, on the first report', () => {
    const r = rig('youtube');
    expect(r.ready()).toBe(0);
    r.yt({ currentTime: 0, playerState: -1 });
    r.yt({ currentTime: 0.2, playerState: 1 });
    expect(r.ready()).toBe(1);
  });

  it('says hello after the load', () => {
    const r = rig('youtube');
    r.p.onLoad();
    expect(r.sent[0]).toMatchObject({ event: 'listening' });
  });

  it('a seek moves the reported position at once and sends the command', () => {
    const r = rig('youtube');
    r.yt({ currentTime: 1, playerState: 1 });
    r.p.seekMs(40_000);
    expect(r.p.positionMs()).toBe(40_000);
    expect(r.sent.at(-1)).toMatchObject({ func: 'seekTo', args: [40, true] });
  });

  it('play() resolves true once the frame reports playing, false when the browser refuses', async () => {
    const r = rig('youtube');
    const pending = r.p.play();
    r.yt({ currentTime: 0, playerState: 1 });
    await expect(pending).resolves.toBe(true);
    expect(r.sent.at(-1)).toMatchObject({ func: 'playVideo' });

    const blocked = rig('youtube');
    await expect(blocked.p.play()).resolves.toBe(false);
    // It gave the frame the confirmation window and no longer.
    expect(PLAY_CONFIRM_MS).toBeLessThanOrEqual(2_000);
  });

  it('pause stops the extrapolation immediately', () => {
    const r = rig('youtube');
    r.yt({ currentTime: 5, playerState: 1 });
    r.tick(1_000);
    r.p.pause();
    r.tick(3_000);
    expect(r.p.positionMs()).toBe(6_000);
    expect(r.p.paused()).toBe(true);
  });

  it('a stall is waiting, and clears when playing resumes', () => {
    const r = rig('youtube');
    const events: string[] = [];
    r.p.subscribe((e) => events.push(e));
    r.yt({ currentTime: 5, playerState: 1 });
    r.yt({ currentTime: 5, playerState: 3 });
    expect(r.p.waiting()).toBe(true);
    r.tick(1_000);
    expect(r.p.positionMs()).toBe(5_000);
    r.yt({ currentTime: 5, playerState: 1 });
    expect(r.p.waiting()).toBe(false);
    expect(events).toEqual(['playing', 'waiting', 'playing']);
  });

  it('reports the end', () => {
    const r = rig('youtube');
    const events: string[] = [];
    r.p.subscribe((e) => events.push(e));
    r.yt({ currentTime: 99, playerState: 0 });
    expect(events).toContain('ended');
  });

  it('cannot nudge its rate: it snaps to the speeds YouTube has, and says so', () => {
    const r = rig('youtube');
    expect(r.p.coarseRate).toBe(true);
    r.p.setRate(1.05);
    expect(r.sent).toEqual([]);
    r.p.setRate(1.5);
    expect(r.sent.at(-1)).toMatchObject({ func: 'setPlaybackRate', args: [1.5] });
  });

  it('a disposed player sends nothing and hears nothing', () => {
    const r = rig('youtube');
    r.p.dispose();
    r.p.pause();
    r.yt({ currentTime: 3, playerState: 1 });
    expect(r.sent).toEqual([]);
    expect(r.p.paused()).toBe(true);
  });
});

describe('FramePlayer (Vimeo)', () => {
  it('subscribes to events after ready and follows play, timeupdate and pause', () => {
    const r = rig('vimeo');
    r.p.onMessage({ event: 'ready' });
    const subs = r.sent.map((m) => (m as { method: string; value: string }).value);
    expect(subs).toEqual(expect.arrayContaining(['play', 'pause', 'timeupdate']));
    expect(r.ready()).toBe(1);
    r.vm('play', { seconds: 7 });
    r.tick(500);
    expect(r.p.positionMs()).toBe(7_500);
    r.vm('pause', { seconds: 7.6 });
    expect(r.p.paused()).toBe(true);
    expect(r.p.coarseRate).toBe(false);
  });

  it('nudges the rate continuously', () => {
    const r = rig('vimeo');
    r.p.setRate(1.05);
    expect(r.sent.at(-1)).toEqual({ method: 'setPlaybackRate', value: 1.05 });
  });

  it('a buffer end resumes whatever it was doing', () => {
    const r = rig('vimeo');
    r.vm('play', { seconds: 1 });
    r.vm('bufferstart', { seconds: 2 });
    expect(r.p.waiting()).toBe(true);
    r.vm('bufferend', { seconds: 2 });
    expect(r.p.waiting()).toBe(false);
    expect(r.p.paused()).toBe(false);
  });
});

describe('MediaElementPlayer', () => {
  function el() {
    const v = document.createElement('video');
    let paused = true;
    Object.defineProperty(v, 'paused', { get: () => paused });
    v.play = vi.fn(async () => {
      paused = false;
    });
    v.pause = vi.fn(() => {
      paused = true;
    });
    return v;
  }

  it('wraps currentTime and the rate, and forwards the element events', async () => {
    const v = el();
    const p = new MediaElementPlayer(v);
    const events: string[] = [];
    p.subscribe((e) => events.push(e));
    v.currentTime = 12;
    expect(p.positionMs()).toBe(12_000);
    p.seekMs(30_000);
    expect(v.currentTime).toBe(30);
    p.setRate(1.25);
    expect(v.playbackRate).toBe(1.25);
    expect(await p.play()).toBe(true);
    v.dispatchEvent(new Event('waiting'));
    v.dispatchEvent(new Event('playing'));
    v.dispatchEvent(new Event('ended'));
    expect(events).toEqual(['waiting', 'playing', 'ended']);
    p.dispose();
    v.dispatchEvent(new Event('ended'));
    expect(events).toHaveLength(3);
  });

  it('a refused play() is "needs a click"', async () => {
    const v = el();
    v.play = vi.fn(async () => {
      throw new DOMException('no', 'NotAllowedError');
    });
    expect(await new MediaElementPlayer(v).play()).toBe(false);
  });

  it('reports ready once metadata is there', () => {
    const v = el();
    const ready = vi.fn();
    new MediaElementPlayer(v, ready);
    expect(ready).not.toHaveBeenCalled();
    v.dispatchEvent(new Event('loadedmetadata'));
    expect(ready).toHaveBeenCalledTimes(1);
  });
});
