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
  endpoint: 'wss://chat.example.org/voice/ws',
  token: 'v1.claims.mac',
  userId: 'me',
  iceServers: [{ urls: 'stun:stun.example.org' }],
  mic: { deviceId: 'default', echoCancellation: true, autoGainControl: true, inputVolume: 100 },
};

const READY = {
  session_id: 'vs-1',
  resume_token: 'rt-1',
  heartbeat_interval: 13_750,
  user_id: 'me',
  channel_id: 'c1',
  participants: [{ user_id: 'u5', speaking: 1 }],
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

const flush = async (n = 12) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
};

/** Drive the handshake through to a connected peer. */
async function handshake(world: FakeEngineWorld, engine: RtcVoiceEngine) {
  const done = engine.connect(params);
  done.catch(() => undefined);
  await flush();
  const ws = world.sockets[0]!;
  ws.open();
  ws.serve(VoiceOp.READY, READY);
  await flush();
  ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-sdp' });
  await flush();
  const pc = world.peers[0]!;
  pc.setState('connected');
  await done;
  return { ws, pc };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('RtcVoiceEngine handshake', () => {
  it('runs IDENTIFY, READY, offer, answer and reports connected', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    expect(ws.url).toBe('wss://chat.example.org/voice/ws');
    // IDENTIFY carries only the token (spec 4.3).
    expect(ws.ops(VoiceOp.IDENTIFY)[0]!.d).toEqual({ token: 'v1.claims.mac' });
    expect((pc.config as { iceServers: unknown[] }).iceServers).toEqual([
      { urls: 'stun:stun.example.org' },
    ]);
    const offer = ws.ops(VoiceOp.SDP_OFFER)[0]!.d as { sdp: string };
    expect(offer.sdp).toContain('usedtx=1');
    expect(offer.sdp).toContain('stereo=0');
    expect(pc.transceivers).toHaveLength(1);
    // The mic line is sendonly (spec 4.2): sendrecv would add a silent remote track.
    expect(pc.transceivers[0]!.init).toMatchObject({ direction: 'sendonly' });
    expect(pc.remote[0]).toEqual({ type: 'answer', sdp: 'answer-sdp' });
    expect(engine.status).toBe('connected');
    expect(events.status).toHaveBeenLastCalledWith('connected');
    // READY lists who is already talking.
    expect(events.speaking).toHaveBeenCalledWith('u5', true, false);
  });

  it('heartbeats at the interval READY asks for', async () => {
    const { world, engine } = setup();
    const { ws } = await handshake(world, engine);
    ws.serve(VoiceOp.HEARTBEAT_ACK, { t: 1 });
    vi.advanceTimersByTime(13_750);
    expect(ws.ops(VoiceOp.HEARTBEAT)).toHaveLength(1);
    ws.serve(VoiceOp.HEARTBEAT_ACK, { t: 1 });
    vi.advanceTimersByTime(13_750);
    expect(ws.ops(VoiceOp.HEARTBEAT)).toHaveLength(2);
  });

  it('treats a missing heartbeat ack as a dead socket and resumes', async () => {
    const { world, events, engine } = setup();
    const { ws } = await handshake(world, engine);
    vi.advanceTimersByTime(13_750); // beat 1, never acked
    vi.advanceTimersByTime(13_750); // no ack: drop
    expect(events.status).toHaveBeenLastCalledWith('reconnecting');
    await vi.advanceTimersByTimeAsync(600);
    const ws2 = world.sockets[1]!;
    expect(ws2).not.toBe(ws);
    ws2.open();
    expect(ws2.ops(VoiceOp.RESUME)).toHaveLength(1);
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

  it('a bad token (4004) while connecting is a typed failure', async () => {
    const { world, engine } = setup();
    const p = engine.connect(params).catch((e: unknown) => e);
    await flush();
    world.sockets[0]!.open();
    world.sockets[0]!.drop(VoiceClose.AUTHENTICATION_FAILED);
    expect(((await p) as VoiceConnectError).voice.code).toBe('closed');
  });
});

describe('RtcVoiceEngine media', () => {
  it('opens the mic gate only while transmitting and reports SPEAKING (1 held, 0 released)', async () => {
    const { world, engine } = setup();
    const { ws } = await handshake(world, engine);
    const track = world.audio.contexts[0]!.dests[0]!.stream.getAudioTracks()[0] as unknown as {
      enabled: boolean;
    };
    expect(track.enabled).toBe(false);
    engine.setTransmitting(true);
    expect(track.enabled).toBe(true);
    engine.setTransmitting(true); // no duplicate report
    engine.setTransmitting(false);
    expect(track.enabled).toBe(false);
    const speaking = ws.ops(VoiceOp.SPEAKING).map((f) => (f.d as { speaking: number }).speaking);
    expect(speaking).toEqual([1, 0]);
  });

  it('plays forwarded streams keyed by their msid user id and drops people who leave', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    pc.receive('u9');
    expect(world.audio.elements).toHaveLength(1);
    expect(events.peer).toHaveBeenCalledWith('u9', true);
    ws.serve(VoiceOp.SPEAKING, { user_id: 'u9', speaking: 1, audio_level: -20 });
    expect(events.speaking).toHaveBeenCalledWith('u9', true, false);
    ws.serve(VoiceOp.SPEAKING, { user_id: 'u9', speaking: 0 });
    expect(events.speaking).toHaveBeenLastCalledWith('u9', false, false);
    ws.serve(VoiceOp.CLIENT_DISCONNECT, { user_id: 'u9' });
    expect(events.peer).toHaveBeenLastCalledWith('u9', false);
  });

  it('ducks everyone else while a priority speaker talks', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    pc.receive('u9');
    pc.receive('boss');
    const [, g9, gBoss] = world.audio.contexts.at(-1)!.gains;
    ws.serve(VoiceOp.SPEAKING, { user_id: 'boss', speaking: 5 });
    expect(g9!.gain.value).toBeCloseTo(0.4);
    expect(gBoss!.gain.value).toBe(1);
    ws.serve(VoiceOp.SPEAKING, { user_id: 'boss', speaking: 0 });
    expect(g9!.gain.value).toBe(1);
  });

  it('applies per-user volume, local mute and deafen to remote audio', async () => {
    const { world, engine } = setup();
    engine.setUserVolume('u9', 170); // set before they arrive
    engine.setUserMuted('u8', true);
    const { pc } = await handshake(world, engine);
    pc.receive('u9');
    pc.receive('u8');
    // contexts: [mic ctx, playback ctx]; playback gains: master, u9, u8
    const [master, g9, g8] = world.audio.contexts.at(-1)!.gains;
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

  it('answers a server renegotiation offer with SDP_ANSWER', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    ws.serve(VoiceOp.SDP_OFFER, { sdp: 'server-offer' });
    await flush();
    expect(pc.remote.at(-1)).toEqual({ type: 'offer', sdp: 'server-offer' });
    expect(ws.ops(VoiceOp.SDP_ANSWER).at(-1)!.d).toEqual({ sdp: 'answer-sdp' });
  });

  it('is the impolite peer: ignores a server offer while its own is outstanding (spec 4.4)', async () => {
    const { world, engine } = setup();
    const done = engine.connect(params);
    done.catch(() => undefined);
    await flush();
    const ws = world.sockets[0]!;
    ws.open();
    ws.serve(VoiceOp.READY, READY);
    await flush();
    const pc = world.peers[0]!;
    expect(ws.ops(VoiceOp.SDP_OFFER)).toHaveLength(1); // our offer is outstanding
    ws.serve(VoiceOp.SDP_OFFER, { sdp: 'server-offer' });
    await flush();
    // No rollback, no answer: the server abandons its offer and replaces it after answering ours.
    expect(pc.rollbacks).toBe(0);
    expect(ws.ops(VoiceOp.SDP_ANSWER)).toHaveLength(0);
    expect(pc.remote).toHaveLength(0);
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-1' });
    await flush();
    pc.setState('connected');
    await done;
    expect(pc.remote).toEqual([{ type: 'answer', sdp: 'answer-1' }]);
    // The replacement offer now gets answered.
    ws.serve(VoiceOp.SDP_OFFER, { sdp: 'server-offer-2' });
    await flush();
    expect(pc.remote.at(-1)).toEqual({ type: 'offer', sdp: 'server-offer-2' });
    expect(ws.ops(VoiceOp.SDP_ANSWER)).toHaveLength(1);
  });

  it('applies the channel bitrate to the offer and the sender (spec 4.4)', async () => {
    const { world, engine } = setup();
    const setParameters = vi.fn(async () => undefined);
    const done = engine.connect({ ...params, maxBitrate: 96_000 });
    done.catch(() => undefined);
    await flush();
    const ws = world.sockets[0]!;
    ws.open();
    ws.serve(VoiceOp.READY, READY);
    await flush();
    const pc = world.peers[0]!;
    (pc as unknown as { getSenders: () => unknown[] }).getSenders = () => [
      {
        track: { kind: 'audio' },
        getParameters: () => ({ encodings: [{}] }),
        setParameters,
      },
    ];
    expect((ws.ops(VoiceOp.SDP_OFFER)[0]!.d as { sdp: string }).sdp).toContain(
      'maxaveragebitrate=96000',
    );
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'a' });
    await flush();
    pc.setState('connected');
    await done;
    expect(setParameters).toHaveBeenCalledWith({ encodings: [{ maxBitrate: 96_000 }] });
  });

  it('swaps the input device mid-call without renegotiating', async () => {
    const { world, engine } = setup();
    const { pc, ws } = await handshake(world, engine);
    const replace = vi.fn(async () => undefined);
    (pc as unknown as { getSenders: () => unknown[] }).getSenders = () => [
      { track: { kind: 'audio' }, replaceTrack: replace },
    ];
    await engine.setInputDevice({ ...params.mic, deviceId: 'usb-2' });
    expect(replace).toHaveBeenCalledOnce();
    expect(world.audio.gum).toHaveBeenCalledTimes(2);
    expect(ws.ops(VoiceOp.SDP_OFFER)).toHaveLength(1);
  });
});

describe('RtcVoiceEngine reconnect and resume', () => {
  it('resumes with the resume token after the socket drops; media keeps flowing', async () => {
    const { world, events, engine } = setup();
    const { ws } = await handshake(world, engine);
    ws.drop(1006);
    expect(events.status).toHaveBeenLastCalledWith('reconnecting');
    await vi.advanceTimersByTimeAsync(600);
    const ws2 = world.sockets[1]!;
    expect(ws2).not.toBe(ws);
    ws2.open();
    expect(ws2.ops(VoiceOp.RESUME)[0]!.d).toEqual({ session_id: 'vs-1', resume_token: 'rt-1' });
    expect(ws2.ops(VoiceOp.IDENTIFY)).toHaveLength(0);
    ws2.serve(VoiceOp.RESUMED, { ...READY, participants: [] });
    expect(engine.status).toBe('connected');
    expect(world.peers).toHaveLength(1);
    expect(world.peers[0]!.closed).toBe(false);
  });

  it('SFU restarting (4015) resumes; session invalid (4006) asks for a new join', async () => {
    const a = setup();
    const { ws } = await handshake(a.world, a.engine);
    ws.drop(VoiceClose.SERVER_RESTARTING);
    expect(a.events.fatal).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(600);
    expect(a.world.sockets).toHaveLength(2);

    const b = setup();
    const { ws: wsB } = await handshake(b.world, b.engine);
    wsB.drop(VoiceClose.SESSION_INVALID);
    expect(b.events.fatal).toHaveBeenCalledWith(expect.objectContaining({ code: 'closed' }));
    expect(b.engine.status).toBe('idle');
    expect(b.world.peers[0]!.closed).toBe(true);
  });

  it('removed from the call (4014) closes everything and does not reconnect', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    ws.drop(VoiceClose.DISCONNECTED);
    expect(events.fatal).toHaveBeenCalledWith(expect.objectContaining({ code: 'removed' }));
    expect(pc.closed).toBe(true);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(world.sockets).toHaveLength(1);
  });

  it('reports replaced (4016) and full (4017) distinctly', async () => {
    const a = setup();
    const { ws } = await handshake(a.world, a.engine);
    ws.drop(VoiceClose.SESSION_REPLACED);
    expect(a.events.fatal).toHaveBeenCalledWith(expect.objectContaining({ code: 'replaced' }));
    const b = setup();
    const { ws: wsB } = await handshake(b.world, b.engine);
    wsB.drop(VoiceClose.SERVER_FULL);
    expect(b.events.fatal).toHaveBeenCalledWith(expect.objectContaining({ code: 'full' }));
  });

  it('gives up after repeated socket failures', async () => {
    const { world, events, engine } = setup();
    const { ws } = await handshake(world, engine);
    let sock: FakeSocket = ws;
    for (let i = 0; i < 7; i++) {
      sock.drop(1006);
      if (events.fatal.mock.calls.length) break;
      await vi.advanceTimersByTimeAsync(9_000);
      sock = world.sockets.at(-1)!;
    }
    expect(events.fatal).toHaveBeenCalledOnce();
  });

  it('restarts ICE (a new offer with iceRestart) after the peer stays disconnected', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    pc.setState('disconnected');
    expect(events.status).toHaveBeenLastCalledWith('reconnecting');
    await vi.advanceTimersByTimeAsync(DISCONNECT_GRACE_MS + 10);
    expect(pc.offers.at(-1)).toEqual({ iceRestart: true });
    expect(ws.ops(VoiceOp.SDP_OFFER)).toHaveLength(2);
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-2' });
    await flush();
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

  it('restarts ICE immediately when the browser comes back online (network change)', async () => {
    const { world, engine } = setup();
    const { pc } = await handshake(world, engine);
    world.goOnline();
    await flush();
    expect(pc.offers.at(-1)).toEqual({ iceRestart: true });
  });

  it('queues the ICE restart while the socket is down and sends it after RESUMED', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    ws.drop(1006);
    world.goOnline(); // Wi-Fi to 4G: both sockets and ICE are gone
    await flush();
    expect(pc.offers.at(-1)).toEqual({ iceRestart: true });
    expect(ws.ops(VoiceOp.SDP_OFFER)).toHaveLength(1); // nothing could be sent yet
    await vi.advanceTimersByTimeAsync(600);
    const ws2 = world.sockets[1]!;
    ws2.open();
    ws2.serve(VoiceOp.RESUMED, { ...READY, participants: [] });
    await flush();
    expect(ws2.ops(VoiceOp.SDP_OFFER)).toHaveLength(1);
  });

  it('retries ICE on failure and gives up after the retry budget', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    for (let i = 0; i < MAX_ICE_RESTARTS; i++) {
      pc.setState('failed');
      await flush();
      ws.serve(VoiceOp.SDP_ANSWER, { sdp: `a${i}` });
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
