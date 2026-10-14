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
import { CuePlayer, cueEnabled, type Cue } from './sounds';
import { VadGate } from './vad';
import { VIDEO_PUBLISH } from './protocol';
import {
  applyContentHint,
  browserVideoDeps,
  CaptureError,
  openCamera,
  openScreen,
  resetLocalVideo,
  stopCapture,
  useLocalVideo,
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
}

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
  async startCamera(profiles: readonly VideoProfile[], deviceId = 'default'): Promise<void> {
    if (!this.target) return;
    this.stopCamera();
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
      this.announceVideo();
    } catch (e) {
      useLocalVideo.setState({ error: e instanceof CaptureError ? e.message : String(e) });
    }
  }

  stopCamera(): void {
    const cam = useLocalVideo.getState().camera;
    if (!cam) return;
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
    stopCapture(screen.stream);
    useLocalVideo.setState({ screen: null });
    patchSession({ screen: false });
    this.announceVideo();
  }

  dismissVideoError(): void {
    useLocalVideo.setState({ error: null });
  }

  /** Tell the room about the camera / share, once the SFU forwards video (VIDEO_PUBLISH). */
  private announceVideo(): void {
    if (this.deps.publishVideo ?? VIDEO_PUBLISH) this.sendState();
  }

  private stopCaptures(): void {
    const { camera, screen } = useLocalVideo.getState();
    stopCapture(camera?.stream);
    stopCapture(screen?.stream);
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
    if (event === 'VOICE_STATE_UPDATE') {
      const userId = String(d.user_id ?? '');
      const channelId = (d.channel_id as string | null | undefined) ?? null;
      if (userId === me) this.onOwnState(d as unknown as VoiceState);
      else this.onOtherState(userId, channelId);
    } else if (event === 'SR_VOICE_ERROR') {
      if (d.channel_id !== undefined && d.channel_id !== t.channelId) return;
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
    patchSession({ serverMute: !!v.mute, serverDeaf: !!v.deaf });
    this.engine?.setDeafened(!!v.deaf || useVoiceSession.getState().selfDeaf);
    this.recompute();
    this.maybeStart();
  }

  private onOtherState(userId: string, channelId: string | null): void {
    const t = this.target;
    if (!t || !userId || useVoiceSession.getState().status !== 'connected') return;
    const here = channelId === t.channelId;
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
    const engine = this.deps.createEngine(this.engineEvents());
    this.engine = engine;
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
        endpoint: this.server.endpoint,
        token: this.server.token,
        userId: me,
        ...(this.server.ice_servers ? { iceServers: this.server.ice_servers } : {}),
        ...bitrateOf(t.channelId),
        mic: micOptions(s),
      })
      .then(() => {
        if (this.engine !== engine) return;
        this.rejoins = 0;
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
      peer: (userId, present) => {
        if (!present) patchSession({ speaking: omit(useVoiceSession.getState().speaking, userId) });
      },
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
      !session.selfMute && !session.selfDeaf && !session.serverMute && !session.serverDeaf;
    const open = allowed && (mode === 'ptt' ? session.pttActive : this.vadOpen);
    this.engine?.setTransmitting(open);
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

  private cue(cue: Cue): void {
    const s = getVoiceSettings();
    if (!cueEnabled(cue, s)) return;
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
