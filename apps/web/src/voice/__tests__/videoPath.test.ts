import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Op } from '../../gateway/protocol';
import { VoiceController, HINT_DEBOUNCE_MS, type ControllerDeps } from '../controller';
import type { EngineEvents, PublishInfo, VoiceEngine } from '../engine';
import type { VideoHint } from '../protocol';
import {
  PREVIEW_FIRST_MS,
  PREVIEW_INTERVAL_MS,
  PreviewUploader,
  dataUriBytes,
  type PreviewDeps,
} from '../preview';
import { resetSession, useVoiceSession } from '../session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../settings';
import { applyStreamEvent, ownerOfKey, resetStreams, streamKey, useStreams } from '../streams';
import {
  FREE_PROFILES,
  SHARE_PRESETS,
  buildHints,
  resetLocalVideo,
  resetRemoteVideo,
  setTileViewport,
  setVideoOff,
  setWatching,
  useRemoteVideo,
  type VideoDeps,
} from '../video';
import { fakeAudio, FakeStream, FakeTrack } from './fakes';

class RecordingEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'connected';
  published: { source: string; stream: MediaStream; info: PublishInfo }[] = [];
  unpublished: string[] = [];
  hints: (readonly VideoHint[])[] = [];
  constructor(readonly events: EngineEvents) {}
  async connect() {}
  disconnect() {}
  setTransmitting() {}
  setDeafened() {}
  setUserVolume() {}
  setUserMuted() {}
  setMasterVolume() {}
  setInputVolume() {}
  async setInputDevice() {}
  async setOutputDevice() {}
  restartIce() {}
  publishVideo(source: 'camera' | 'screen', stream: MediaStream, info: PublishInfo) {
    this.published.push({ source, stream, info });
  }
  unpublishVideo(source: 'camera' | 'screen') {
    this.unpublished.push(source);
  }
  setVideoSubscriptions(h: readonly VideoHint[]) {
    this.hints.push(h);
  }
}

const videoStream = () =>
  new FakeStream([Object.assign(new FakeTrack('v'), { kind: 'video' })]) as unknown as MediaStream;
const videoDeps = (): VideoDeps => ({
  getUserMedia: async () => videoStream(),
  getDisplayMedia: async () => videoStream(),
  enumerateDevices: async () => [],
});

function harness(preview?: PreviewDeps) {
  const sent: { op: number; d: Record<string, unknown> }[] = [];
  const listeners = new Set<(e: string, d: unknown) => void>();
  const engines: RecordingEngine[] = [];
  const deps: ControllerDeps = {
    createEngine: (ev) => {
      const e = new RecordingEngine(ev);
      engines.push(e);
      return e;
    },
    sendGateway: (op, d) => (sent.push({ op, d: d as Record<string, unknown> }), true),
    subscribe: (l) => (listeners.add(l), () => listeners.delete(l)),
    currentUserId: () => 'me',
    voiceStatesOf: () => ({}),
    audio: fakeAudio().deps,
    now: () => 0,
    video: videoDeps(),
    ...(preview ? { preview } : {}),
  };
  const c = new VoiceController(deps);
  const emit = (e: string, d: unknown) => listeners.forEach((l) => l(e, d));
  c.join({ guildId: 'g1', channelId: 'c1', name: 'General' });
  emit('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c1', user_id: 'me' });
  emit('VOICE_SERVER_UPDATE', {
    guild_id: 'g1',
    channel_id: 'c1',
    endpoint: '/voice/ws',
    token: 't',
  });
  return { c, sent, emit, engines, engine: () => engines.at(-1)! };
}

const flush = async () => {
  for (let i = 0; i < 12; i++) await Promise.resolve();
};
const preset = SHARE_PRESETS[1]!;

beforeEach(() => {
  vi.useFakeTimers();
  resetSession();
  resetLocalVideo();
  resetRemoteVideo();
  resetStreams();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
});
afterEach(() => vi.useRealTimers());

describe('publishing from the controller', () => {
  it('hands the camera to the engine with its size and takes it back when it is turned off', async () => {
    const { c, engine } = harness();
    await flush();
    await c.startCamera(FREE_PROFILES, 'default');
    const e = engine();
    expect(e.published).toHaveLength(1);
    expect(e.published[0]!).toMatchObject({
      source: 'camera',
      info: { width: 1280, height: 720, fps: 30 },
    });
    c.stopCamera();
    expect(e.unpublished).toEqual(['camera']);
  });

  it('opens a stream with op 18 for a screen share and closes it with op 19', async () => {
    const { c, sent, engine } = harness();
    await flush();
    await c.startScreenShare({ preset, source: 'window', audio: true, hint: 'detail' });
    const e = engine();
    expect(e.published.at(-1)).toMatchObject({
      source: 'screen',
      info: { height: 720, fps: 30 },
    });
    expect(sent.find((m) => m.op === Op.STREAM_CREATE)!.d).toEqual({
      guild_id: 'g1',
      channel_id: 'c1',
      height: 720,
      fps: 30,
    });
    c.stopScreenShare();
    expect(e.unpublished).toContain('screen');
    expect(sent.find((m) => m.op === Op.STREAM_DELETE)!.d).toEqual({
      stream_key: 'guild:g1:c1:me',
    });
  });

  it('publishes the running captures again when a new engine takes over', async () => {
    const { c, sent, emit, engines } = harness();
    await flush();
    await c.startCamera(FREE_PROFILES, 'default');
    await c.startScreenShare({ preset, source: 'screen', audio: false, hint: 'motion' });
    const creates = sent.filter((m) => m.op === Op.STREAM_CREATE).length;
    // The voice session dies; the gateway issues a new server and a second engine starts.
    engines[0]!.events.fatal({ code: 'closed', message: 'gone' });
    emit('VOICE_SERVER_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      endpoint: '/voice/ws',
      token: 't2',
    });
    await flush();
    const second = engines[1]!;
    expect(second.published.map((p) => p.source).sort()).toEqual(['camera', 'screen']);
    // Leaving the channel ended the stream on the gateway, so it is opened again.
    expect(sent.filter((m) => m.op === Op.STREAM_CREATE)).toHaveLength(creates + 1);
    expect(second.hints.length).toBeGreaterThan(0);
  });

  it('drops a share the plan does not allow, with a note', async () => {
    const { c, emit } = harness();
    await flush();
    await c.startScreenShare({ preset, source: 'screen', audio: false, hint: 'motion' });
    expect(useVoiceSession.getState().screen).toBe(true);
    emit('SR_VOICE_ERROR', { channel_id: null, code: 'stream_quality_not_allowed' });
    expect(useVoiceSession.getState().screen).toBe(false);
    expect(useVoiceSession.getState().notice).toMatch(/above what your plan allows/);
  });
});

describe('watching streams', () => {
  it('asks the gateway with the stream key and follows the audience updates', async () => {
    const { c, sent, emit } = harness();
    await flush();
    c.watchStream('42');
    expect(useRemoteVideo.getState().watching['42']).toBe(true);
    expect(sent.find((m) => m.op === Op.STREAM_WATCH)!.d).toEqual({
      stream_key: 'guild:g1:c1:42',
    });
    emit('STREAM_CREATE', {
      stream_key: 'guild:g1:c1:42',
      viewer_ids: ['me'],
      max_viewers: 25,
      quality: { height: 720, fps: 30 },
      paused: false,
    });
    expect(useStreams.getState().byOwner['42']).toMatchObject({
      viewerIds: ['me'],
      maxViewers: 25,
      height: 720,
    });
    emit('STREAM_UPDATE', { stream_key: 'guild:g1:c1:42', viewer_ids: ['me', '77', '78'] });
    expect(useStreams.getState().byOwner['42']!.viewerIds).toHaveLength(3);
    // The streamer stops: we are no longer watching.
    emit('STREAM_DELETE', { stream_key: 'guild:g1:c1:42', reason: 'user_requested' });
    expect(useStreams.getState().byOwner['42']).toBeUndefined();
    expect(useRemoteVideo.getState().watching['42']).toBe(false);
  });

  it('stops watching with op 19 and un-watches when the stream is full', async () => {
    const { c, sent, emit } = harness();
    await flush();
    c.watchStream('42');
    emit('SR_VOICE_ERROR', { channel_id: null, code: 'stream_full', limit: 5, count: 5 });
    expect(useRemoteVideo.getState().watching['42']).toBe(false);
    expect(useVoiceSession.getState().refusal?.code).toBe('viewerCap');
    c.watchStream('43');
    c.unwatchStream('43');
    expect(sent.filter((m) => m.op === Op.STREAM_DELETE).at(-1)!.d).toEqual({
      stream_key: 'guild:g1:c1:43',
    });
  });

  it('tells the SFU which sources to forward, debounced, and only when something changed', async () => {
    const { c, engine } = harness();
    await flush();
    const e = engine();
    // The first sync after connecting is immediate.
    expect(e.hints).toHaveLength(1);
    c.watchStream('42');
    setTileViewport('42', 'screen', { width: 640, height: 360 });
    expect(e.hints).toHaveLength(1);
    vi.advanceTimersByTime(HINT_DEBOUNCE_MS);
    expect(e.hints).toHaveLength(2);
    expect(e.hints.at(-1)).toEqual([
      { user_id: '42', source: 'screen', enabled: true, viewport: { width: 640, height: 360 } },
    ]);
    // Same state again: nothing is sent.
    setTileViewport('42', 'screen', { width: 640, height: 360 });
    vi.advanceTimersByTime(HINT_DEBOUNCE_MS * 2);
    expect(e.hints).toHaveLength(2);
    c.unwatchStream('42');
    vi.advanceTimersByTime(HINT_DEBOUNCE_MS);
    expect(e.hints.at(-1)![0]).toMatchObject({ source: 'screen', enabled: false });
  });
});

describe('subscription hints', () => {
  const cam = { '7': {} as MediaStream, '8': {} as MediaStream };

  it('names every remote camera with its tile size and switches off the ones the viewer disabled', () => {
    setTileViewport('7', 'camera', { width: 320, height: 180 });
    setVideoOff('8', true);
    expect(
      buildHints({
        camera: cam,
        watching: {},
        videoOff: useRemoteVideo.getState().videoOff,
        viewports: useRemoteVideo.getState().viewports,
      }),
    ).toEqual([
      { user_id: '7', source: 'camera', enabled: true, viewport: { width: 320, height: 180 } },
      { user_id: '8', source: 'camera', enabled: false },
    ]);
  });

  it('keeps a screen the viewer stopped watching in the list, switched off', () => {
    setWatching('9', true);
    setWatching('10', false);
    const hints = buildHints({ ...useRemoteVideo.getState(), camera: {} });
    expect(hints).toEqual([
      { user_id: '10', source: 'screen', enabled: false },
      { user_id: '9', source: 'screen', enabled: true },
    ]);
  });

  it('only stores a tile size when it changed', () => {
    setTileViewport('7', 'camera', { width: 320, height: 180 });
    const before = useRemoteVideo.getState().viewports;
    setTileViewport('7', 'camera', { width: 320, height: 180 });
    expect(useRemoteVideo.getState().viewports).toBe(before);
    setTileViewport('7', 'camera', null);
    expect(useRemoteVideo.getState().viewports['7:camera']).toBeUndefined();
  });
});

describe('stream keys and events', () => {
  it('builds and reads keys for servers and DM calls', () => {
    expect(streamKey('g1', 'c1', 'u1')).toBe('guild:g1:c1:u1');
    expect(streamKey('@me', 'c1', 'u1')).toBe('call:c1:u1');
    expect(ownerOfKey('guild:g1:c1:u1')).toBe('u1');
    expect(ownerOfKey('call:c1:u1')).toBe('u1');
    expect(ownerOfKey('bogus')).toBeNull();
  });

  it('ignores events it does not know and malformed ones', () => {
    expect(applyStreamEvent('MESSAGE_CREATE', {}, 'me')).toEqual({});
    expect(applyStreamEvent('STREAM_UPDATE', { stream_key: 7 }, 'me')).toEqual({});
    expect(useStreams.getState().byOwner).toEqual({});
  });
});

describe('stream previews (VOX-CLI-008)', () => {
  const uri = `data:image/jpeg;base64,${'A'.repeat(100)}`;

  it('uploads a thumbnail soon after the share starts, then on a timer, and stops with it', async () => {
    const upload = vi.fn(async () => undefined);
    const capture = vi.fn(async () => uri);
    const u = new PreviewUploader({ capture, upload });
    u.start('guild:g1:c1:me', videoStream());
    expect(upload).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(PREVIEW_FIRST_MS);
    expect(upload).toHaveBeenCalledWith('guild:g1:c1:me', uri);
    await vi.advanceTimersByTimeAsync(PREVIEW_INTERVAL_MS);
    expect(upload).toHaveBeenCalledTimes(2);
    u.stop();
    await vi.advanceTimersByTimeAsync(PREVIEW_INTERVAL_MS * 3);
    expect(upload).toHaveBeenCalledTimes(2);
  });

  it('skips frames that are missing or too big and survives a failed upload', async () => {
    const frames = [null, `data:image/jpeg;base64,${'A'.repeat(400_000)}`, uri, uri];
    const upload = vi
      .fn<(k: string, d: string) => Promise<void>>()
      .mockRejectedValueOnce(new Error('429'))
      .mockResolvedValue(undefined);
    const u = new PreviewUploader({ capture: async () => frames.shift() ?? null, upload });
    u.start('k', videoStream());
    await vi.advanceTimersByTimeAsync(PREVIEW_FIRST_MS);
    expect(upload).not.toHaveBeenCalled(); // null frame
    await vi.advanceTimersByTimeAsync(PREVIEW_INTERVAL_MS);
    expect(upload).not.toHaveBeenCalled(); // over 256 KiB
    await vi.advanceTimersByTimeAsync(PREVIEW_INTERVAL_MS);
    expect(upload).toHaveBeenCalledTimes(1); // rejected, swallowed
    await vi.advanceTimersByTimeAsync(PREVIEW_INTERVAL_MS);
    expect(upload).toHaveBeenCalledTimes(2);
    u.stop();
  });

  it('measures a data URI by its decoded size', () => {
    expect(dataUriBytes('data:image/jpeg;base64,QUJD')).toBe(3);
    expect(dataUriBytes('data:image/jpeg;base64,QUI=')).toBe(2);
    expect(dataUriBytes('nope')).toBe(0);
  });

  it('starts when a share starts and stops with it', async () => {
    const upload = vi.fn(async () => undefined);
    const { c } = harness({ capture: async () => uri, upload });
    await flush();
    await c.startScreenShare({ preset, source: 'screen', audio: false, hint: 'motion' });
    await vi.advanceTimersByTimeAsync(PREVIEW_FIRST_MS);
    expect(upload).toHaveBeenCalledWith('guild:g1:c1:me', uri);
    c.stopScreenShare();
    await vi.advanceTimersByTimeAsync(PREVIEW_INTERVAL_MS * 2);
    expect(upload).toHaveBeenCalledTimes(1);
  });
});
