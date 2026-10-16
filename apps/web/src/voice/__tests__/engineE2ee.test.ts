import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RtcVoiceEngine, type ConnectParams, type EngineEvents } from '../engine';
import { VoiceOp } from '../protocol';
import { fakeEngineDeps } from './fakes';

// VOX-E2E-003: with E2EE on, the engine hands every sender and receiver to the transform hooks
// and builds the peer connection with the options the transform mode needs.

const flush = async (n = 12) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
};

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('RtcVoiceEngine with E2EE hooks', () => {
  it('attaches the mic sender and every remote receiver', async () => {
    const world = fakeEngineDeps();
    const events = {
      status: vi.fn(),
      speaking: vi.fn(),
      quality: vi.fn(),
      fatal: vi.fn(),
      level: vi.fn(),
      micLost: vi.fn(),
      peer: vi.fn(),
      governor: vi.fn(),
    } satisfies EngineEvents;
    const e2ee = {
      peerConfig: { encodedInsertableStreams: true },
      attachSender: vi.fn(),
      attachReceiver: vi.fn(),
    };
    const params: ConnectParams = {
      endpoint: 'wss://chat.example.org/voice/ws',
      token: 't',
      userId: 'me',
      mic: { deviceId: 'default', echoCancellation: true, autoGainControl: true, inputVolume: 100 },
      e2ee,
    };
    const engine = new RtcVoiceEngine(events, world.deps);
    const done = engine.connect(params);
    done.catch(() => undefined);
    await flush();
    const ws = world.sockets[0]!;
    ws.open();
    ws.serve(VoiceOp.READY, {
      session_id: 's',
      resume_token: 'r',
      heartbeat_interval: 13_750,
      user_id: 'me',
      channel_id: 'c1',
      participants: [],
    });
    await flush();
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-sdp' });
    await flush();
    const pc = world.peers[0]!;
    pc.setState('connected');
    await done;

    expect(pc.config).toMatchObject({ encodedInsertableStreams: true });
    expect(e2ee.attachSender).toHaveBeenCalledTimes(1);
    expect(e2ee.attachSender.mock.calls[0]![1]).toBe('audio');
    pc.receive('u9');
    expect(e2ee.attachReceiver).toHaveBeenCalledWith(undefined, 'audio', 'u9');
  });
});
