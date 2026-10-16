import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RtcVoiceEngine, type ConnectParams, type EngineEvents } from '../engine';
import { VoiceOp } from '../protocol';
import { fakeEngineDeps } from './fakes';

// LNG-USET-050: the QoS switch asks for high packet priority on the voice line.
const flush = async (n = 12) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
};
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

async function join(highPriority: boolean) {
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
  const params: ConnectParams = {
    endpoint: 'wss://chat.example.org/voice/ws',
    token: 't',
    userId: 'me',
    mic: { deviceId: 'default', echoCancellation: true, autoGainControl: true, inputVolume: 100 },
    ...(highPriority ? { highPriority: true } : {}),
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
  world.peers[0]!.setState('connected');
  await done;
  return world.peers[0]!.transceivers[0]!;
}

describe('high packet priority', () => {
  it('marks the mic line when switched on', async () => {
    const tx = await join(true);
    expect((tx.init as { sendEncodings?: unknown }).sendEncodings).toEqual([
      { priority: 'high', networkPriority: 'high' },
    ]);
  });
  it('leaves the line alone by default', async () => {
    const tx = await join(false);
    expect((tx.init as { sendEncodings?: unknown }).sendEncodings).toBeUndefined();
  });
});
