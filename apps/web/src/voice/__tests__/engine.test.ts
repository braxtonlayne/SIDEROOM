import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  CONNECT_TIMEOUT_MS,
  DISCONNECT_GRACE_MS,
  MAX_ICE_RESTARTS,
  QUALITY_INTERVAL_MS,
  RtcVoiceEngine,
  VoiceConnectError,
  type ConnectParams,
  type EngineEvents,
} from '../engine';
import { VoiceClose, VoiceOp } from '../protocol';
import { fakeEngineDeps, type FakeEngineWorld, type FakeSocket } from './fakes';

const params: ConnectParams = {
  endpoint: 'voice.example.org:8443',
  token: 'tok',
  guildId: 'g1',
  userId: 'me',
  sessionId: 'sess',
  iceServers: [{ urls: 'stun:stun.example.org' }],
  mic: { deviceId: 'default', echoCancellation: true, autoGainControl: true, inputVolume: 100 },
};

function setup() {
  const world = fakeEngineDeps();
  const events = {
    status: vi.fn(),
    speaking: vi.fn(),
    quality: vi.fn(),
    fatal: vi.fn(),
    level: vi.fn(),
    micLost: vi.fn(),
    peer: vi.fn(),
  } satisfies EngineEvents;
  const engine = new RtcVoiceEngine(events, world.deps);
  return { world, events, engine };
}

const flush = async (n = 8) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
};

/** Drive the handshake up to (not including) the peer reaching "connected". */
async function handshake(world: FakeEngineWorld, engine: RtcVoiceEngine) {
  const done = engine.connect(params);
  done.catch(() => undefined);
  await flush();
  const ws = world.sockets[0]!;
  ws.serve(VoiceOp.HELLO, { heartbeat_interval: 10_000 });
  ws.serve(VoiceOp.READY, {
    ssrc: 77,
    ice_servers: [{ urls: 'turn:turn.example.org', username: 'u', credential: 'c' }],
  });
  await flush();
  ws.serve(VoiceOp.SESSION_DESCRIPTION, { sdp: 'answer-sdp' });
  await flush();
  const pc = world.peers[0]!;
  pc.setState('connected');
  await done;
  return { ws, pc };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('RtcVoiceEngine handshake', () => {
  it('runs HELLO, IDENTIFY, READY, offer, answer and reports connected', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    expect(ws.url).toBe('ws://voice.example.org:8443/voice?v=1');
    expect(ws.ops(VoiceOp.IDENTIFY)[0]!.d).toEqual({
      server_id: 'g1',
      user_id: 'me',
      session_id: 'sess',
      token: 'tok',
    });
    // ICE servers: from VOICE_SERVER_UPDATE plus the READY extras.
    expect((pc.config as { iceServers: unknown[] }).iceServers).toHaveLength(2);
    const offer = ws.ops(VoiceOp.SELECT_PROTOCOL)[0]!.d as {
      sdp: string;
      protocol: string;
      type: string;
    };
    expect(offer.protocol).toBe('webrtc');
    expect(offer.type).toBe('offer');
    expect(offer.sdp).toContain('usedtx=1');
    expect(pc.transceivers).toHaveLength(1);
    expect(pc.remote[0]).toEqual({ type: 'answer', sdp: 'answer-sdp' });
    expect(engine.status).toBe('connected');
    expect(events.status).toHaveBeenLastCalledWith('connected');
  });

  it('heartbeats at the interval the server asks for', async () => {
    const { world, engine } = setup();
    const { ws } = await handshake(world, engine);
    vi.advanceTimersByTime(25_000);
    expect(ws.ops(VoiceOp.HEARTBEAT)).toHaveLength(2);
  });

  it('fails with a typed error when the microphone is denied', async () => {
    const { world, engine } = setup();
    world.audio.gum.mockRejectedValue(new DOMException('no', 'NotAllowedError'));
    const err = await engine.connect(params).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(VoiceConnectError);
    expect((err as VoiceConnectError).voice).toMatchObject({
      code: 'mic',
      mic: 'permission-denied',
    });
    expect(world.sockets).toHaveLength(0);
    expect(engine.status).toBe('idle');
  });

  it('times out when the SFU never answers', async () => {
    const { world, engine } = setup();
    const p = engine.connect(params).catch((e: unknown) => e);
    await flush();
    vi.advanceTimersByTime(CONNECT_TIMEOUT_MS + 1);
    const err = await p;
    expect((err as VoiceConnectError).voice.code).toBe('connect-timeout');
    expect(world.sockets[0]!.closedWith).toBe(1000);
    expect(engine.status).toBe('idle');
  });
});

describe('RtcVoiceEngine media', () => {
  it('opens the mic gate only while transmitting and announces SPEAKING', async () => {
    const { world, engine } = setup();
    const { ws } = await handshake(world, engine);
    const track = world.audio.contexts[0]!.dests[0]!.stream.getAudioTracks()[0] as unknown as {
      enabled: boolean;
    };
    expect(track.enabled).toBe(false);
    engine.setTransmitting(true);
    expect(track.enabled).toBe(true);
    engine.setTransmitting(true); // no duplicate announcement
    engine.setTransmitting(false);
    expect(track.enabled).toBe(false);
    const speaking = ws.ops(VoiceOp.SPEAKING).map((f) => (f.d as { speaking: number }).speaking);
    expect(speaking).toEqual([1, 0]);
  });

  it('plays forwarded streams, reports speakers and drops people who leave', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    pc.receive('u9');
    expect(world.audio.elements).toHaveLength(1);
    expect(events.peer).toHaveBeenCalledWith('u9', true);
    ws.serve(VoiceOp.SPEAKING, { user_id: 'u9', speaking: 1 });
    expect(events.speaking).toHaveBeenCalledWith('u9', true);
    ws.serve(VoiceOp.SPEAKING, { user_id: 'u9', speaking: 0 });
    expect(events.speaking).toHaveBeenLastCalledWith('u9', false);
    ws.serve(VoiceOp.CLIENT_DISCONNECT, { user_id: 'u9' });
    expect(events.peer).toHaveBeenLastCalledWith('u9', false);
  });

  it('applies per-user volume, local mute and deafen to remote audio', async () => {
    const { world, engine } = setup();
    engine.setUserVolume('u9', 170); // set before they arrive
    engine.setUserMuted('u8', true);
    const { pc } = await handshake(world, engine);
    pc.receive('u9');
    pc.receive('u8');
    const gains = world.audio.contexts.at(-1)!.gains;
    // contexts: [mic ctx, playback ctx]; playback gains: master, u9, u8
    const [master, g9, g8] = gains;
    expect(g9!.gain.value).toBe(1.7);
    expect(g8!.gain.value).toBe(0);
    engine.setDeafened(true);
    expect(master!.gain.value).toBe(0);
    engine.setDeafened(false);
    engine.setUserMuted('u8', false);
    expect(g8!.gain.value).toBe(1);
  });

  it('polls connection quality from stats', async () => {
    const { world, events, engine } = setup();
    const { pc } = await handshake(world, engine);
    pc.stats = [{ type: 'candidate-pair', state: 'succeeded', currentRoundTripTime: 0.03 }];
    await vi.advanceTimersByTimeAsync(QUALITY_INTERVAL_MS + 10);
    expect(events.quality).toHaveBeenLastCalledWith({ level: 'good', pingMs: 30, lossPct: null });
  });

  it('answers a server-initiated renegotiation offer', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    ws.serve(VoiceOp.SESSION_DESCRIPTION, { type: 'offer', sdp: 'server-offer' });
    await flush();
    expect(pc.remote.at(-1)).toEqual({ type: 'offer', sdp: 'server-offer' });
    const answer = ws.ops(VoiceOp.SELECT_PROTOCOL).at(-1)!.d as { type: string; sdp: string };
    expect(answer).toMatchObject({ type: 'answer', sdp: 'answer-sdp' });
  });

  it('swaps the input device mid-call without renegotiating', async () => {
    const { world, engine } = setup();
    const { pc } = await handshake(world, engine);
    const replace = vi.fn(async () => undefined);
    (pc as unknown as { getSenders: () => unknown[] }).getSenders = () => [
      { track: { kind: 'audio' }, replaceTrack: replace },
    ];
    await engine.setInputDevice({ ...params.mic, deviceId: 'usb-2' });
    expect(replace).toHaveBeenCalledOnce();
    expect(world.audio.gum).toHaveBeenCalledTimes(2);
  });
});

describe('RtcVoiceEngine reconnect and resume', () => {
  it('resumes on the same session after the socket drops', async () => {
    const { world, events, engine } = setup();
    const { ws } = await handshake(world, engine);
    ws.drop(1006);
    expect(events.status).toHaveBeenLastCalledWith('reconnecting');
    await vi.advanceTimersByTimeAsync(600);
    const ws2 = world.sockets[1]!;
    expect(ws2).not.toBe(ws);
    ws2.serve(VoiceOp.HELLO, { heartbeat_interval: 10_000 });
    expect(ws2.ops(VoiceOp.RESUME)[0]!.d).toEqual({
      server_id: 'g1',
      session_id: 'sess',
      token: 'tok',
    });
    expect(ws2.ops(VoiceOp.IDENTIFY)).toHaveLength(0);
    ws2.serve(VoiceOp.RESUMED);
    expect(engine.status).toBe('connected');
    expect(world.peers).toHaveLength(1);
  });

  it('asks for a fresh voice server when the session is no longer valid', async () => {
    const { world, events, engine } = setup();
    const { ws } = await handshake(world, engine);
    ws.drop(VoiceClose.SESSION_TIMEOUT);
    expect(events.fatal).toHaveBeenCalledOnce();
    expect(engine.status).toBe('idle');
    expect(world.peers[0]!.closed).toBe(true);
  });

  it('gives up after repeated socket failures', async () => {
    const { world, events, engine } = setup();
    const { ws } = await handshake(world, engine);
    let sock: FakeSocket = ws;
    for (let i = 0; i < 6; i++) {
      sock.drop(1006);
      if (events.fatal.mock.calls.length) break;
      await vi.advanceTimersByTimeAsync(9_000);
      sock = world.sockets.at(-1)!;
    }
    expect(events.fatal).toHaveBeenCalledOnce();
  });

  it('restarts ICE after the peer stays disconnected', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    pc.setState('disconnected');
    expect(events.status).toHaveBeenLastCalledWith('reconnecting');
    await vi.advanceTimersByTimeAsync(DISCONNECT_GRACE_MS + 10);
    expect(pc.offers.at(-1)).toEqual({ iceRestart: true });
    expect(ws.ops(VoiceOp.SELECT_PROTOCOL)).toHaveLength(2);
    pc.setState('connected');
    expect(engine.status).toBe('connected');
  });

  it('does not restart when the peer recovers within the grace period', async () => {
    const { world, engine } = setup();
    const { pc } = await handshake(world, engine);
    pc.setState('disconnected');
    await vi.advanceTimersByTimeAsync(DISCONNECT_GRACE_MS - 500);
    pc.setState('connected');
    await vi.advanceTimersByTimeAsync(5_000);
    expect(pc.offers).toHaveLength(1);
  });

  it('restarts ICE immediately when the browser comes back online', async () => {
    const { world, engine } = setup();
    const { pc } = await handshake(world, engine);
    world.goOnline();
    await flush();
    expect(pc.offers.at(-1)).toEqual({ iceRestart: true });
  });

  it('restarts ICE on failure and gives up after the retry budget', async () => {
    const { world, events, engine } = setup();
    const { pc } = await handshake(world, engine);
    for (let i = 0; i < MAX_ICE_RESTARTS; i++) {
      pc.setState('failed');
      await flush();
    }
    expect(pc.offers.filter((o) => o?.iceRestart)).toHaveLength(MAX_ICE_RESTARTS);
    pc.setState('failed');
    expect(events.fatal).toHaveBeenCalledOnce();
  });

  it('tears everything down on disconnect', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    pc.receive('u9');
    engine.disconnect();
    expect(ws.closedWith).toBe(1000);
    expect(pc.closed).toBe(true);
    expect(world.audio.contexts.every((c) => c.closed)).toBe(true);
    expect(engine.status).toBe('idle');
  });
});
