import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RtcVoiceEngine, type ConnectParams, type EngineEvents } from '../engine';
import { parseStreamId, VoiceOp } from '../protocol';
import { fakeEngineDeps, FakeStream, FakeTrack, type FakeEngineWorld } from './fakes';

const params: ConnectParams = {
  endpoint: 'wss://chat.example.org/voice/ws',
  token: 'v1.claims.mac',
  userId: 'me',
  mic: { deviceId: 'default', echoCancellation: true, autoGainControl: true, inputVolume: 100 },
};
const READY = {
  session_id: 'vs-1',
  resume_token: 'rt-1',
  heartbeat_interval: 13_750,
  user_id: 'me',
  channel_id: 'c1',
  participants: [],
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
    remoteVideo: vi.fn(),
    videoPublished: vi.fn(),
    videoLayer: vi.fn(),
  } satisfies EngineEvents;
  const engine = new RtcVoiceEngine(events, world.deps);
  return { world, events, engine };
}

const flush = async (n = 16) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
};

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

/** A capture stream with a video track and, optionally, audio. */
function capture(audio = false) {
  const tracks = [
    Object.assign(new FakeTrack('cam'), { kind: 'video' }),
    ...(audio ? [new FakeTrack('tab audio')] : []),
  ];
  return new FakeStream(tracks, 'local') as unknown as MediaStream;
}

const published = (ws: { ops(op: number): { d?: unknown }[] }) =>
  ws.ops(VoiceOp.VIDEO).map((f) => f.d as Record<string, unknown>);

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('publishing video (VOX-CLI-006/007)', () => {
  it('puts the camera on its own sendonly line with three simulcast layers and names it to the SFU', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    engine.publishVideo('camera', capture(), { width: 1280, height: 720, fps: 30 });
    await flush();
    // The mic line plus the camera line.
    expect(pc.transceivers).toHaveLength(2);
    const cam = pc.transceivers[1]!;
    const init = cam.init as {
      direction: string;
      sendEncodings: { rid: string; scaleResolutionDownBy: number; maxBitrate: number }[];
    };
    expect(init.direction).toBe('sendonly');
    expect(init.sendEncodings.map((e) => e.rid)).toEqual(['f', 'h', 'q']);
    expect(init.sendEncodings.map((e) => e.scaleResolutionDownBy)).toEqual([1, 2, 4]);
    // A new offer carries the line, then the SFU is told which mid is the camera.
    expect(ws.ops(VoiceOp.SDP_OFFER)).toHaveLength(2);
    expect(published(ws)).toEqual([
      {
        t: 'publish',
        mid: cam.mid,
        source: 'camera',
        active: true,
        width: 1280,
        height: 720,
        fps: 30,
      },
    ]);
    // Answering the renegotiation does not announce it twice.
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-2' });
    await flush();
    expect(published(ws)).toHaveLength(1);
  });

  it('sends a screen and its audio on two lines, each announced with its source', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    engine.publishVideo('screen', capture(true), { width: 1920, height: 1080, fps: 60 });
    await flush();
    expect(pc.transceivers).toHaveLength(3);
    const sent = published(ws);
    expect(sent.map((m) => m.source).sort()).toEqual(['screen', 'screen_audio']);
    const screen = sent.find((m) => m.source === 'screen')!;
    const init = pc.transceivers[1]!.init as { sendEncodings: { maxFramerate: number }[] };
    expect(init.sendEncodings).toHaveLength(1);
    expect(init.sendEncodings[0]!.maxFramerate).toBe(60);
    expect(screen).toMatchObject({ width: 1920, height: 1080, fps: 60, active: true });
    expect(pc.transceivers.map((t) => t.mid)).toContain(screen.mid);
  });

  it('adds the lines to the first offer when the capture was running before the peer existed', async () => {
    const { world, engine } = setup();
    const done = engine.connect(params);
    done.catch(() => undefined);
    await flush();
    engine.publishVideo('camera', capture(), { width: 640, height: 360, fps: 30 });
    const ws = world.sockets[0]!;
    ws.open();
    ws.serve(VoiceOp.READY, READY);
    await flush();
    const pc = world.peers[0]!;
    expect(pc.transceivers).toHaveLength(2);
    expect(published(ws)).toHaveLength(1);
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-sdp' });
    await flush();
    pc.setState('connected');
    await done;
  });

  it('stops the line and tells the SFU when the camera goes off', async () => {
    const { world, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    engine.publishVideo('camera', capture(), { width: 1280, height: 720, fps: 30 });
    await flush();
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-2' });
    await flush();
    const mid = pc.transceivers[1]!.mid;
    engine.unpublishVideo('camera');
    await flush();
    expect(pc.transceivers[1]!.stopped).toBe(true);
    expect(published(ws).at(-1)).toMatchObject({
      t: 'publish',
      mid,
      source: 'camera',
      active: false,
    });
    // And a fresh offer removes the line.
    expect(ws.ops(VoiceOp.SDP_OFFER)).toHaveLength(3);
  });

  it('forgets its captures when the call ends', async () => {
    const { world, engine } = setup();
    const { pc } = await handshake(world, engine);
    engine.publishVideo('camera', capture(), { width: 1280, height: 720, fps: 30 });
    await flush();
    engine.disconnect();
    expect(pc.closed).toBe(true);
    // A later session starts without it.
    const again = engine.connect(params);
    again.catch(() => undefined);
    await flush();
    world.sockets[1]!.open();
    world.sockets[1]!.serve(VoiceOp.READY, READY);
    await flush();
    expect(world.peers[1]!.transceivers).toHaveLength(1);
    engine.disconnect();
  });
});

describe('receiving video (VOX-SFU-005/006 forwarded lines)', () => {
  it('tells the app about a camera and a screen by the msid stream names and keeps them out of the audio', async () => {
    const { world, events, engine } = setup();
    const { pc } = await handshake(world, engine);
    const cam = pc.receiveStream('42-camera', ['video']);
    const screen = pc.receiveStream('42-screen', ['video', 'audio']);
    expect(events.remoteVideo).toHaveBeenCalledWith('42', 'camera', cam);
    // Both tracks of the screen group arrive on one stream, reported once.
    expect(events.remoteVideo).toHaveBeenCalledWith('42', 'screen', screen);
    expect(events.remoteVideo).toHaveBeenCalledTimes(2);
    // Neither is a voice: only the plain user id stream is.
    expect(events.peer).not.toHaveBeenCalled();
    pc.receiveStream('42', ['audio']);
    expect(events.peer).toHaveBeenCalledWith('42', true);
  });

  it('drops a stream when its lines are removed or the SFU says the source ended', async () => {
    const { world, events, engine } = setup();
    const { ws, pc } = await handshake(world, engine);
    const cam = pc.receiveStream('42-camera', ['video']) as unknown as FakeStream;
    cam.removeAll();
    expect(events.remoteVideo).toHaveBeenLastCalledWith('42', 'camera', null);
    pc.receiveStream('7-screen', ['video']);
    ws.serve(VoiceOp.VIDEO, { t: 'published', user_id: '7', source: 'screen', active: false });
    expect(events.remoteVideo).toHaveBeenLastCalledWith('7', 'screen', null);
    expect(events.videoPublished).toHaveBeenLastCalledWith('7', 'screen', false, {
      width: 0,
      height: 0,
      fps: 0,
    });
  });

  it('reports what a source is published as and which layer it is sent at', async () => {
    const { world, events, engine } = setup();
    const { ws } = await handshake(world, engine);
    ws.serve(VoiceOp.VIDEO, {
      t: 'published',
      user_id: '7',
      source: 'camera',
      active: true,
      width: 1280,
      height: 720,
      fps: 30,
    });
    expect(events.videoPublished).toHaveBeenCalledWith('7', 'camera', true, {
      width: 1280,
      height: 720,
      fps: 30,
    });
    ws.serve(VoiceOp.VIDEO, {
      t: 'layer',
      user_id: '7',
      source: 'camera',
      layer: 'mid',
      reason: 'bandwidth',
    });
    expect(events.videoLayer).toHaveBeenCalledWith('7', 'camera', 'mid');
    // Garbage is ignored.
    ws.serve(VoiceOp.VIDEO, 'nonsense');
    ws.serve(VoiceOp.VIDEO, { t: 'who-knows' });
  });

  it('sends subscription hints and repeats them after every new offer', async () => {
    const { world, engine } = setup();
    const { ws } = await handshake(world, engine);
    const hints = [
      {
        user_id: '7',
        source: 'camera' as const,
        enabled: true,
        viewport: { width: 320, height: 180 },
      },
      { user_id: '8', source: 'screen' as const, enabled: true },
    ];
    engine.setVideoSubscriptions(hints);
    expect(published(ws).at(-1)).toEqual({ t: 'subscribe', subscriptions: hints });
    const n = published(ws).length;
    engine.restartIce();
    await flush();
    // The renegotiation offer is followed by the same hints.
    expect(published(ws).length).toBe(n + 1);
    expect(published(ws).at(-1)).toEqual({ t: 'subscribe', subscriptions: hints });
  });
});

describe('stream names', () => {
  it('splits a user id from the camera and screen suffix', () => {
    expect(parseStreamId('123')).toEqual({ userId: '123', kind: 'mic' });
    expect(parseStreamId('123-camera')).toEqual({ userId: '123', kind: 'camera' });
    expect(parseStreamId('123-screen')).toEqual({ userId: '123', kind: 'screen' });
  });
});
