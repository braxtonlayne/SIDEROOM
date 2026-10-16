import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client';
import { PartyController, HEARTBEAT_MS, TICK_MS } from '../controller';
import { FakePlayer, FakeServer, deps, person } from './fakes';

/** The server clock runs 5 s ahead of this browser's, so the offset estimate matters. */
const SKEW = 5_000;

let server: FakeServer;
let player: FakePlayer;
let c: PartyController;
let d: ReturnType<typeof deps>;

function boot(o: { host?: boolean; control?: boolean } = {}) {
  vi.useFakeTimers();
  vi.setSystemTime(1_700_000_000_000);
  server = new FakeServer(() => Date.now() + SKEW, '1', o);
  player = new FakePlayer(() => Date.now());
  d = deps(server, player);
  c = new PartyController(d);
}
const tick = (ms: number) => vi.advanceTimersByTimeAsync(ms);

beforeEach(() => boot());
afterEach(async () => {
  await c.close();
  vi.useRealTimers();
});

describe('opening a party', () => {
  it('joins, estimates the server clock offset and hands the media to the player', async () => {
    await c.open('900');
    expect(c.state.status).toBe('active');
    expect(c.state.party?.id).toBe('900');
    expect(c.state.offsetMs).toBe(SKEW);
    expect(d.load).toHaveBeenCalledTimes(1);
    expect(d.load.mock.calls[0]?.[0].asset_id).toBe('501');
  });

  it('joins with the invite code when the party is private and unknown to the viewer', async () => {
    const api = server.api();
    const join = vi.fn(() => Promise.resolve({ ...server.party, playback: server.view() }));
    api.get = vi.fn(() => Promise.reject(new ApiError(404, 'nope')));
    api.join = join;
    c = new PartyController({ ...d, api });
    await c.open('900', { code: 'abc' });
    expect(join).toHaveBeenCalledWith('900', 'abc');
    expect(c.state.status).toBe('active');
  });

  it('joins a visible party the viewer is not in yet', async () => {
    server.party.you = {
      member: false,
      host: false,
      can_control: false,
      can_queue: false,
      can_manage: false,
    };
    const api = server.api();
    const join = vi.fn(() => Promise.resolve({ ...server.party, playback: server.view() }));
    api.join = join;
    c = new PartyController({ ...d, api });
    await c.open('900');
    expect(join).toHaveBeenCalled();
  });

  it('reports an age-restricted party as an age error, and a missing one as gone', async () => {
    server.failNext = new ApiError(403, 'age', { code: 900011, message: 'age' });
    await c.open('900');
    expect(c.state.status).toBe('error');
    expect(c.state.error?.kind).toBe('age');

    boot();
    server.failNext = new ApiError(404, 'gone');
    await c.open('900');
    expect(c.state.error?.kind).toBe('gone');
  });

  it('starts the player where the party is when it is already running', async () => {
    server.change({ playing: true });
    await tick(30_000);
    await c.open('900');
    const start = d.load.mock.calls[0]?.[1] as number;
    expect(start).toBeGreaterThanOrEqual(29_900);
    expect(start).toBeLessThanOrEqual(30_100);
    expect(d.load.mock.calls[0]?.[2]).toBe(true);
  });
});

describe('applying server state', () => {
  beforeEach(async () => {
    await c.open('900');
  });

  it('plays when the server plays, landing on the expected position', async () => {
    await tick(2_000);
    server.change({ playing: true });
    server.emitPlayback('play');
    await tick(0);
    expect(player.paused()).toBe(false);
    expect(Math.abs(player.positionMs() - server.position())).toBeLessThan(100);
  });

  it('pauses on the server position when the server pauses', async () => {
    server.change({ playing: true });
    server.emitPlayback('play');
    await tick(10_000);
    server.change({ playing: false });
    server.emitPlayback('pause');
    await tick(0);
    expect(player.paused()).toBe(true);
    expect(Math.abs(player.positionMs() - server.pb.position_ms)).toBeLessThan(100);
  });

  it('follows a seek while playing', async () => {
    server.change({ playing: true });
    server.emitPlayback('play');
    await tick(1_000);
    server.change({ position_ms: 3_000_000 });
    server.emitPlayback('seek');
    await tick(0);
    expect(Math.abs(player.positionMs() - 3_000_000)).toBeLessThan(200);
  });

  it('ignores a state that is not newer than the one applied (event order)', async () => {
    server.change({ playing: true });
    const play = server.view();
    server.change({ playing: false });
    const pause = server.view();
    c.receive(pause);
    c.receive(play); // arrives late
    await tick(0);
    expect(c.state.party?.playback.playing).toBe(false);
    expect(c.state.appliedVersion).toBe(pause.version);
    expect(player.paused()).toBe(true);
  });

  it('applies a newer state from the heartbeat when a gateway event was lost', async () => {
    server.change({ playing: true });
    await tick(HEARTBEAT_MS + 10);
    expect(c.state.party?.playback.playing).toBe(true);
    expect(player.paused()).toBe(false);
  });

  it('needs a click when the browser refuses to autoplay, and recovers from it', async () => {
    player.blocked = true;
    server.change({ playing: true });
    server.emitPlayback('play');
    await tick(0);
    expect(c.state.needsGesture).toBe(true);
    player.blocked = false;
    await c.resumeFromGesture();
    expect(c.state.needsGesture).toBe(false);
    expect(player.paused()).toBe(false);
  });

  it('reloads the media when the party switches title', async () => {
    d.load.mockClear();
    const media = {
      ...server.party.media!,
      asset_id: '777',
      title: 'Perfect Blue',
      title_id: '11',
    };
    server.party = { ...server.party, media };
    server.change({ position_ms: 0, playing: true });
    server.emit('WATCH_PARTY_UPDATE', {
      type: 'media',
      party: { ...server.party, playback: server.view() },
    });
    await tick(0);
    expect(d.load).toHaveBeenCalledTimes(1);
    expect(d.load.mock.calls[0]?.[0].asset_id).toBe('777');
    // The playback event for the same change does not load it twice.
    server.emitPlayback('media');
    await tick(0);
    expect(d.load).toHaveBeenCalledTimes(1);
  });
});

describe('drift correction (spec section 4)', () => {
  beforeEach(async () => {
    await c.open('900');
    server.change({ playing: true });
    server.emitPlayback('play');
    await tick(0);
  });

  it('leaves a drift under 100 ms alone', async () => {
    player.seekMs(player.positionMs() + 80);
    player.seeks.length = 0;
    player.rates.length = 0;
    await tick(TICK_MS * 3);
    expect(player.rates).toEqual([]);
    expect(player.seeks).toEqual([]);
  });

  it('nudges the rate when ahead by 100 ms to 2 s, then settles when converged', async () => {
    player.seekMs(player.positionMs() + 600);
    player.rates.length = 0;
    await tick(TICK_MS);
    expect(player.rates.at(-1)).toBe(0.95);
    // Running at 0.95x closes a 600 ms gap in about 12 s.
    await tick(16_000);
    expect(player.rates.at(-1)).toBe(1);
    expect(Math.abs(c.state.driftMs)).toBeLessThan(60);
  });

  it('speeds up when behind', async () => {
    player.seekMs(player.positionMs() - 800);
    player.rates.length = 0;
    await tick(TICK_MS);
    expect(player.rates.at(-1)).toBe(1.05);
  });

  it('hard seeks when more than 2 s off, then waits for the player to settle', async () => {
    player.seekMs(player.positionMs() - 5_000);
    player.seeks.length = 0;
    await tick(TICK_MS);
    expect(player.seeks).toHaveLength(1);
    expect(Math.abs(player.seeks[0]! - server.position())).toBeLessThan(700);
    expect(Math.abs(player.positionMs() - server.position())).toBeLessThan(300);
  });

  it('does not judge drift while the player is buffering', async () => {
    player.seekMs(player.positionMs() - 5_000);
    player.isWaiting = true;
    player.seeks.length = 0;
    await tick(TICK_MS * 4);
    expect(player.seeks).toEqual([]);
  });

  it('follows the party speed', async () => {
    server.change({ rate: 1.5 });
    server.emitPlayback('rate');
    await tick(0);
    expect(player.currentRate()).toBe(1.5);
    await tick(5_000);
    expect(Math.abs(player.positionMs() - server.position())).toBeLessThan(300);
  });
});

describe('buffering coordination', () => {
  beforeEach(async () => {
    await c.open('900');
    server.change({ playing: true });
    server.emitPlayback('play');
    await tick(0);
  });

  it('reports a stall only after 1.5 s, and its end', async () => {
    player.emit('waiting');
    await tick(1_000);
    expect(server.heartbeats).toEqual([]);
    await tick(600);
    expect(server.heartbeats).toEqual([{ buffering: true }]);
    expect(c.state.localBuffering).toBe(true);
    player.emit('playing');
    await tick(0);
    expect(server.heartbeats.at(-1)).toEqual({ buffering: false });
    expect(c.state.localBuffering).toBe(false);
  });

  it('says nothing about a stall that clears in time', async () => {
    player.emit('waiting');
    await tick(900);
    player.emit('playing');
    await tick(2_000);
    expect(server.heartbeats.filter((h) => h.buffering !== undefined)).toEqual([]);
  });

  it('shows who the party waits for while the server holds everybody', async () => {
    server.party = {
      ...server.party,
      participants: [person('1'), person('2', { buffering: true, username: 'ana' })],
    };
    server.change({ playing: false, auto_paused: true });
    server.emit('WATCH_PARTY_UPDATE', {
      type: 'participant_updated',
      party: { ...server.party, playback: server.view() },
    });
    server.emitPlayback('buffering');
    await tick(0);
    expect(c.state.waitingFor).toEqual(['ana']);
    expect(c.state.party?.playback.auto_paused).toBe(true);
    expect(player.paused()).toBe(true);
    // Resumed by the server: playing again, nobody to wait for.
    server.party = { ...server.party, participants: [person('1'), person('2')] };
    server.change({ playing: true, auto_paused: false });
    server.emit('WATCH_PARTY_UPDATE', {
      type: 'participant_updated',
      party: { ...server.party, playback: server.view() },
    });
    server.emitPlayback('resumed');
    await tick(0);
    expect(c.state.waitingFor).toEqual([]);
    expect(player.paused()).toBe(false);
  });
});

describe('commands', () => {
  it('sends a toggle guarded by the version the viewer saw and applies the answer', async () => {
    await c.open('900');
    await c.toggle();
    expect(server.commands.at(-1)).toMatchObject({ action: 'play', expected_version: 1 });
    expect(c.state.party?.playback.playing).toBe(true);
    expect(player.paused()).toBe(false);
    await c.toggle();
    expect(server.commands.at(-1)).toMatchObject({ action: 'pause' });
    expect(player.paused()).toBe(true);
  });

  it('re-syncs from a conflict instead of undoing a newer state', async () => {
    await c.open('900');
    // Someone else played behind our back; the event has not arrived yet.
    server.change({ playing: true });
    await c.toggle(); // we still believe "paused" -> would play, but the server is already ahead
    expect(c.state.party?.playback.playing).toBe(true);
    expect(c.state.notice).toMatch(/changed/);
  });

  it('seeks and changes the speed', async () => {
    await c.open('900');
    await c.seek(600_000);
    expect(server.commands.at(-1)).toEqual({ action: 'seek', position_ms: 600_000 });
    await c.setRate(1.25);
    expect(server.commands.at(-1)).toEqual({ action: 'rate', rate: 1.25 });
    expect(player.currentRate()).toBe(1.25);
  });

  it('explains a refusal without changing anything', async () => {
    boot({ host: false, control: false });
    await c.open('900');
    server.failNext = new ApiError(403, 'no');
    await c.toggle();
    expect(c.state.notice).toMatch(/cannot control/);
    expect(c.state.party?.playback.playing).toBe(false);
  });

  it('raises a hand with a reaction and shows other raised hands', async () => {
    boot({ host: false, control: false });
    await c.open('900');
    await c.requestControl();
    expect(server.api().react).toBeDefined();
    server.emit('WATCH_PARTY_REACTION', { party_id: '900', user_id: '2', emoji: '✋', at_ms: 1 });
    expect(c.state.hands).toEqual(['2']);
    expect(c.state.reactions).toHaveLength(1);
    await tick(4_100);
    expect(c.state.reactions).toHaveLength(0);
    c.lowerHand('2');
    expect(c.state.hands).toEqual([]);
  });

  it('sends the end of the media once', async () => {
    await c.open('900');
    server.change({ playing: true, position_ms: 7_199_000 });
    server.emitPlayback('seek');
    await tick(0);
    player.emit('ended');
    player.emit('ended');
    await tick(0);
    expect(server.commands.filter((x) => x.action === 'ended')).toHaveLength(1);
  });
});

describe('leaving and being removed', () => {
  it('ends the session when the heartbeat says the viewer is gone', async () => {
    await c.open('900');
    server.member = false;
    await tick(HEARTBEAT_MS + 10);
    expect(c.state.status).toBe('ended');
    expect(c.state.error?.kind).toBe('removed');
  });

  it('ends on a removal event and says why for the age gate', async () => {
    await c.open('900');
    server.emit('WATCH_PARTY_UPDATE', {
      type: 'removed',
      party_id: '900',
      reason: 'age_restricted',
    });
    expect(c.state.status).toBe('ended');
    expect(c.state.error?.kind).toBe('age');
    expect(player.paused()).toBe(true);
  });

  it('ends when the host ends the party', async () => {
    await c.open('900');
    server.emit('WATCH_PARTY_UPDATE', {
      type: 'ended',
      party: { ...server.party, ended: true, playback: server.view() },
    });
    expect(c.state.status).toBe('ended');
  });

  it('leaves on the server when asked to', async () => {
    await c.open('900');
    await c.close({ leave: true });
    expect(d.api.leave).toHaveBeenCalledWith('900');
    expect(c.state.status).toBe('idle');
  });

  it('ignores events of other parties', async () => {
    await c.open('900');
    server.emit('WATCH_PARTY_PLAYBACK', {
      party_id: '12345',
      reason: 'play',
      by: null,
      playback: { ...server.view(), version: 99, playing: true },
    });
    expect(c.state.party?.playback.playing).toBe(false);
  });

  it('refetches its permissions when the host changes the shared control', async () => {
    boot({ host: false, control: false });
    await c.open('900');
    expect(c.state.party?.you?.can_control).toBe(false);
    server.party = {
      ...server.party,
      settings: { ...server.party.settings, shared_control: 'everyone' },
      you: { ...server.party.you!, can_control: true },
    };
    server.emit('WATCH_PARTY_UPDATE', {
      type: 'settings',
      party: { ...server.party, you: null, playback: server.view() },
    });
    await tick(0);
    expect(c.state.party?.you?.can_control).toBe(true);
  });
});
