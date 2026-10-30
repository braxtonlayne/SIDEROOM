import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RtcVoiceEngine, type ConnectParams, type EngineEvents } from '../engine';
import { clampMusicKbps, parseStreamId, tuneOpus, VoiceOp } from '../protocol';
import { fakeEngineDeps, FakeStream, FakeTrack, type FakeEngineWorld } from './fakes';

// MUS-JB-001: a stereo music track as a second audio sender, under the same per-sender frame encryptor as the
// microphone, announced to the SFU as `music`, with its own Opus profile and bitrate, and received on its own
// msid stream (`<user>-music`).

const READY = {
  session_id: 'vs-1',
  resume_token: 'rt-1',
  heartbeat_interval: 13_750,
  user_id: 'me',
  channel_id: 'c1',
  participants: [],
};

const flush = async (n = 16) => {
  for (let i = 0; i < n; i++) await Promise.resolve();
};

function setup(extra: Partial<ConnectParams> = {}) {
  const world = fakeEngineDeps();
  const events = {
    status: vi.fn(),
    speaking: vi.fn(),
    quality: vi.fn(),
    fatal: vi.fn(),
    level: vi.fn(),
    micLost: vi.fn(),
    peer: vi.fn(),
    remoteMusic: vi.fn(),
    videoPublished: vi.fn(),
  } satisfies EngineEvents;
  const e2ee = {
    peerConfig: {},
    attachSender: vi.fn(),
    attachReceiver: vi.fn(),
  };
  const params: ConnectParams = {
    endpoint: 'wss://chat.example.org/voice/ws',
    token: 'v1.claims.mac',
    userId: 'me',
    mic: { deviceId: 'default', echoCancellation: true, autoGainControl: true, inputVolume: 100 },
    e2ee,
    ...extra,
  };
  const engine = new RtcVoiceEngine(events, world.deps);
  return { world, events, engine, params, e2ee };
}

async function handshake(world: FakeEngineWorld, engine: RtcVoiceEngine, params: ConnectParams) {
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

/** What the player's MediaStreamAudioDestinationNode hands over: one stereo audio track. */
const musicStream = () =>
  new FakeStream([new FakeTrack('music')], 'player') as unknown as MediaStream;

const published = (ws: { ops(op: number): { d?: unknown }[] }) =>
  ws.ops(VoiceOp.VIDEO).map((f) => f.d as Record<string, unknown>);

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('tuneOpus profiles', () => {
  const sdp = 'a=rtpmap:111 opus/48000/2\r\na=fmtp:111 minptime=10;useinbandfec=1\r\n';

  it('keeps voice mono with DTX and makes the music profile stereo without DTX', () => {
    const voice = tuneOpus(sdp, 64_000);
    expect(voice).toContain('stereo=0');
    expect(voice).toContain('usedtx=1');
    expect(voice).toContain('maxaveragebitrate=64000');
    expect(voice).not.toContain('sprop-stereo');
    const music = tuneOpus(sdp, 256_000, 'music');
    expect(music).toContain('stereo=1');
    expect(music).toContain('sprop-stereo=1');
    expect(music).toContain('usedtx=0');
    expect(music).toContain('useinbandfec=1');
    expect(music).toContain('maxaveragebitrate=256000');
  });

  it('clamps the music bitrate to 32..320 kbit/s with 256 as the default', () => {
    expect(clampMusicKbps(undefined)).toBe(256);
    expect(clampMusicKbps(999)).toBe(320);
    expect(clampMusicKbps(1)).toBe(32);
    expect(clampMusicKbps(128)).toBe(128);
  });

  it('names the music stream group', () => {
    expect(parseStreamId('42-music')).toEqual({ userId: '42', kind: 'music' });
    expect(parseStreamId('42-screen')).toEqual({ userId: '42', kind: 'screen' });
    expect(parseStreamId('42')).toEqual({ userId: '42', kind: 'mic' });
  });
});

describe('sending music', () => {
  it('adds a second sendonly audio line under the same E2EE hook, announced as music', async () => {
    const { world, engine, params, e2ee } = setup();
    const { ws, pc } = await handshake(world, engine, params);
    engine.publishMusic(musicStream());
    await flush();
    // The mic line plus the music line.
    expect(pc.transceivers).toHaveLength(2);
    const music = pc.transceivers[1]!;
    const init = music.init as { direction: string; sendEncodings: { maxBitrate: number }[] };
    expect(init.direction).toBe('sendonly');
    expect(init.sendEncodings[0]!.maxBitrate).toBe(256_000);
    // The same per-sender encryptor hook sees both audio senders (D-249).
    expect(e2ee.attachSender).toHaveBeenCalledTimes(2);
    expect(e2ee.attachSender.mock.calls.map((c) => c[1])).toEqual(['audio', 'audio']);
    expect(published(ws)).toEqual([
      { t: 'publish', mid: music.mid, source: 'music', active: true, width: 0, height: 0, fps: 0 },
    ]);
  });

  it('negotiates the stereo profile while music is attached and the voice profile again after', async () => {
    const { world, engine, params } = setup({ maxBitrate: 64_000 });
    const { ws, pc } = await handshake(world, engine, params);
    expect(pc.localDescription?.sdp).toContain('stereo=0');
    engine.publishMusic(musicStream(), 320);
    await flush();
    const withMusic = pc.localDescription?.sdp ?? '';
    expect(withMusic).toContain('stereo=1');
    expect(withMusic).toContain('sprop-stereo=1');
    expect(withMusic).toContain('usedtx=0');
    expect(withMusic).toContain('maxaveragebitrate=320000');
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-2' });
    await flush();

    engine.unpublishMusic();
    await flush();
    expect(published(ws).at(-1)).toMatchObject({ source: 'music', active: false });
    expect(pc.transceivers[1]!.stopped).toBe(true);
    const after = pc.localDescription?.sdp ?? '';
    expect(after).toContain('stereo=0');
    expect(after).toContain('usedtx=1');
    expect(after).toContain('maxaveragebitrate=64000');
  });

  it('uses the channel bitrate from the connect params and never goes above 320 kbit/s', async () => {
    const a = setup({ musicKbps: 128 });
    const ha = await handshake(a.world, a.engine, a.params);
    a.engine.publishMusic(musicStream());
    await flush();
    expect(
      (ha.pc.transceivers[1]!.init as { sendEncodings: { maxBitrate: number }[] }).sendEncodings[0]!
        .maxBitrate,
    ).toBe(128_000);

    const b = setup();
    const hb = await handshake(b.world, b.engine, b.params);
    b.engine.publishMusic(musicStream(), 5000);
    await flush();
    expect(
      (hb.pc.transceivers[1]!.init as { sendEncodings: { maxBitrate: number }[] }).sendEncodings[0]!
        .maxBitrate,
    ).toBe(320_000);
  });

  it('adds the music line to the first offer when it was attached before the peer existed', async () => {
    const { world, engine, params } = setup();
    const done = engine.connect(params);
    done.catch(() => undefined);
    await flush();
    engine.publishMusic(musicStream());
    const ws = world.sockets[0]!;
    ws.open();
    ws.serve(VoiceOp.READY, READY);
    await flush();
    const pc = world.peers[0]!;
    expect(pc.transceivers).toHaveLength(2);
    expect(published(ws)).toHaveLength(1);
    expect(pc.localDescription?.sdp ?? '').toContain('sprop-stereo=1');
  });

  it('ignores a stream without an audio track', async () => {
    const { world, engine, params } = setup();
    const { pc } = await handshake(world, engine, params);
    engine.publishMusic(new FakeStream([]) as unknown as MediaStream);
    await flush();
    expect(pc.transceivers).toHaveLength(1);
  });
});

describe('receiving music', () => {
  it('plays <user>-music separately from the voice, under the E2EE receiver hook for that user', async () => {
    const { world, engine, params, events, e2ee } = setup();
    const { pc } = await handshake(world, engine, params);
    const stream = pc.receiveStream('u2-music', ['audio']);
    expect(events.remoteMusic).toHaveBeenCalledWith('u2', stream);
    // It is not mistaken for u2's microphone.
    expect(events.peer).not.toHaveBeenCalled();
    expect(e2ee.attachReceiver).toHaveBeenCalledWith(undefined, 'audio', 'u2');
    // It is played (one element for the music).
    expect(world.audio.elements).toHaveLength(1);
    // The DJ stops: the SFU removes the line and the music is dropped.
    (stream as unknown as FakeStream).removeAll();
    expect(events.remoteMusic).toHaveBeenLastCalledWith('u2', null);
  });

  it('a listener mutes music separately: playback is silent and the SFU is asked to stop sending it', async () => {
    const { world, engine, params } = setup();
    const { ws, pc } = await handshake(world, engine, params);
    pc.receiveStream('u2-music', ['audio']);
    ws.serve(VoiceOp.VIDEO, { t: 'published', user_id: 'u2', source: 'music', active: true });
    await flush();
    engine.setVideoSubscriptions([{ user_id: 'u3', source: 'camera', layer: 'low' }]);
    engine.setMusicMuted(true);
    const subs = published(ws).filter((m) => m.t === 'subscribe');
    const last = subs.at(-1)!.subscriptions as {
      user_id: string;
      source: string;
      enabled?: boolean;
    }[];
    // The caller's hints are kept and the music mute is added for the DJ.
    expect(last).toContainEqual({ user_id: 'u3', source: 'camera', layer: 'low' });
    expect(last).toContainEqual({ user_id: 'u2', source: 'music', enabled: false });
    // Unmuting sends the list without it.
    engine.setMusicMuted(false);
    const again = published(ws)
      .filter((m) => m.t === 'subscribe')
      .at(-1)!.subscriptions as {
      source: string;
    }[];
    expect(again.some((h) => h.source === 'music')).toBe(false);
  });

  it('forgets the music of someone who leaves', async () => {
    const { world, engine, params, events } = setup();
    const { ws, pc } = await handshake(world, engine, params);
    pc.receiveStream('u2-music', ['audio']);
    ws.serve(VoiceOp.CLIENT_DISCONNECT, { user_id: 'u2' });
    await flush();
    expect(events.remoteMusic).toHaveBeenLastCalledWith('u2', null);
  });
});

describe('music playback', () => {
  it('has its own volume and mute, is ducked by a priority speaker and silenced by deafen', async () => {
    const { RemoteAudio, DUCK } = await import('../playback');
    const { fakeAudio } = await import('./fakes');
    const w = fakeAudio();
    const r = new RemoteAudio(w.deps);
    const voice = new FakeStream([new FakeTrack('v')], 'u9') as unknown as MediaStream;
    const song = new FakeStream([new FakeTrack('m')], 'u2-music') as unknown as MediaStream;
    r.add('u9', voice);
    r.addMusic('u2', song);
    const gains = w.contexts[0]!.gains;
    // gains[0] is the master; then the voice, then the music.
    const [, voiceGain, musicGain] = gains;
    expect(r.users).toEqual(['u9']);
    expect(r.hasMusic('u2')).toBe(true);

    r.setMusicVolume(50);
    expect(musicGain!.gain.value).toBeCloseTo(0.5);
    expect(voiceGain!.gain.value).toBe(1);

    r.setMusicMuted(true);
    expect(musicGain!.gain.value).toBe(0);
    expect(voiceGain!.gain.value).toBe(1);
    r.setMusicMuted(false);

    // A priority speaker ducks the music like every other voice but their own.
    r.setPriority('u9', true);
    expect(musicGain!.gain.value).toBeCloseTo(0.5 * DUCK);
    expect(voiceGain!.gain.value).toBe(1);
    r.setPriority('u9', false);
    expect(musicGain!.gain.value).toBeCloseTo(0.5);

    // A music track that starts after the mute is silent too.
    r.setMusicMuted(true);
    r.addMusic('u3', song);
    expect(w.contexts[0]!.gains[3]!.gain.value).toBe(0);

    r.removeMusic('u2');
    expect(r.hasMusic('u2')).toBe(false);
  });
});

describe('tapping the player for voice', () => {
  it('connects the player node to a two-channel stream destination and detaches cleanly', async () => {
    const { tapMusicForVoice } = await import('../playback');
    const { FakeAudioContext, FakeNode } = await import('./fakes');
    const ctx = new FakeAudioContext();
    const node = Object.assign(new FakeNode(), { disconnect: vi.fn() });
    const tap = tapMusicForVoice(ctx as unknown as AudioContext, node as unknown as AudioNode)!;
    expect(tap.stream).toBe(ctx.dests[0]!.stream);
    expect(node.connected).toContain(ctx.dests[0]);
    expect(ctx.dests[0]).toMatchObject({ channelCount: 2, channelCountMode: 'explicit' });
    tap.detach();
    expect(node.disconnect).toHaveBeenCalledWith(ctx.dests[0]);
    expect(tapMusicForVoice({} as AudioContext, node as unknown as AudioNode)).toBeNull();
  });
});
