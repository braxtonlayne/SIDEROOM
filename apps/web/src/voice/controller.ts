/**
 * Voice controller: joins and leaves voice channels and owns the policy around the engine.
 *
 * Join = gateway `VOICE_STATE_UPDATE` (op 4) -> wait for the VOICE_SERVER_UPDATE (endpoint, token, ICE
 * servers) -> engine.connect. A refusal arrives as SR_VOICE_ERROR. Then it turns mic level (voice
 * activity), the PTT key, mute / deafen and moderator mutes into one "transmitting" decision, plays the cue
 * sounds, and re-joins through the gateway when the voice session dies.
 *
 * Every platform dependency is injected (`ControllerDeps`), so tests drive it with a mock engine.
 */
import { Op } from '../gateway/protocol';
import { t } from '../i18n';
import { getGateway } from '../store/connect';
import { onDispatch } from '../store/events';
import { useStore } from '../store/store';
import type { VoiceServerInfo, VoiceState } from '../store/types';
import { browserAudioDeps, type AudioDeps } from './deps';
import {
  RtcVoiceEngine,
  VoiceConnectError,
  type EngineEvents,
  type EngineE2ee,
  type EngineFactory,
  type VoiceEngine,
  type VoiceError,
} from './engine';
import { PttController, ShortcutListener } from './ptt';
import { PING_HISTORY, patchSession, resetSession, useVoiceSession } from './session';
import {
  effectiveInputMode,
  getVoiceSettings,
  useVoiceSettings,
  userVolumeOf,
  type VoiceSettings,
} from './settings';
import { streamerState } from '../settings/user/prefs';
import { CuePlayer, cueEnabled, type Cue } from './sounds';
import { VadGate } from './vad';
import { e2eeFlagEnabled } from './e2ee/flag';
import type { E2eeRuntime, E2eeStartContext, E2eeStarter } from './e2ee/runtime';
import { VIDEO_PUBLISH } from './protocol';
import { PreviewUploader, type PreviewDeps, browserPreviewDeps } from './preview';
import { LocalSoundPlayer } from './soundPlayer';
import {
  playErrorKind,
  sendSoundboardSound,
  soundAudio,
  useSoundboard,
  type SendSoundBody,
  type Sound,
} from './soundboard';
import { ApiError } from '../api/client';
import { applyStreamEvent, ownerOfKey, resetStreams, streamKey } from './streams';
import {
  applyContentHint,
  bestProfile,
  browserVideoDeps,
  buildHints,
  CaptureError,
  openCamera,
  openScreen,
  resetLocalVideo,
  resetRemoteVideo,
  setRemoteInfo,
  setRemoteLayer,
  setRemoteVideo,
  setWatching,
  stopCapture,
  useLocalVideo,
  useRemoteVideo,
  type ShareOptions,
  type VideoDeps,
  type VideoProfile,
} from './video';

export interface JoinTarget {
  guildId: string;
  channelId: string;
  name: string;
  /** The channel denies USE_VAD, so Push to Talk is the only input mode. */
  pttForced?: boolean;
}

export interface ControllerDeps {
  createEngine: EngineFactory;
  /** Send a client gateway op; false when the gateway is not open. */
  sendGateway(op: number, d: unknown): boolean;
  subscribe(listener: (event: string, data: unknown) => void): () => void;
  currentUserId(): string | null;
  voiceStatesOf(guildId: string): Record<string, VoiceState>;
  audio: AudioDeps;
  now(): number;
  /** Camera / screen capture (VOX-CLI-006/007); the browser by default. */
  video?: VideoDeps;
  /** Announce `self_video` / `self_stream` in the voice state (default: `VIDEO_PUBLISH`). */
  publishVideo?: boolean;
  /** Stream preview thumbnails (VOX-CLI-008); the canvas and the API by default. */
  preview?: PreviewDeps;
  /** Announce a soundboard play (VOX-SB-003); `POST /channels/{id}/send-soundboard-sound` by default. */
  sendSound?: (channelId: string, body: SendSoundBody) => Promise<void>;
  /**
   * VOX-E2E-003: start end-to-end encryption for a call (`voice.e2ee`). Absent: calls are
   * DTLS-SRTP only. `unsupported`: the browser has no encoded-frame API, so the join fails.
   */
  e2ee?: E2eeStarter;
}

export type SoundResult = { ok: true } | { ok: false; message: string };

/** Wait this long after the last tile resize / watch change before sending subscription hints. */
export const HINT_DEBOUNCE_MS = 200;

// (`t` is shadowed by the join target inside the start path.)
const e2eeUnsupportedText = () => t('voice.e2ee.unsupported');
const e2eeFailedText = () => t('voice.e2ee.failed');

export const DM_GUILD_ID = '@me';
export const SERVER_WAIT_MS = 10_000;
export const MAX_REJOINS = 3;
export const MAX_MIC_RECOVERIES = 3;

const defaultDeps = (): ControllerDeps => ({
  createEngine: (events) => new RtcVoiceEngine(events),
  sendGateway: (op, d) => getGateway()?.send(op, d) ?? false,
  subscribe: onDispatch,
  currentUserId: () => useStore.getState().currentUserId,
  voiceStatesOf: (g) => useStore.getState().voiceStates[g] ?? {},
  audio: browserAudioDeps,
  now: () => Date.now(),
  ...(e2eeFlagEnabled()
    ? {
        e2ee: (ctx: E2eeStartContext) =>
          import('./e2ee/runtime').then((m) => m.startBrowserE2ee(ctx)),
      }
    : {}),
});

export class VoiceController {
  private engine: VoiceEngine | null = null;
  private target: JoinTarget | null = null;
  /** The last channel the viewer asked for, so "Try again" after a failure can re-join it. */
  private lastTarget: JoinTarget | null = null;
  /** We have seen our own voice state for the target channel (so a later "left" is a real disconnect). */
  private joinedSeen = false;
  private server: VoiceServerInfo | null = null;
  private started = false;
  private waitTimer: ReturnType<typeof setTimeout> | undefined;
  private rejoins = 0;
  private micRecoveries = 0;
  private mutedBeforeDeaf = false;
  private vad: VadGate;
  private vadOpen = false;
  private roster = new Set<string>();
  private cues: CuePlayer;
  private ptt: PttController;
  private keys: ShortcutListener;
  private offDispatch: (() => void) | null = null;
  private offSettings: (() => void) | null = null;
  private levelListeners = new Set<(db: number) => void>();
  private previews: PreviewUploader | null = null;
  private localSounds: LocalSoundPlayer | null = null;
  /** Streams the viewer asked to watch and the gateway has not confirmed yet. */
  private pendingWatch = new Set<string>();
  private offHints: (() => void) | null = null;
  private hintTimer: ReturnType<typeof setTimeout> | undefined;
  private lastHints = '';
  private e2ee: E2eeRuntime | null = null;
  /** Bumped per engine start, so a late E2EE start for an abandoned attempt is dropped. */
  private startSeq = 0;

  constructor(private deps: ControllerDeps = defaultDeps()) {
    const s = getVoiceSettings();
    this.vad = new VadGate({ auto: s.autoSensitivity, thresholdDb: s.thresholdDb });
    this.cues = new CuePlayer(() => this.deps.audio.createAudioContext());
    this.ptt = new PttController((active) => this.onPtt(active));
    this.keys = new ShortcutListener();
  }

  /** Swap dependencies (tests). Only valid while idle. */
  configure(deps: Partial<ControllerDeps>): void {
    this.deps = { ...this.deps, ...deps };
    this.cues = new CuePlayer(() => this.deps.audio.createAudioContext());
  }

  /** Attach the key listeners and settings subscription (once, from the shell). */
  start(): () => void {
    this.ptt.attach();
    this.keys.attach();
    this.offSettings ??= useVoiceSettings.subscribe((st, prev) => {
      if (st.settings !== prev.settings) this.applySettings(st.settings, prev.settings);
    });
    this.applyKeys();
    return () => this.stop();
  }

  stop(): void {
    this.ptt.detach();
    this.keys.detach();
    this.offSettings?.();
    this.offSettings = null;
    this.leave(false);
  }

  /** Mic level (dBFS) while in a call; the settings page uses its own meter outside calls. */
  onLevel(cb: (db: number) => void): () => void {
    this.levelListeners.add(cb);
    return () => this.levelListeners.delete(cb);
  }

  // ---- join / leave ----

  join(target: JoinTarget): void {
    if (
      this.target?.channelId === target.channelId &&
      useVoiceSession.getState().status !== 'error'
    )
      return;
    if (this.target) this.teardownEngine();
    this.target = { ...target, pttForced: !!target.pttForced };
    this.lastTarget = this.target;
    this.joinedSeen = false;
    this.server = null;
    this.started = false;
    this.rejoins = 0;
    this.micRecoveries = 0;
    const prior = useVoiceSession.getState();
    const keepDeaf = prior.selfDeaf;
    resetSession({
      status: 'joining',
      guildId: target.guildId,
      channelId: target.channelId,
      channelName: target.name,
      pttForced: !!target.pttForced,
      selfMute: prior.selfMute || keepDeaf,
      selfDeaf: keepDeaf,
    });
    this.offDispatch?.();
    this.offDispatch = this.deps.subscribe((e, d) => this.onDispatch(e, d));
    this.roster = new Set(
      Object.values(this.deps.voiceStatesOf(target.guildId))
        .filter((v) => v.channel_id === target.channelId)
        .map((v) => v.user_id),
    );
    this.sendState();
    this.armWait();
    this.applyKeys();
    this.applyPtt();
  }

  /** Join the last requested channel again (after a refused or failed join). */
  retry(): void {
    if (this.lastTarget && !this.target) this.join(this.lastTarget);
  }

  /** Forget a failed attempt's message. */
  dismissError(): void {
    if (!this.target) resetSession({ ...mutedFlags() });
  }

  leave(sendState = true): void {
    const had = this.target;
    this.teardownEngine();
    clearTimeout(this.waitTimer);
    this.offDispatch?.();
    this.offDispatch = null;
    if (had && sendState) {
      this.deps.sendGateway(Op.VOICE_STATE_UPDATE, {
        guild_id: had.guildId === DM_GUILD_ID ? null : had.guildId,
        channel_id: null,
        self_mute: useVoiceSession.getState().selfMute,
        self_deaf: useVoiceSession.getState().selfDeaf,
      });
    }
    const wasConnected = useVoiceSession.getState().status === 'connected';
    const { selfMute, selfDeaf } = useVoiceSession.getState();
    this.target = null;
    this.joinedSeen = false;
    this.server = null;
    this.roster.clear();
    this.vad.reset();
    this.vadOpen = false;
    this.stopCaptures();
    resetSession({ selfMute, selfDeaf });
    this.applyPtt();
    this.applyKeys();
    if (wasConnected && sendState) this.cue('selfLeave');
  }

  // ---- mute / deafen ----

  setSelfMute(mute: boolean): void {
    const s = useVoiceSession.getState();
    if (s.selfMute === mute) return;
    let deaf = s.selfDeaf;
    // Unmuting while deafened undeafens too (Discord).
    if (!mute && deaf) {
      deaf = false;
      this.engine?.setDeafened(s.serverDeaf);
      this.cue('undeafen');
    } else {
      this.cue(mute ? 'mute' : 'unmute');
    }
    patchSession({ selfMute: mute, selfDeaf: deaf });
    this.afterSelfChange();
  }

  toggleMute(): void {
    this.setSelfMute(!useVoiceSession.getState().selfMute);
  }

  setSelfDeaf(deaf: boolean): void {
    const s = useVoiceSession.getState();
    if (s.selfDeaf === deaf) return;
    if (deaf) {
      this.mutedBeforeDeaf = s.selfMute;
      patchSession({ selfDeaf: true, selfMute: true });
    } else {
      patchSession({ selfDeaf: false, selfMute: this.mutedBeforeDeaf });
    }
    this.cue(deaf ? 'deafen' : 'undeafen');
    this.engine?.setDeafened(deaf || s.serverDeaf);
    this.afterSelfChange();
  }

  toggleDeafen(): void {
    this.setSelfDeaf(!useVoiceSession.getState().selfDeaf);
  }

  private afterSelfChange(): void {
    this.recompute();
    if (this.target) this.sendState();
  }

  /** The channel's USE_VAD permission changed (or the viewer moved to a channel with a different one). */
  setPttForced(forced: boolean): void {
    if (useVoiceSession.getState().pttForced === forced) return;
    patchSession({ pttForced: forced });
    if (this.target) this.target = { ...this.target, pttForced: forced };
    this.applyPtt();
    this.recompute();
  }

  // ---- per-person ----

  setUserVolume(userId: string, percent: number): void {
    useVoiceSettings.getState().setUserVolume(userId, percent);
  }
  setUserMuted(userId: string, muted: boolean): void {
    useVoiceSettings.getState().setLocalMuted(userId, muted);
  }

  // ---- camera and screen share (VOX-CLI-006 / 007) ----

  private get videoDeps(): VideoDeps {
    return this.deps.video ?? browserVideoDeps;
  }

  /** Turn the camera on, capped to the best profile the viewer may send. Failures land in `useLocalVideo.error`. */
  async startCamera(
    profiles: readonly VideoProfile[],
    deviceId = getVoiceSettings().cameraDeviceId,
  ): Promise<void> {
    if (!this.target) return;
    this.stopCamera();
    this.cameraProfiles = profiles;
    useLocalVideo.setState({ error: null });
    try {
      const cam = await openCamera(this.videoDeps, deviceId, profiles);
      if (!this.target) {
        stopCapture(cam.stream);
        return;
      }
      cam.stream.getVideoTracks()[0]?.addEventListener('ended', () => {
        if (useLocalVideo.getState().camera?.stream === cam.stream) {
          this.stopCamera();
          useLocalVideo.setState({ error: t('voice.capture.cameraLost') });
        }
      });
      useLocalVideo.setState({ camera: cam });
      patchSession({ camera: true });
      this.publishCapture('camera');
      this.announceVideo();
    } catch (e) {
      useLocalVideo.setState({ error: e instanceof CaptureError ? e.message : String(e) });
    }
  }

  stopCamera(): void {
    const cam = useLocalVideo.getState().camera;
    if (!cam) return;
    this.engine?.unpublishVideo?.('camera');
    stopCapture(cam.stream);
    useLocalVideo.setState({ camera: null });
    patchSession({ camera: false });
    this.announceVideo();
  }

  /** Start a screen / window / tab share with the picker's choices. Cancelling the browser picker is silent. */
  async startScreenShare(opts: ShareOptions): Promise<void> {
    if (!this.target) return;
    this.stopScreenShare();
    useLocalVideo.setState({ error: null });
    try {
      const screen = await openScreen(this.videoDeps, opts);
      if (!this.target) {
        stopCapture(screen.stream);
        return;
      }
      // The browser's own "Stop sharing" button ends the track.
      screen.stream.getVideoTracks()[0]?.addEventListener('ended', () => {
        if (useLocalVideo.getState().screen?.stream === screen.stream) this.stopScreenShare();
      });
      useLocalVideo.setState({ screen });
      patchSession({ screen: true });
      this.publishCapture('screen');
      this.startStream(screen);
      this.announceVideo();
    } catch (e) {
      if (e instanceof CaptureError && e.code === 'cancelled') return;
      useLocalVideo.setState({ error: e instanceof CaptureError ? e.message : String(e) });
    }
  }

  /** Change the content hint of a running share (motion for video, detail for text). */
  setShareHint(hint: ShareOptions['hint']): void {
    const screen = useLocalVideo.getState().screen;
    if (!screen) return;
    applyContentHint(screen.stream, hint);
    useLocalVideo.setState({ screen: { ...screen, hint } });
  }

  stopScreenShare(): void {
    const screen = useLocalVideo.getState().screen;
    if (!screen) return;
    this.engine?.unpublishVideo?.('screen');
    this.endStream();
    stopCapture(screen.stream);
    useLocalVideo.setState({ screen: null });
    patchSession({ screen: false });
    this.announceVideo();
  }

  dismissVideoError(): void {
    useLocalVideo.setState({ error: null });
  }

  /** The size and rate a running capture is published with (what the track reports, else what was asked). */
  private captureInfo(
    kind: 'camera' | 'screen',
  ): { width: number; height: number; fps: number } | null {
    const local = useLocalVideo.getState();
    const cap = kind === 'camera' ? local.camera : local.screen;
    if (!cap) return null;
    const asked =
      kind === 'screen' && local.screen
        ? { height: local.screen.preset.height, fps: local.screen.preset.fps }
        : bestProfile(this.cameraProfiles);
    const got = cap.stream.getVideoTracks()[0]?.getSettings?.();
    const height = Math.round(got?.height ?? asked.height);
    return {
      width: Math.round(got?.width ?? (height * 16) / 9),
      height,
      fps: Math.round(got?.frameRate ?? asked.fps),
    };
  }

  /** Put a running capture on the engine's lines (when the engine is up and video is published at all). */
  private publishCapture(kind: 'camera' | 'screen'): void {
    if (!(this.deps.publishVideo ?? VIDEO_PUBLISH) || !this.engine?.publishVideo) return;
    const local = useLocalVideo.getState();
    const cap = kind === 'camera' ? local.camera : local.screen;
    const info = this.captureInfo(kind);
    if (cap && info) this.engine.publishVideo(kind, cap.stream, info);
  }

  /** After a (re)connect: send again whatever the viewer is capturing, and the subscription hints. */
  private republish(): void {
    this.publishCapture('camera');
    if (useLocalVideo.getState().screen) {
      this.publishCapture('screen');
      this.sendStreamCreate();
    }
    this.syncHints(true);
  }

  /** Profiles the camera was last opened with, for the size the SFU is told. */
  private cameraProfiles: readonly VideoProfile[] = [];

  private get streamKeyOfMe(): string | null {
    const t = this.target;
    const me = this.deps.currentUserId();
    return t && me ? streamKey(t.guildId, t.channelId, me) : null;
  }

  /** Op 18: tell the gateway a screen share started (it checks the allowance and opens the stream). */
  private sendStreamCreate(): void {
    const t = this.target;
    const screen = useLocalVideo.getState().screen;
    if (!t || !screen || !(this.deps.publishVideo ?? VIDEO_PUBLISH)) return;
    this.deps.sendGateway(Op.STREAM_CREATE, {
      guild_id: t.guildId === DM_GUILD_ID ? null : t.guildId,
      channel_id: t.channelId,
      height: screen.preset.height,
      fps: screen.preset.fps,
    });
  }

  private startStream(screen: { stream: MediaStream }): void {
    if (!(this.deps.publishVideo ?? VIDEO_PUBLISH)) return;
    this.sendStreamCreate();
    const key = this.streamKeyOfMe;
    if (!key) return;
    this.previews ??= new PreviewUploader(this.deps.preview ?? browserPreviewDeps);
    this.previews.start(key, screen.stream);
  }

  private endStream(): void {
    this.previews?.stop();
    const key = this.streamKeyOfMe;
    if (key && (this.deps.publishVideo ?? VIDEO_PUBLISH)) {
      this.deps.sendGateway(Op.STREAM_DELETE, { stream_key: key });
    }
  }

  // ---- soundboard (VOX-SB-002 / VOX-SB-003) ----

  /** Hear a sound yourself, without sending it (the popover's preview); works outside a call too. */
  async previewSound(sound: Sound): Promise<boolean> {
    try {
      const data = await soundAudio(sound);
      this.localSounds ??= new LocalSoundPlayer(this.deps.audio);
      return await this.localSounds.play(
        data,
        sound.volume * (useSoundboard.getState().volume / 100),
        getVoiceSettings().outputDeviceId,
      );
    } catch {
      return false;
    }
  }

  /**
   * Play a sound into the call: the gateway checks the permission, that the viewer can be heard and the
   * cooldown (and tells the room), then the file is mixed into the outgoing audio so everyone hears it at their
   * volume for the viewer, and the viewer hears it too.
   */
  async playSoundboard(sound: Sound): Promise<SoundResult> {
    const tgt = this.target;
    const engine = this.engine;
    if (
      !tgt ||
      tgt.guildId === DM_GUILD_ID ||
      !engine ||
      useVoiceSession.getState().status !== 'connected'
    ) {
      return { ok: false, message: t('voice.soundboard.notInCall') };
    }
    const body: SendSoundBody = {
      sound_id: sound.id,
      ...(sound.guildId && sound.guildId !== tgt.guildId ? { source_guild_id: sound.guildId } : {}),
    };
    try {
      const send = this.deps.sendSound ?? sendSoundboardSound;
      const [data] = await Promise.all([soundAudio(sound), send(tgt.channelId, body)]);
      if (this.engine !== engine) return { ok: false, message: t('voice.soundboard.notInCall') };
      const level = sound.volume * (useSoundboard.getState().volume / 100);
      const mixed = (await engine.playSound?.(data.slice(0), level)) ?? false;
      void this.previewSelf(data, level);
      return mixed ? { ok: true } : { ok: false, message: t('voice.soundboard.notMixed') };
    } catch (e) {
      const status = e instanceof ApiError ? e.status : 0;
      switch (playErrorKind(status)) {
        case 'permission':
          return { ok: false, message: t('voice.soundboard.errPermission') };
        case 'silenced':
          return { ok: false, message: t('voice.soundboard.errSilenced') };
        case 'cooldown':
          return { ok: false, message: t('voice.soundboard.errCooldown') };
        case 'gone':
          return { ok: false, message: t('voice.soundboard.errGone') };
        default:
          return { ok: false, message: t('voice.soundboard.errOther') };
      }
    }
  }

  /** The sender hears their own sound (the call's audio does not loop back to them). */
  private async previewSelf(data: ArrayBuffer, level: number): Promise<void> {
    this.localSounds ??= new LocalSoundPlayer(this.deps.audio);
    await this.localSounds.play(data.slice(0), level, getVoiceSettings().outputDeviceId);
  }

  // ---- watching other people's streams (VOX-UI-004) ----

  /** Start watching someone's screen share ("Watch Stream"): op 20, then the SFU is told to forward it. */
  watchStream(ownerId: string): void {
    const t = this.target;
    this.pendingWatch.add(ownerId);
    setWatching(ownerId, true);
    if (t) {
      this.deps.sendGateway(Op.STREAM_WATCH, {
        stream_key: streamKey(t.guildId, t.channelId, ownerId),
      });
    }
  }

  /** Stop watching: op 19 (a viewer's delete is an unwatch) and the SFU stops forwarding. */
  unwatchStream(ownerId: string): void {
    const t = this.target;
    this.pendingWatch.delete(ownerId);
    setWatching(ownerId, false);
    if (t) {
      this.deps.sendGateway(Op.STREAM_DELETE, {
        stream_key: streamKey(t.guildId, t.channelId, ownerId),
      });
    }
  }

  /** Tell the SFU which cameras and screens to forward and how big (debounced; only when it changed). */
  private syncHints(force = false): void {
    clearTimeout(this.hintTimer);
    const run = () => {
      const engine = this.engine;
      if (!engine?.setVideoSubscriptions) return;
      const hints = buildHints(useRemoteVideo.getState());
      const json = JSON.stringify(hints);
      if (!force && json === this.lastHints) return;
      this.lastHints = json;
      engine.setVideoSubscriptions(hints);
    };
    if (force) run();
    else this.hintTimer = setTimeout(run, HINT_DEBOUNCE_MS);
  }

  /** Tell the room about the camera / share, once the SFU forwards video (VIDEO_PUBLISH). */
  private announceVideo(): void {
    if (this.deps.publishVideo ?? VIDEO_PUBLISH) this.sendState();
  }

  private stopCaptures(): void {
    const { camera, screen } = useLocalVideo.getState();
    stopCapture(camera?.stream);
    stopCapture(screen?.stream);
    this.previews?.stop();
    this.localSounds?.dispose();
    this.localSounds = null;
    resetLocalVideo();
  }

  // ---- gateway ----

  private sendState(): boolean {
    const t = this.target;
    if (!t) return false;
    const s = useVoiceSession.getState();
    const ok = this.deps.sendGateway(Op.VOICE_STATE_UPDATE, {
      guild_id: t.guildId === DM_GUILD_ID ? null : t.guildId,
      channel_id: t.channelId,
      self_mute: s.selfMute,
      self_deaf: s.selfDeaf,
      ...((this.deps.publishVideo ?? VIDEO_PUBLISH)
        ? { self_video: s.camera, self_stream: s.screen }
        : {}),
    });
    if (!ok)
      this.fail({
        code: 'gateway',
        message: 'Not connected to SIDE ROOM. Reconnecting, try again in a moment.',
      });
    return ok;
  }

  private armWait(): void {
    clearTimeout(this.waitTimer);
    this.waitTimer = setTimeout(() => {
      if (!this.target || this.started) return;
      this.fail({
        code: 'timeout',
        message: 'The voice server did not answer. Try joining again.',
      });
    }, SERVER_WAIT_MS);
  }

  private onDispatch(event: string, data: unknown): void {
    const me = this.deps.currentUserId();
    const d = (data ?? {}) as Record<string, unknown>;
    const t = this.target;
    if (event === 'READY') {
      // A brand-new gateway session knows nothing of our voice state: say it again.
      if (t) {
        this.sendState();
        this.armWait();
      }
      return;
    }
    if (!t) return;
    if (event === 'E2EE_MLS') {
      this.e2ee?.call.onMessage(d);
      return;
    }
    if (event === 'STREAM_CREATE' || event === 'STREAM_UPDATE' || event === 'STREAM_DELETE') {
      const r = applyStreamEvent(event, data, me);
      const started = r.started ? ownerOfKey(r.started) : null;
      if (started) {
        this.pendingWatch.delete(started);
        setWatching(started, true);
      }
      const ended = r.ended ? ownerOfKey(r.ended) : null;
      if (ended) {
        this.pendingWatch.delete(ended);
        setWatching(ended, false);
      }
      return;
    }
    if (event === 'VOICE_STATE_UPDATE') {
      const userId = String(d.user_id ?? '');
      const channelId = (d.channel_id as string | null | undefined) ?? null;
      if (userId === me) this.onOwnState(d as unknown as VoiceState);
      else this.onOtherState(userId, channelId);
    } else if (event === 'SR_VOICE_ERROR') {
      if (d.channel_id != null && d.channel_id !== t.channelId) return;
      // A refused camera or share (sender allowance, viewer cap) leaves the call running (VOX-UI-013).
      if (this.started && this.onVideoRefusal(String(d.code ?? ''), d.limit)) return;
      // A share the plan does not allow (quality above the viewer's profiles) is dropped with a note.
      if (d.code === 'stream_quality_not_allowed') {
        this.dropShareForQuality();
        return;
      }
      // Stream ops (18 to 22) are answered with `channel_id: null`; a refused stream op never ends the call.
      if (d.channel_id === null) return;
      this.fail({
        code: 'refused',
        message: refusalText(String(d.code ?? ''), String(d.message ?? '')),
      });
    } else if (event === 'VOICE_SERVER_UPDATE') {
      const sameGuild = (typeof d.guild_id === 'string' ? d.guild_id : DM_GUILD_ID) === t.guildId;
      if (!sameGuild) return;
      // After a move a stale update for the old channel can still arrive: only the target channel counts.
      if (typeof d.channel_id === 'string' && d.channel_id !== t.channelId) return;
      this.server = {
        guild_id: typeof d.guild_id === 'string' ? d.guild_id : null,
        endpoint: String(d.endpoint ?? ''),
        token: String(d.token ?? ''),
        ...(Array.isArray(d.ice_servers) ? { ice_servers: d.ice_servers } : {}),
      } as VoiceServerInfo;
      // A new server for a live session (node move / re-key): reconnect to it.
      if (this.started) {
        this.teardownEngine();
        this.started = false;
      }
      this.maybeStart();
    }
  }

  private onOwnState(v: VoiceState): void {
    const t = this.target;
    if (!t) return;
    // A refusal to leave the old guild can echo a state for another guild: only ours counts.
    if ((v.guild_id ?? DM_GUILD_ID) !== t.guildId) return;
    if (v.channel_id === null) {
      // Disconnected by a moderator or the server (not by us: leave() clears the target first).
      if (this.joinedSeen) {
        const keep = {
          selfMute: useVoiceSession.getState().selfMute,
          selfDeaf: useVoiceSession.getState().selfDeaf,
        };
        this.teardownEngine();
        clearTimeout(this.waitTimer);
        this.offDispatch?.();
        this.offDispatch = null;
        this.target = null;
        this.joinedSeen = false;
        this.server = null;
        resetSession({
          ...keep,
          status: 'error',
          error: { code: 'disconnected', message: 'You were disconnected from voice.' },
        });
        this.cue('selfLeave');
        this.applyPtt();
        this.applyKeys();
      }
      return;
    }
    if (v.channel_id !== t.channelId) {
      // Moved by a moderator: follow to the new channel (a VOICE_SERVER_UPDATE follows if needed).
      const name = lookupChannelName(v.channel_id) ?? t.name;
      this.teardownEngine();
      this.target = { ...t, channelId: v.channel_id, name };
      this.started = false;
      // The voice token is per room: wait for the new channel's VOICE_SERVER_UPDATE.
      this.server = null;
      this.armWait();
      patchSession({ channelId: v.channel_id, channelName: name, status: 'connecting' });
      this.roster = new Set(
        Object.values(this.deps.voiceStatesOf(t.guildId))
          .filter((x) => x.channel_id === v.channel_id)
          .map((x) => x.user_id),
      );
    }
    this.joinedSeen = true;
    const wasSuppressed = useVoiceSession.getState().suppressed;
    patchSession({ serverMute: !!v.mute, serverDeaf: !!v.deaf, suppressed: !!v.suppress });
    // Invited to speak from the stage audience: the viewer unmutes themselves (Discord).
    if (wasSuppressed && !v.suppress) this.noteInvited();
    this.engine?.setDeafened(!!v.deaf || useVoiceSession.getState().selfDeaf);
    this.recompute();
    this.maybeStart();
  }

  private onOtherState(userId: string, channelId: string | null): void {
    const t = this.target;
    if (!t || !userId || useVoiceSession.getState().status !== 'connected') return;
    const here = channelId === t.channelId;
    // The store applies the same event; read the roster once it has.
    if (this.e2ee) queueMicrotask(() => this.e2ee?.call.onRosterChanged());
    if (here && !this.roster.has(userId)) {
      this.roster.add(userId);
      this.cue('join');
    } else if (!here && this.roster.has(userId)) {
      this.roster.delete(userId);
      this.cue('leave');
      patchSession({ speaking: omit(useVoiceSession.getState().speaking, userId) });
    }
  }

  // ---- engine ----

  private maybeStart(): void {
    const t = this.target;
    const me = this.deps.currentUserId();
    if (!t || !me || this.started || !this.server) return;
    if (!this.server.endpoint || !this.server.token) return;
    this.started = true;
    clearTimeout(this.waitTimer);
    patchSession({ status: 'connecting', error: null });
    const starter = this.deps.e2ee;
    if (!starter) {
      this.startEngine(t, me);
      return;
    }
    const seq = ++this.startSeq;
    const guildId = t.guildId;
    starter({
      me,
      channelId: t.channelId,
      send: (d) => this.deps.sendGateway(Op.E2EE_MLS, d),
      roster: () => {
        const cur = this.target;
        if (!cur) return [me];
        const ids = Object.values(this.deps.voiceStatesOf(guildId))
          .filter((v) => v.channel_id === cur.channelId)
          .map((v) => v.user_id);
        return ids.includes(me) ? ids : [...ids, me];
      },
    })
      .then((rt) => {
        if (seq !== this.startSeq || !this.started || this.target?.channelId !== t.channelId) {
          if (rt !== 'unsupported') rt.stop();
          return;
        }
        if (rt === 'unsupported') {
          this.fail({ code: 'refused', message: e2eeUnsupportedText() });
          return;
        }
        this.e2ee = rt;
        this.startEngine(t, me, rt.engine);
      })
      .catch(() => {
        if (seq !== this.startSeq) return;
        this.fail({ code: 'connect', message: e2eeFailedText() });
      });
  }

  private startEngine(t: JoinTarget, me: string, e2ee?: EngineE2ee): void {
    const server = this.server;
    if (!server) return;
    const engine = this.deps.createEngine(this.engineEvents());
    this.engine = engine;
    this.lastHints = '';
    this.offHints?.();
    this.offHints = useRemoteVideo.subscribe((st, prev) => {
      if (
        st.camera !== prev.camera ||
        st.watching !== prev.watching ||
        st.viewports !== prev.viewports ||
        st.videoOff !== prev.videoOff
      )
        this.syncHints();
    });
    const s = getVoiceSettings();
    engine.setMasterVolume(s.outputVolume);
    const session = useVoiceSession.getState();
    engine.setDeafened(session.selfDeaf || session.serverDeaf);
    for (const [id, pct] of Object.entries(s.userVolumes)) engine.setUserVolume(id, pct);
    for (const id of Object.keys(s.localMuted)) engine.setUserMuted(id, true);
    this.vad.configure({ auto: s.autoSensitivity, thresholdDb: s.thresholdDb });
    this.recompute();
    void engine
      .connect({
        endpoint: server.endpoint,
        token: server.token,
        userId: me,
        ...(server.ice_servers ? { iceServers: server.ice_servers } : {}),
        ...bitrateOf(t.channelId),
        mic: micOptions(s),
        ...(s.qosHighPriority ? { highPriority: true } : {}),
        ...(e2ee ? { e2ee } : {}),
      })
      .then(() => {
        if (this.engine !== engine) return;
        this.rejoins = 0;
        this.republish();
        patchSession({ status: 'connected', error: null });
        this.cue('selfJoin');
        this.recompute();
        if (s.outputDeviceId !== 'default')
          void engine.setOutputDevice(s.outputDeviceId).catch(() => undefined);
      })
      .catch((e: unknown) => {
        if (this.engine !== engine) return;
        if (e instanceof VoiceConnectError && e.voice.code === 'mic') {
          this.fail({
            code: 'mic',
            message: e.voice.message,
            ...(e.voice.mic ? { mic: e.voice.mic } : {}),
          });
        } else {
          this.rejoin();
        }
      });
  }

  private engineEvents(): EngineEvents {
    return {
      status: (st) => {
        if (!this.target || !this.started) return;
        if (st === 'reconnecting') patchSession({ status: 'reconnecting' });
        else if (st === 'connected') patchSession({ status: 'connected' });
      },
      speaking: (userId, speaking) => {
        if (userId === this.deps.currentUserId()) return; // our own ring follows `transmitting`
        patchSession({ speaking: { ...useVoiceSession.getState().speaking, [userId]: speaking } });
      },
      quality: (quality) =>
        patchSession({
          quality,
          pingHistory:
            quality.pingMs === null
              ? useVoiceSession.getState().pingHistory
              : [...useVoiceSession.getState().pingHistory, quality.pingMs].slice(-PING_HISTORY),
        }),
      fatal: (e) => this.onEngineFatal(e),
      level: (db) => this.onLevelReading(db),
      micLost: () => this.recoverMic(),
      governor: (reduced) => patchSession({ reducedQuality: reduced }),
      peer: (userId, present) => {
        if (!present) patchSession({ speaking: omit(useVoiceSession.getState().speaking, userId) });
      },
      remoteVideo: (userId, kind, stream) => setRemoteVideo(userId, kind, stream),
      videoPublished: (userId, source, active, size) =>
        setRemoteInfo(userId, source, active ? size : null),
      videoLayer: (userId, source, layer) => setRemoteLayer(userId, source, layer),
    };
  }

  /**
   * The capture device went away mid-call (VOX-CLI-001). Instead of ending the call, reopen the system default
   * input and carry on; only when that fails too (or keeps failing) does the call end with the fix-it message.
   */
  private recoverMic(): void {
    const engine = this.engine;
    const giveUp = () =>
      this.fail({
        code: 'mic',
        mic: 'no-device',
        message: t('voice.controller.micDisconnected'),
      });
    if (!engine || this.micRecoveries >= MAX_MIC_RECOVERIES) return giveUp();
    this.micRecoveries++;
    void engine
      .setInputDevice({ ...micOptions(getVoiceSettings()), deviceId: 'default' })
      .then(() => {
        if (this.engine !== engine) return;
        patchSession({ notice: t('voice.controller.micRecovered') });
      })
      .catch(() => {
        if (this.engine === engine) giveUp();
      });
  }

  /**
   * SR_VOICE_ERROR codes for a refused camera or share (VOX-SIG-009, sr-gateway `JoinDenied`):
   * `video_limit_reached` (sender allowance of the channel) and `stream_full` (audience of a stream), each
   * with a numeric `limit` and `count`. The earlier proposed names `video_limit` / `viewer_cap` are still
   * accepted. Returns true when the code was one of them.
   */
  private onVideoRefusal(raw: string, limit: unknown): boolean {
    const code =
      raw === 'video_limit_reached' || raw === 'video_limit'
        ? 'video_limit'
        : raw === 'stream_full' || raw === 'viewer_cap'
          ? 'viewer_cap'
          : null;
    if (!code) return false;
    const n = typeof limit === 'number' ? limit : null;
    if (code === 'video_limit') {
      this.stopCamera();
      this.stopScreenShare();
    } else {
      // The stream is full: nobody we asked to watch is being forwarded.
      for (const owner of this.pendingWatch) setWatching(owner, false);
      this.pendingWatch.clear();
    }
    patchSession({
      refusal:
        code === 'video_limit'
          ? {
              code: 'videoLimit',
              message:
                n === null
                  ? t('voice.refusal.videoLimitNoCount')
                  : t('voice.refusal.videoLimit', { limit: n }),
            }
          : {
              code: 'viewerCap',
              message:
                n === null
                  ? t('voice.refusal.viewerCapNoCount')
                  : t('voice.refusal.viewerCap', { limit: n }),
            },
    });
    return true;
  }

  private dropShareForQuality(): void {
    this.stopScreenShare();
    patchSession({ notice: t('voice.refusal.streamQuality') });
  }

  private noteInvited(): void {
    patchSession({ notice: t('voice.stage.invited') });
  }

  dismissRefusal(): void {
    patchSession({ refusal: null });
  }

  /** Dismiss the "recovered" note. */
  dismissNotice(): void {
    patchSession({ notice: null });
  }

  private onLevelReading(db: number): void {
    for (const cb of [...this.levelListeners]) cb(db);
    const open = this.vad.process(db, this.deps.now());
    if (open !== this.vadOpen) {
      this.vadOpen = open;
      this.recompute();
    }
  }

  private onEngineFatal(e: VoiceError): void {
    if (!this.target) return;
    if (e.code === 'removed') {
      // A moderator disconnected or moved us. The gateway's VOICE_STATE_UPDATE (and, for a move, the next
      // VOICE_SERVER_UPDATE) tells us which; do not re-join on our own.
      this.teardownEngine();
      this.started = false;
      this.server = null;
      patchSession({ status: 'connecting' });
      this.armWait();
    } else if (e.code === 'replaced' || e.code === 'full') {
      this.fail({ code: e.code === 'full' ? 'refused' : 'replaced', message: e.message });
    } else {
      this.rejoin();
    }
  }

  /** The gateway still says we are in the channel but voice died: ask for a fresh server, a few times. */
  private rejoin(): void {
    if (!this.target) return;
    this.teardownEngine();
    this.started = false;
    this.server = null;
    if (this.rejoins++ >= MAX_REJOINS) {
      this.fail({
        code: 'connect',
        message: 'Could not connect to voice. Check your connection and try again.',
      });
      return;
    }
    patchSession({ status: 'reconnecting' });
    // Spec 4.5: a repeated op 4 for the channel we are in issues no new VOICE_SERVER_UPDATE, so leave first.
    const t = this.target;
    const s = useVoiceSession.getState();
    this.deps.sendGateway(Op.VOICE_STATE_UPDATE, {
      guild_id: t.guildId === DM_GUILD_ID ? null : t.guildId,
      channel_id: null,
      self_mute: s.selfMute,
      self_deaf: s.selfDeaf,
    });
    this.sendState();
    this.armWait();
  }

  private fail(error: NonNullable<ReturnType<typeof useVoiceSession.getState>['error']>): void {
    const t = this.target;
    const { selfMute, selfDeaf } = useVoiceSession.getState();
    this.teardownEngine();
    clearTimeout(this.waitTimer);
    this.offDispatch?.();
    this.offDispatch = null;
    if (t) {
      this.deps.sendGateway(Op.VOICE_STATE_UPDATE, {
        guild_id: t.guildId === DM_GUILD_ID ? null : t.guildId,
        channel_id: null,
        self_mute: selfMute,
        self_deaf: selfDeaf,
      });
    }
    this.target = null;
    this.joinedSeen = false;
    this.server = null;
    this.started = false;
    this.stopCaptures();
    resetSession({ selfMute, selfDeaf, status: 'error', error });
    this.cue('error');
    this.applyPtt();
    this.applyKeys();
  }

  private teardownEngine(): void {
    const e = this.engine;
    this.engine = null;
    e?.disconnect();
    this.startSeq++;
    this.e2ee?.stop();
    this.e2ee = null;
    this.offHints?.();
    this.offHints = null;
    clearTimeout(this.hintTimer);
    this.lastHints = '';
    this.pendingWatch.clear();
    resetRemoteVideo();
    resetStreams();
    this.vad.reset();
    this.vadOpen = false;
    patchSession({
      transmitting: false,
      pttActive: false,
      speaking: {},
      quality: useVoiceSession.getState().quality,
    });
  }

  // ---- transmit decision ----

  private onPtt(active: boolean): void {
    patchSession({ pttActive: active });
    const session = useVoiceSession.getState();
    if (session.status === 'connected' && !session.selfMute) this.cue(active ? 'pttOn' : 'pttOff');
    this.recompute();
  }

  private recompute(): void {
    const s = getVoiceSettings();
    const session = useVoiceSession.getState();
    const mode = effectiveInputMode(s, session.pttForced);
    const allowed =
      !session.selfMute &&
      !session.selfDeaf &&
      !session.serverMute &&
      !session.serverDeaf &&
      !session.suppressed;
    const open = allowed && (mode === 'ptt' ? session.pttActive : this.vadOpen);
    this.engine?.setTransmitting(open);
    this.engine?.setAttenuation?.(open ? s.attenuation : 0);
    if (open !== session.transmitting) patchSession({ transmitting: open });
  }

  private applyPtt(): void {
    const s = getVoiceSettings();
    const session = useVoiceSession.getState();
    const inCall = !!this.target;
    const mode = effectiveInputMode(s, session.pttForced);
    this.ptt.configure({
      bind: s.keybinds.ptt,
      releaseMs: s.pttReleaseMs,
      enabled: inCall && mode === 'ptt',
    });
  }

  private applyKeys(): void {
    const s = getVoiceSettings();
    this.keys.configure(
      {
        toggleMute: s.keybinds.toggleMute,
        toggleDeafen: s.keybinds.toggleDeafen,
        disconnect: s.keybinds.disconnect,
      },
      {
        toggleMute: () => this.toggleMute(),
        toggleDeafen: () => this.toggleDeafen(),
        disconnect: () => this.leave(),
      },
      !!this.target,
    );
  }

  private applySettings(s: VoiceSettings, prev: VoiceSettings): void {
    this.vad.configure({ auto: s.autoSensitivity, thresholdDb: s.thresholdDb });
    this.applyPtt();
    this.applyKeys();
    const e = this.engine;
    if (e) {
      if (s.outputVolume !== prev.outputVolume) e.setMasterVolume(s.outputVolume);
      if (s.attenuation !== prev.attenuation) this.recompute();
      if (s.inputVolume !== prev.inputVolume) e.setInputVolume(s.inputVolume);
      if (s.outputDeviceId !== prev.outputDeviceId)
        void e.setOutputDevice(s.outputDeviceId).catch(() => undefined);
      if (
        s.inputDeviceId !== prev.inputDeviceId ||
        s.echoCancellation !== prev.echoCancellation ||
        s.autoGainControl !== prev.autoGainControl
      ) {
        void e.setInputDevice(micOptions(s)).catch(() => undefined);
      }
      for (const id of new Set([...Object.keys(s.userVolumes), ...Object.keys(prev.userVolumes)])) {
        if (userVolumeOf(s, id) !== userVolumeOf(prev, id))
          e.setUserVolume(id, userVolumeOf(s, id));
      }
      for (const id of new Set([...Object.keys(s.localMuted), ...Object.keys(prev.localMuted)])) {
        if (!!s.localMuted[id] !== !!prev.localMuted[id]) e.setUserMuted(id, !!s.localMuted[id]);
      }
    }
    this.recompute();
  }

  /** One repetition of the incoming-call ringtone (VOX-UI-010), at the cue volume. */
  playRing(): void {
    this.cue('ring');
  }

  private cue(cue: Cue): void {
    const s = getVoiceSettings();
    if (!cueEnabled(cue, s) || streamerState().silence) return;
    this.cues.play(cue, s.soundVolume / 100);
  }
}

function refusalText(code: string, message: string): string {
  switch (code) {
    case 'missing_permission':
      return "You don't have permission to join this voice channel.";
    case 'channel_full':
      return 'This voice channel is full.';
    case 'voice_unavailable':
      return 'Voice is not available on this server.';
    case 'not_voice_channel':
    case 'unknown_channel':
      return 'That voice channel is no longer available.';
    case 'stage_unsupported':
      return 'Stage channels are not supported yet.';
    default:
      return message || 'Could not join voice.';
  }
}

const mutedFlags = () => {
  const { selfMute, selfDeaf } = useVoiceSession.getState();
  return { selfMute, selfDeaf };
};

const micOptions = (s: VoiceSettings) => ({
  deviceId: s.inputDeviceId,
  echoCancellation: s.echoCancellation,
  autoGainControl: s.autoGainControl,
  inputVolume: s.inputVolume,
});

function omit<T>(rec: Record<string, T>, key: string): Record<string, T> {
  if (!(key in rec)) return rec;
  const copy = { ...rec };
  delete copy[key];
  return copy;
}

/** The channel's bitrate (bps) from the store, when the API provided one. */
function bitrateOf(channelId: string): { maxBitrate: number } | Record<string, never> {
  const bitrate = useStore.getState().channels[channelId]?.bitrate;
  return bitrate && bitrate > 0 ? { maxBitrate: bitrate } : {};
}

function lookupChannelName(channelId: string): string | undefined {
  return useStore.getState().channels[channelId]?.name ?? undefined;
}

let instance: VoiceController | null = null;
export function voiceController(): VoiceController {
  instance ??= new VoiceController();
  return instance;
}
/** Tests: replace (or clear) the shared controller. */
export function setVoiceController(c: VoiceController | null): void {
  instance?.stop();
  instance = c;
}
