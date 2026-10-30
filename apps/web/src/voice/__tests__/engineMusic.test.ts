import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RtcVoiceEngine, type ConnectParams, type EngineEvents } from '../engine';
import {
  clampMusicKbps,
  parseStreamId,
  stereoAnswer,
  tuneOpus,
  tuneOpusOffer,
  VoiceOp,
} from '../protocol';
import {
  fakeEngineDeps,
  FakeStream,
  FakeTrack,
  type FakeEngineWorld,
  type FakePeer,
} from './fakes';

/**
 * Like Chrome, the fake peer's offer has one audio m-line per transceiver, each carrying the msid of the stream it
 * was added with and listing Opus 111 (and RED 63, which refers to it). The default canned offer has one line.
 */
function realisticOffers(world: FakeEngineWorld) {
  const create = world.deps.createPeer;
  world.deps.createPeer = (config) => {
    const pc = create(config) as FakePeer;
    const original = pc.createOffer.bind(pc);
    pc.createOffer = async (o) => {
      const base = await original(o);
      const sections = pc.transceivers.map((tx, i) => {
        const stream = (tx.init as { streams: { id: string }[] }).streams[0]!.id;
        return [
          'm=audio 9 UDP/TLS/RTP/SAVPF 111 63',
          `a=mid:${i}`,
          `a=msid:${stream} track${i}`,
          'a=rtpmap:111 opus/48000/2',
          'a=rtcp-fb:111 transport-cc',
          'a=fmtp:111 minptime=10;useinbandfec=1',
          'a=rtpmap:63 red/48000/2',
          'a=fmtp:63 111/111',
        ].join('\r\n');
      });
      return { ...base, sdp: ['v=0', ...sections, ''].join('\r\n') };
    };
    return pc;
  };
}

/** The m-line sections of a description, split into the mic (first) and the rest. */
const sectionsOf = (sdp: string) => sdp.split('\r\nm=').filter((s) => s.startsWith('audio'));

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
  realisticOffers(world);
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

describe('the per-line offer and the answer', () => {
  const offer = [
    'v=0',
    'm=audio 9 UDP/TLS/RTP/SAVPF 111 63',
    'a=mid:0',
    'a=msid:mic t0',
    'a=rtpmap:111 opus/48000/2',
    'a=fmtp:111 minptime=10;useinbandfec=1',
    'm=audio 9 UDP/TLS/RTP/SAVPF 111 63',
    'a=mid:1',
    'a=msid:song t1',
    'a=rtpmap:111 opus/48000/2',
    'a=fmtp:111 minptime=10;useinbandfec=1',
    '',
  ].join('\r\n');

  it('finds the music line by its stream id or its mid', () => {
    for (const line of [{ streamId: 'song' }, { streamId: 'nope', mid: '1' }]) {
      const [mic, song] = sectionsOf(tuneOpusOffer(offer, 64_000, { ...line, kbps: 256 }));
      expect(mic).not.toContain('stereo=1');
      expect(song).toContain('SAVPF 112');
      expect(song).toContain('stereo=1');
    }
  });

  it('with no music line every audio line is voice, and a clash on the stereo type keeps the music line mono', () => {
    const none = tuneOpusOffer(offer, 64_000, null);
    expect(none).not.toContain('stereo=1');
    expect(none).not.toContain('112');
    const clash = offer + 'a=rtpmap:112 VP8/90000\r\n';
    const out = tuneOpusOffer(clash, 64_000, { streamId: 'song', kbps: 256 });
    expect(out).not.toContain('stereo=1');
  });

  it('puts stereo=1 back on the stereo type in an answer, and nowhere else', () => {
    const answer = [
      'v=0',
      'm=audio 9 UDP/TLS/RTP/SAVPF 112',
      'a=rtpmap:112 opus/48000/2',
      'a=fmtp:112 minptime=10;useinbandfec=1',
      'm=audio 9 UDP/TLS/RTP/SAVPF 111',
      'a=rtpmap:111 opus/48000/2',
      'a=fmtp:111 minptime=10;useinbandfec=1',
      '',
    ].join('\r\n');
    const out = stereoAnswer(answer);
    expect(out).toContain('a=fmtp:112 minptime=10;useinbandfec=1;stereo=1;sprop-stereo=1');
    expect(out).toContain('a=fmtp:111 minptime=10;useinbandfec=1\r\n');
    expect(stereoAnswer('answer-sdp')).toBe('answer-sdp');
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

  it('keeps the microphone line mono and gives the music line its own stereo payload type', async () => {
    const { world, engine, params } = setup({ maxBitrate: 64_000 });
    const { ws, pc } = await handshake(world, engine, params);
    const first = pc.localDescription?.sdp ?? '';
    expect(first).toContain('stereo=0');
    expect(first).not.toContain('112');
    engine.publishMusic(musicStream(), 320);
    await flush();
    const [mic, song] = sectionsOf(pc.localDescription?.sdp ?? '');
    // The microphone line: mono, DTX, the channel bitrate, never a stereo fmtp, never the stereo type.
    expect(mic).toContain('SAVPF 111');
    expect(mic).toContain('stereo=0');
    expect(mic).toContain('usedtx=1');
    expect(mic).toContain('maxaveragebitrate=64000');
    expect(mic).not.toContain('stereo=1');
    expect(mic).not.toContain('sprop-stereo');
    expect(mic).not.toContain('112');
    // The music line: only the stereo type, stereo with sprop-stereo, no DTX, the music bitrate, no mono type.
    expect(song).toMatch(/^audio 9 UDP\/TLS\/RTP\/SAVPF 112\r\n/);
    expect(song).toContain('a=rtpmap:112 opus/48000/2');
    expect(song).toContain('a=rtcp-fb:112 transport-cc');
    expect(song).toMatch(/a=fmtp:112 [^\r\n]*stereo=1/);
    expect(song).toContain('sprop-stereo=1');
    expect(song).toContain('usedtx=0');
    expect(song).toContain('maxaveragebitrate=320000');
    expect(song).not.toContain('111');
    expect(song).not.toContain('a=rtpmap:63');
    ws.serve(VoiceOp.SDP_ANSWER, { sdp: 'answer-2' });
    await flush();

    engine.unpublishMusic();
    await flush();
    expect(published(ws).at(-1)).toMatchObject({ source: 'music', active: false });
    expect(pc.transceivers[1]!.stopped).toBe(true);
    const after = pc.localDescription?.sdp ?? '';
    expect(after).not.toContain('stereo=1');
    expect(after).not.toContain('112');
    expect(after).toContain('usedtx=1');
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
    const [mic, song] = sectionsOf(pc.localDescription?.sdp ?? '');
    expect(mic).not.toContain('stereo=1');
    expect(song).toContain('sprop-stereo=1');
    expect(song).toContain('SAVPF 112');
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
