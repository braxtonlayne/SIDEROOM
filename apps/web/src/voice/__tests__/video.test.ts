import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Op } from '../../gateway/protocol';
import { VoiceController, type ControllerDeps } from '../controller';
import type { EngineEvents, VoiceEngine } from '../engine';
import { IDLE_SESSION, resetSession, useVoiceSession } from '../session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../settings';
import {
  allowedProfiles,
  bestProfile,
  cameraConstraints,
  displayConstraints,
  fitsProfiles,
  FREE_PROFILES,
  openCamera,
  openScreen,
  presetLocked,
  resetLocalVideo,
  SHARE_PRESETS,
  simulcastEncodings,
  toCaptureError,
  useLocalVideo,
  type VideoDeps,
} from '../video';
import { fakeAudio, FakeTrack } from './fakes';

class VTrack extends FakeTrack {
  contentHint = '';
  constructor(kind: 'video' | 'audio') {
    super(kind);
    this.kind = kind;
  }
}
class VStream {
  constructor(public tracks: VTrack[]) {}
  getVideoTracks() {
    return this.tracks.filter((t) => t.kind === 'video') as unknown as MediaStreamTrack[];
  }
  getAudioTracks() {
    return this.tracks.filter((t) => t.kind === 'audio') as unknown as MediaStreamTrack[];
  }
  getTracks() {
    return this.tracks as unknown as MediaStreamTrack[];
  }
}
const stream = (audio = false) =>
  new VStream([new VTrack('video'), ...(audio ? [new VTrack('audio')] : [])]);

function videoDeps(over: Partial<VideoDeps> = {}): VideoDeps & {
  last: VStream | null;
  calls: unknown[];
} {
  const state = { last: null as VStream | null, calls: [] as unknown[] };
  const mk = (audio: boolean) => {
    state.last = stream(audio);
    return state.last as unknown as MediaStream;
  };
  return Object.assign(state, {
    getUserMedia: async (c: MediaStreamConstraints) => (state.calls.push(c), mk(false)),
    getDisplayMedia: async (o: DisplayMediaStreamOptions) => (state.calls.push(o), mk(!!o.audio)),
    enumerateDevices: async () => [],
    ...over,
  });
}

describe('entitlement caps', () => {
  it('a request fits when one profile covers both height and frame rate', () => {
    const p = [
      { height: 720, fps: 60 },
      { height: 1080, fps: 30 },
    ];
    expect(fitsProfiles(p, 720, 60)).toBe(true);
    expect(fitsProfiles(p, 1080, 30)).toBe(true);
    expect(fitsProfiles(p, 1080, 60)).toBe(false);
    expect(fitsProfiles(FREE_PROFILES, 720, 30)).toBe(true);
    expect(fitsProfiles(FREE_PROFILES, 720, 60)).toBe(false);
  });

  it('merges member and server profiles and falls back to the free tier', () => {
    expect(allowedProfiles(undefined, null)).toEqual(FREE_PROFILES);
    expect(allowedProfiles([{ height: 720, fps: 30 }], [{ height: 1080, fps: 60 }])).toHaveLength(
      2,
    );
  });

  it('locks the presets above the profiles and picks the best one for the camera', () => {
    const locked = SHARE_PRESETS.filter((p) => presetLocked(p, FREE_PROFILES)).map((p) => p.id);
    expect(locked).toEqual(['720p60', '1080p30', '1080p60']);
    expect(SHARE_PRESETS.some((p) => presetLocked(p, [{ height: 1080, fps: 60 }]))).toBe(false);
    expect(
      bestProfile([
        { height: 720, fps: 60 },
        { height: 1080, fps: 30 },
      ]),
    ).toEqual({
      height: 720,
      fps: 60,
    });
    expect(bestProfile([])).toEqual(FREE_PROFILES[0]);
  });
});

describe('constraints and encodings', () => {
  it('builds display constraints from the preset, source and audio choice', () => {
    const preset = SHARE_PRESETS.find((p) => p.id === '720p30')!;
    const c = displayConstraints(preset, true, 'tab');
    expect(c.audio).toBe(true);
    expect(c.video).toMatchObject({
      height: { ideal: 720 },
      frameRate: { ideal: 30, max: 30 },
      displaySurface: 'browser',
    });
    expect(
      (displayConstraints(preset, false, 'screen').video as { displaySurface: string })
        .displaySurface,
    ).toBe('monitor');
    expect(
      (displayConstraints(preset, false, 'window').video as { displaySurface: string })
        .displaySurface,
    ).toBe('window');
  });

  it('pins a chosen camera and leaves the default alone', () => {
    const p = { height: 720, fps: 30 };
    expect((cameraConstraints('cam-1', p).video as MediaTrackConstraints).deviceId).toEqual({
      exact: 'cam-1',
    });
    expect(
      (cameraConstraints('default', p).video as MediaTrackConstraints).deviceId,
    ).toBeUndefined();
  });

  it('simulcast has three layers: full, half, quarter, with falling bitrate', () => {
    const enc = simulcastEncodings({ height: 720, fps: 30 });
    expect(enc.map((e) => e.rid)).toEqual(['f', 'h', 'q']);
    expect(enc.map((e) => e.scaleResolutionDownBy)).toEqual([1, 2, 4]);
    expect(enc[0]!.maxBitrate!).toBeGreaterThan(enc[1]!.maxBitrate!);
    expect(enc[1]!.maxBitrate!).toBeGreaterThan(enc[2]!.maxBitrate!);
    expect(enc[2]!.maxFramerate).toBe(15);
  });
});

describe('capture', () => {
  it('opens the camera clamped to the best profile', async () => {
    const deps = videoDeps();
    const cam = await openCamera(deps, 'cam-1', [{ height: 1080, fps: 60 }]);
    expect(cam.deviceId).toBe('cam-1');
    expect((deps.calls[0] as MediaStreamConstraints).video).toMatchObject({
      height: { ideal: 1080 },
      frameRate: { ideal: 60, max: 60 },
    });
  });

  it('falls back to the default camera when the remembered one is gone', async () => {
    let n = 0;
    const deps = videoDeps({
      getUserMedia: async () => {
        n++;
        if (n === 1) throw new DOMException('gone', 'OverconstrainedError');
        return stream() as unknown as MediaStream;
      },
    });
    await openCamera(deps, 'old-cam', FREE_PROFILES);
    expect(n).toBe(2);
  });

  it('turns a blocked camera into a message and a closed picker into a silent cancel', async () => {
    expect(toCaptureError(new DOMException('x', 'NotAllowedError'), 'camera').code).toBe('denied');
    expect(toCaptureError(new DOMException('x', 'NotAllowedError'), 'screen').code).toBe(
      'cancelled',
    );
    expect(toCaptureError(new DOMException('x', 'NotFoundError'), 'camera').code).toBe('no-device');
    expect(toCaptureError(new DOMException('x', 'NotReadableError'), 'camera').code).toBe('in-use');
    expect(toCaptureError(new DOMException('x', 'NotSupportedError'), 'screen').code).toBe(
      'unsupported',
    );
  });

  it('applies the content hint and reports whether audio came with the share', async () => {
    const deps = videoDeps();
    const preset = SHARE_PRESETS[1]!;
    const withAudio = await openScreen(deps, {
      preset,
      source: 'tab',
      audio: true,
      hint: 'detail',
    });
    expect(withAudio.hasAudio).toBe(true);
    expect((deps.last!.tracks[0] as VTrack).contentHint).toBe('detail');
    const without = await openScreen(deps, {
      preset,
      source: 'screen',
      audio: false,
      hint: 'motion',
    });
    expect(without.hasAudio).toBe(false);
    expect((deps.last!.tracks[0] as VTrack).contentHint).toBe('motion');
  });
});

class MockEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'connected';
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
}

function harness(publishVideo: boolean | undefined, video: VideoDeps) {
  const sent: { op: number; d: Record<string, unknown> }[] = [];
  const listeners = new Set<(e: string, d: unknown) => void>();
  const deps: ControllerDeps = {
    createEngine: (ev) => new MockEngine(ev),
    sendGateway: (op, d) => (sent.push({ op, d: d as Record<string, unknown> }), true),
    subscribe: (l) => (listeners.add(l), () => listeners.delete(l)),
    currentUserId: () => 'me',
    voiceStatesOf: () => ({}),
    audio: fakeAudio().deps,
    now: () => 0,
    video,
    ...(publishVideo === undefined ? {} : { publishVideo }),
  };
  const c = new VoiceController(deps);
  c.join({ guildId: 'g1', channelId: 'c1', name: 'General' });
  return { c, sent };
}
const sess = () => useVoiceSession.getState();
const flush = async () => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};
const preset = SHARE_PRESETS[1]!;

describe('controller capture', () => {
  beforeEach(() => {
    vi.useRealTimers();
    resetSession();
    resetLocalVideo();
    useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
  });

  it('camera on and off updates the session and stops the tracks', async () => {
    const deps = videoDeps();
    const { c } = harness(undefined, deps);
    await c.startCamera(FREE_PROFILES, 'default');
    expect(sess().camera).toBe(true);
    const tracks = deps.last!.tracks;
    c.stopCamera();
    expect(sess().camera).toBe(false);
    expect(tracks.every((t) => t.stopped)).toBe(true);
    expect(useLocalVideo.getState().camera).toBeNull();
  });

  it('does not announce video in the voice state when publishing is off', async () => {
    const { c, sent } = harness(false, videoDeps());
    const before = sent.length;
    await c.startCamera(FREE_PROFILES);
    await c.startScreenShare({ preset, source: 'screen', audio: false, hint: 'motion' });
    expect(sent).toHaveLength(before);
    expect(sess().camera && sess().screen).toBe(true);
  });

  it('announces self_video and self_stream when publishing is on', async () => {
    const { c, sent } = harness(true, videoDeps());
    await c.startCamera(FREE_PROFILES);
    expect(sent.at(-1)).toMatchObject({
      op: Op.VOICE_STATE_UPDATE,
      d: { self_video: true, self_stream: false },
    });
    await c.startScreenShare({ preset, source: 'window', audio: true, hint: 'detail' });
    expect(sent.at(-1)!.d).toMatchObject({ self_video: true, self_stream: true });
    c.stopScreenShare();
    expect(sent.at(-1)!.d).toMatchObject({ self_video: true, self_stream: false });
  });

  it('the browser "Stop sharing" button ends the share', async () => {
    const deps = videoDeps();
    const { c } = harness(undefined, deps);
    await c.startScreenShare({ preset, source: 'screen', audio: false, hint: 'motion' });
    expect(sess().screen).toBe(true);
    (deps.last!.tracks[0] as VTrack).fireEnded();
    expect(sess().screen).toBe(false);
    expect(useLocalVideo.getState().screen).toBeNull();
  });

  it('a cancelled picker is silent, a blocked camera shows a message', async () => {
    const { c } = harness(
      undefined,
      videoDeps({
        getDisplayMedia: async () => {
          throw new DOMException('x', 'NotAllowedError');
        },
        getUserMedia: async () => {
          throw new DOMException('x', 'NotAllowedError');
        },
      }),
    );
    await c.startScreenShare({ preset, source: 'screen', audio: false, hint: 'motion' });
    expect(useLocalVideo.getState().error).toBeNull();
    expect(sess().screen).toBe(false);
    await c.startCamera(FREE_PROFILES);
    expect(useLocalVideo.getState().error).toMatch(/camera/i);
    c.dismissVideoError();
    expect(useLocalVideo.getState().error).toBeNull();
  });

  it('a camera that disappears mid-call turns off with a message', async () => {
    const deps = videoDeps();
    const { c } = harness(undefined, deps);
    await c.startCamera(FREE_PROFILES);
    (deps.last!.tracks[0] as VTrack).fireEnded();
    expect(sess().camera).toBe(false);
    expect(useLocalVideo.getState().error).toMatch(/camera/i);
  });

  it('leaving the call releases the camera and the screen', async () => {
    const deps = videoDeps();
    const { c } = harness(undefined, deps);
    await c.startCamera(FREE_PROFILES);
    const cam = deps.last!.tracks;
    await c.startScreenShare({ preset, source: 'screen', audio: true, hint: 'motion' });
    const scr = deps.last!.tracks;
    c.leave();
    await flush();
    expect([...cam, ...scr].every((t) => t.stopped)).toBe(true);
    expect(sess()).toMatchObject({ camera: false, screen: false });
    expect(useLocalVideo.getState().camera).toBeNull();
  });

  it('setShareHint changes the hint of the running share', async () => {
    const deps = videoDeps();
    const { c } = harness(undefined, deps);
    await c.startScreenShare({ preset, source: 'screen', audio: false, hint: 'motion' });
    c.setShareHint('detail');
    expect((deps.last!.tracks[0] as VTrack).contentHint).toBe('detail');
    expect(useLocalVideo.getState().screen?.hint).toBe('detail');
  });

  it('ignores capture requests when not in a call', async () => {
    const deps = videoDeps();
    const c = new VoiceController({
      createEngine: (ev) => new MockEngine(ev),
      sendGateway: () => true,
      subscribe: () => () => undefined,
      currentUserId: () => 'me',
      voiceStatesOf: () => ({}),
      audio: fakeAudio().deps,
      now: () => 0,
      video: deps,
    });
    await c.startCamera(FREE_PROFILES);
    expect(deps.calls).toHaveLength(0);
    expect(sess()).toMatchObject({ ...IDLE_SESSION, camera: false });
  });
});
