/**
 * Camera and screen-share capture (VOX-CLI-006 / VOX-CLI-007): constraints, entitlement caps, simulcast
 * encodings and the local capture store. Everything the platform provides goes through `VideoDeps`, so tests
 * inject fakes and never open a real camera or screen.
 *
 * Publishing to the SFU is separate and gated by `VIDEO_PUBLISH` (protocol.ts); the engine (engine.ts) puts a
 * capture on its own lines (camera: three simulcast layers) and the controller announces it.
 */
import { create } from 'zustand';
import { t } from '../i18n';
import type { VideoHint } from './protocol';

/** A camera / screen profile the viewer may send (`PerkSet.video_profiles` on the server). */
export interface VideoProfile {
  height: number;
  fps: number;
}

/** What a free account may send (docs/specs/lounge-voice-video.md: 720p30). */
export const FREE_PROFILES: readonly VideoProfile[] = [{ height: 720, fps: 30 }];

/** A request fits when it is within at least one allowed profile (same rule as `Limits::allows_video`). */
export function fitsProfiles(profiles: readonly VideoProfile[], height: number, fps: number) {
  return profiles.some((p) => height <= p.height && fps <= p.fps);
}

/**
 * The profiles the viewer may send in a server: their own (Patron or free, `GET /donations/me` limits) and
 * the server's level perks (`GET /guilds/{id}/support`); a request is allowed when either covers it.
 */
export function allowedProfiles(
  ...sources: (readonly VideoProfile[] | null | undefined)[]
): VideoProfile[] {
  const all = sources.flatMap((s) => s ?? []);
  return all.length ? all : [...FREE_PROFILES];
}

export type ShareSource = 'screen' | 'window' | 'tab';
export type ContentHint = 'motion' | 'detail';

export interface SharePreset {
  id: string;
  height: number;
  fps: number;
}

/** The resolution / frame-rate choices of the picker, lowest first. */
export const SHARE_PRESETS: readonly SharePreset[] = [
  { id: '480p30', height: 480, fps: 30 },
  { id: '720p30', height: 720, fps: 30 },
  { id: '720p60', height: 720, fps: 60 },
  { id: '1080p30', height: 1080, fps: 30 },
  { id: '1080p60', height: 1080, fps: 60 },
];

export const DEFAULT_PRESET_ID = '720p30';

export const presetLabel = (p: SharePreset) => `${p.height}p ${p.fps} fps`;

/** True when the viewer's entitlements do not cover the preset (the picker shows an upgrade hint). */
export const presetLocked = (p: SharePreset, profiles: readonly VideoProfile[]) =>
  !fitsProfiles(profiles, p.height, p.fps);

/** The highest-quality profile of a set, for clamping the camera. */
export function bestProfile(profiles: readonly VideoProfile[]): VideoProfile {
  const list = profiles.length ? profiles : FREE_PROFILES;
  return list.reduce((a, b) => (b.height * b.fps > a.height * a.fps ? b : a));
}

const WIDTH_FOR_HEIGHT = (h: number) => Math.round((h * 16) / 9);

export function displayConstraints(
  preset: SharePreset,
  audio: boolean,
  source: ShareSource,
): DisplayMediaStreamOptions {
  const surface = source === 'screen' ? 'monitor' : source === 'window' ? 'window' : 'browser';
  return {
    video: {
      width: { ideal: WIDTH_FOR_HEIGHT(preset.height) },
      height: { ideal: preset.height },
      frameRate: { ideal: preset.fps, max: preset.fps },
      displaySurface: surface,
    } as MediaTrackConstraints,
    audio,
  };
}

export function cameraConstraints(deviceId: string, profile: VideoProfile): MediaStreamConstraints {
  return {
    audio: false,
    video: {
      ...(deviceId && deviceId !== 'default' ? { deviceId: { exact: deviceId } } : {}),
      width: { ideal: WIDTH_FOR_HEIGHT(profile.height) },
      height: { ideal: profile.height },
      frameRate: { ideal: profile.fps, max: profile.fps },
    },
  };
}

/** Three simulcast layers (full, half, quarter) with bitrate caps by height, for `addTransceiver`. */
export function simulcastEncodings(profile: VideoProfile): RTCRtpEncodingParameters[] {
  const kbps = (h: number, fps: number) =>
    Math.round(
      (h <= 180 ? 250 : h <= 360 ? 600 : h <= 480 ? 900 : h <= 720 ? 1700 : 3500) *
        (fps > 30 ? 1.4 : 1),
    );
  const layers: [string, number][] = [
    ['f', 1],
    ['h', 2],
    ['q', 4],
  ];
  return layers.map(([rid, scale]) => ({
    rid,
    scaleResolutionDownBy: scale,
    maxFramerate: scale === 4 ? Math.min(15, profile.fps) : profile.fps,
    maxBitrate: kbps(profile.height / scale, profile.fps) * 1000,
    active: true,
  }));
}

export interface VideoDeps {
  getUserMedia(c: MediaStreamConstraints): Promise<MediaStream>;
  getDisplayMedia(o: DisplayMediaStreamOptions): Promise<MediaStream>;
  enumerateDevices(): Promise<MediaDeviceInfo[]>;
}

export const browserVideoDeps: VideoDeps = {
  getUserMedia: (c) => {
    const md = navigator.mediaDevices;
    if (!md?.getUserMedia)
      return Promise.reject(new DOMException('No media devices', 'NotFoundError'));
    return md.getUserMedia(c);
  },
  getDisplayMedia: (o) => {
    const md = navigator.mediaDevices;
    if (!md?.getDisplayMedia)
      return Promise.reject(new DOMException('No screen capture', 'NotSupportedError'));
    return md.getDisplayMedia(o);
  },
  enumerateDevices: async () => {
    const md = navigator.mediaDevices;
    return md?.enumerateDevices ? md.enumerateDevices() : [];
  },
};

export type CaptureErrorCode =
  'denied' | 'no-device' | 'in-use' | 'unsupported' | 'cancelled' | 'unknown';

export class CaptureError extends Error {
  constructor(
    readonly code: CaptureErrorCode,
    message: string,
  ) {
    super(message);
    this.name = 'CaptureError';
  }
}

/** Map a getUserMedia / getDisplayMedia failure to words a person can act on. `cancelled` is not an error. */
export function toCaptureError(e: unknown, kind: 'camera' | 'screen'): CaptureError {
  if (e instanceof CaptureError) return e;
  const name = nameOf(e);
  switch (name) {
    case 'NotAllowedError':
    case 'SecurityError':
    case 'PermissionDeniedError':
      // For a screen share the browser reports "denied" when the person closes the picker.
      return kind === 'screen'
        ? new CaptureError('cancelled', '')
        : new CaptureError('denied', t('voice.capture.cameraBlocked'));
    case 'NotFoundError':
    case 'DevicesNotFoundError':
    case 'OverconstrainedError':
      return new CaptureError('no-device', t('voice.capture.noCamera'));
    case 'NotReadableError':
    case 'AbortError':
      return new CaptureError(
        'in-use',
        kind === 'camera' ? t('voice.capture.cameraBusy') : t('voice.capture.screenFailed'),
      );
    case 'NotSupportedError':
      return new CaptureError('unsupported', t('voice.capture.screenUnsupported'));
    default:
      return new CaptureError(
        'unknown',
        kind === 'camera' ? t('voice.capture.cameraFailed') : t('voice.capture.screenFailed'),
      );
  }
}

export interface ShareOptions {
  preset: SharePreset;
  source: ShareSource;
  /** Also send the tab / system audio, where the browser offers it. */
  audio: boolean;
  hint: ContentHint;
}

export interface LocalCamera {
  stream: MediaStream;
  deviceId: string;
}
export interface LocalScreen extends ShareOptions {
  stream: MediaStream;
  /** True when the browser actually gave an audio track. */
  hasAudio: boolean;
}

interface LocalVideoState {
  camera: LocalCamera | null;
  screen: LocalScreen | null;
  error: string | null;
}

/** The viewer's own captures. The VoiceController is the only writer; tiles read the streams from here. */
export const useLocalVideo = create<LocalVideoState>()(() => ({
  camera: null,
  screen: null,
  error: null,
}));

export const resetLocalVideo = () =>
  useLocalVideo.setState({ camera: null, screen: null, error: null });

const stopStream = (s: MediaStream | undefined | null) => s?.getTracks().forEach((tr) => tr.stop());

/** Apply `contentHint` (motion / detail) to the video track, where the browser supports it. */
export function applyContentHint(stream: MediaStream, hint: ContentHint): void {
  for (const tr of stream.getVideoTracks()) {
    try {
      (tr as MediaStreamTrack & { contentHint?: string }).contentHint = hint;
    } catch {
      /* not supported */
    }
  }
}

/** Open the camera, clamped to the best profile the viewer may send. */
export async function openCamera(
  deps: VideoDeps,
  deviceId: string,
  profiles: readonly VideoProfile[],
): Promise<LocalCamera> {
  const profile = bestProfile(profiles);
  let stream: MediaStream;
  try {
    stream = await deps.getUserMedia(cameraConstraints(deviceId, profile));
  } catch (e) {
    // A remembered camera that is gone: fall back to the system default instead of failing.
    const name = nameOf(e);
    if (deviceId !== 'default' && (name === 'OverconstrainedError' || name === 'NotFoundError')) {
      try {
        stream = await deps.getUserMedia(cameraConstraints('default', profile));
      } catch (e2) {
        throw toCaptureError(e2, 'camera');
      }
    } else throw toCaptureError(e, 'camera');
  }
  if (!stream.getVideoTracks()[0]) {
    stopStream(stream);
    throw toCaptureError(new DOMException('no video track', 'NotFoundError'), 'camera');
  }
  applyContentHint(stream, 'motion');
  return { stream, deviceId };
}

/** Open the screen / window / tab picker of the browser. */
export async function openScreen(deps: VideoDeps, o: ShareOptions): Promise<LocalScreen> {
  let stream: MediaStream;
  try {
    stream = await deps.getDisplayMedia(displayConstraints(o.preset, o.audio, o.source));
  } catch (e) {
    throw toCaptureError(e, 'screen');
  }
  if (!stream.getVideoTracks()[0]) {
    stopStream(stream);
    throw toCaptureError(new DOMException('no video track', 'NotFoundError'), 'screen');
  }
  applyContentHint(stream, o.hint);
  return { ...o, stream, hasAudio: stream.getAudioTracks().length > 0 };
}

export const stopCapture = stopStream;

function nameOf(e: unknown): string {
  const n = (e as { name?: unknown } | null)?.name;
  return typeof n === 'string' ? n : '';
}

/** Size and frame rate a participant's source was published with (op 12 `published`). */
export interface SourceInfo {
  width: number;
  height: number;
  fps: number;
}

/** Where a tile draws a remote video, in CSS pixels; the SFU sends the smallest layer that covers it. */
export interface TileViewport {
  width: number;
  height: number;
}

/**
 * Other people's video, by user id. Filled by the engine when the SFU forwards video (VOX-SFU-005/006).
 * `watching` is the set of streams the viewer chose to watch ("Watch Stream"). `info` and `layers` describe what
 * is arriving (key `<user>:<source>`); `viewports` are the tile sizes the controller turns into subscription
 * hints; `videoOff` are people whose camera the viewer turned off for themselves ("Disable Video").
 */
interface RemoteVideoState {
  camera: Record<string, MediaStream>;
  screen: Record<string, MediaStream>;
  watching: Record<string, boolean>;
  info: Record<string, SourceInfo>;
  layers: Record<string, string>;
  viewports: Record<string, TileViewport>;
  videoOff: Record<string, boolean>;
}
export const useRemoteVideo = create<RemoteVideoState>()(() => ({
  camera: {},
  screen: {},
  watching: {},
  info: {},
  layers: {},
  viewports: {},
  videoOff: {},
}));

export const sourceKey = (userId: string, source: string) => `${userId}:${source}`;

export function setRemoteVideo(
  userId: string,
  kind: 'camera' | 'screen',
  stream: MediaStream | null,
): void {
  useRemoteVideo.setState((s) => {
    const next = { ...s[kind] };
    if (stream) next[userId] = stream;
    else delete next[userId];
    return { [kind]: next } as Pick<RemoteVideoState, typeof kind>;
  });
}

export function setWatching(userId: string, watching: boolean): void {
  useRemoteVideo.setState((s) => ({ watching: { ...s.watching, [userId]: watching } }));
}

/** Record (or forget, with `null`) what a participant's source is published as. */
export function setRemoteInfo(userId: string, source: string, info: SourceInfo | null): void {
  useRemoteVideo.setState((s) => {
    const next = { ...s.info };
    if (info) next[sourceKey(userId, source)] = info;
    else delete next[sourceKey(userId, source)];
    return { info: next };
  });
}

export function setRemoteLayer(userId: string, source: string, layer: string): void {
  useRemoteVideo.setState((s) => ({ layers: { ...s.layers, [sourceKey(userId, source)]: layer } }));
}

/** A tile reports its size (or `null` when it goes away); only rounded changes are stored. */
export function setTileViewport(
  userId: string,
  source: 'camera' | 'screen',
  vp: TileViewport | null,
): void {
  const key = sourceKey(userId, source);
  const prev = useRemoteVideo.getState().viewports[key];
  if (!vp && !prev) return;
  if (vp && prev && prev.width === vp.width && prev.height === vp.height) return;
  useRemoteVideo.setState((s) => {
    const next = { ...s.viewports };
    if (vp) next[key] = vp;
    else delete next[key];
    return { viewports: next };
  });
}

/** "Disable Video" in the user menu: stop receiving this person's camera (it stays off for this call). */
export function setVideoOff(userId: string, off: boolean): void {
  useRemoteVideo.setState((s) => ({ videoOff: { ...s.videoOff, [userId]: off } }));
}

export const resetRemoteVideo = () =>
  useRemoteVideo.setState({
    camera: {},
    screen: {},
    watching: {},
    info: {},
    layers: {},
    viewports: {},
    videoOff: {},
  });

/**
 * The subscription hints for what the viewer sees now: every remote camera with the size of its tile (the SFU
 * picks the smallest simulcast layer that covers it; no tile yet means the thumbnail layer), cameras the viewer
 * turned off, and every screen the viewer watches or stopped watching. Screens are off until named here.
 */
export function buildHints(
  s: Pick<RemoteVideoState, 'camera' | 'watching' | 'viewports' | 'videoOff'>,
): VideoHint[] {
  const hints: VideoHint[] = [];
  const cameras = new Set([...Object.keys(s.camera), ...Object.keys(s.videoOff)]);
  for (const user of [...cameras].sort()) {
    const vp = s.viewports[sourceKey(user, 'camera')];
    hints.push({
      user_id: user,
      source: 'camera',
      enabled: !s.videoOff[user],
      ...(vp ? { viewport: vp } : {}),
    });
  }
  for (const user of Object.keys(s.watching).sort()) {
    const vp = s.viewports[sourceKey(user, 'screen')];
    hints.push({
      user_id: user,
      source: 'screen',
      enabled: !!s.watching[user],
      ...(vp ? { viewport: vp } : {}),
    });
  }
  return hints;
}
