/**
 * The voice media engine: voice WebSocket (`sr-voice-proto`) + RTCPeerConnection to the SFU.
 *
 * The UI never touches this directly; it talks to the `VoiceEngine` interface (see controller.ts), so tests
 * and Ladle swap in a mock. `RtcVoiceEngine` is the real implementation, with every browser dependency
 * injected (`EngineDeps`) so it is unit-tested with fake sockets, peer connections and media streams.
 */
import type { SocketFactory, SocketLike } from '../gateway/client';
import type { VoiceIceServer } from '../store/types';
import { MicPipeline, type MicOptions } from './mic';
import { RemoteAudio } from './playback';
import { browserAudioDeps, type AudioDeps, MicError } from './deps';
import {
  encode,
  parseFrame,
  REJOIN_VOICE_CLOSES,
  SPEAKING_PRIORITY,
  SPEAKING_VOICE,
  clampMusicKbps,
  parseStreamId,
  stereoAnswer,
  tuneOpusOffer,
  VoiceClose,
  VoiceOp,
  voiceUrl,
  type ReadyData,
  type SdpData,
  type QualityData,
  type SpeakingData,
  type VideoHint,
  type VideoIn,
  type VideoLayer,
  type VideoPublishOut,
  type VideoSource,
} from './protocol';
import { simulcastEncodings } from './video';
import { activityDataPort, LABEL as DATA_LABEL } from './dataPort';
import {
  inboundSample,
  qualityFromStats,
  remoteQuality,
  UNKNOWN_REMOTE,
  type ConnectionQuality,
  type InboundSample,
  type RemoteQuality,
} from './quality';

export type EngineStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting';

export interface VoiceError {
  code: 'mic' | 'connect-timeout' | 'closed' | 'removed' | 'replaced' | 'full';
  message: string;
  mic?: MicError['code'];
}

export class VoiceConnectError extends Error {
  constructor(readonly voice: VoiceError) {
    super(voice.message);
    this.name = 'VoiceConnectError';
  }
}

export interface ConnectParams {
  endpoint: string;
  token: string;
  /** The viewer's user id (their own SPEAKING echo is ignored). */
  userId: string;
  iceServers?: VoiceIceServer[];
  /** The channel's audio bitrate in bits per second (advertised to the SFU and set on the sender, spec 4.4). */
  maxBitrate?: number;
  mic: MicOptions;
  /** Mark the voice line as high packet priority (DSCP hint and sender priority, LNG-USET-050). */
  highPriority?: boolean;
  /** VOX-E2E-003: end-to-end encrypt every track through the call's media worker. */
  e2ee?: EngineE2ee;
  /** The channel's music bitrate in kbit/s (`voice.music_bitrate_kbps`, 32..320, default 256; MUS-JB-001). */
  musicKbps?: number;
}

/** How the engine hooks the E2EE frame transform into its senders and receivers. */
export interface EngineE2ee {
  /** Extra RTCPeerConnection options (the createEncodedStreams fallback needs one). */
  peerConfig: Record<string, unknown>;
  attachSender(sender: unknown, kind: 'audio' | 'video'): void;
  attachReceiver(receiver: unknown, kind: 'audio' | 'video', userId: string): void;
  /** ACT-CH-001: seal / open an activity data message with the call's keys; `null` means drop it. */
  sealData?(bytes: Uint8Array): Promise<Uint8Array | null>;
  openData?(sender: string, bytes: Uint8Array): Promise<Uint8Array | null>;
}

/** The part of an RTCRtpReceiver the engine reads for per-tile quality. */
export interface StatsReceiver {
  getStats(): Promise<{ forEach(cb: (s: Record<string, unknown>) => void): void }>;
}

export interface EngineEvents {
  status(s: EngineStatus): void;
  speaking(userId: string, speaking: boolean, priority: boolean): void;
  quality(q: ConnectionQuality): void;
  /** Connection quality of a remote person's microphone line, from its receiver's stats; null when they leave (VOX-UI-003). */
  remoteQuality?(userId: string, q: RemoteQuality | null): void;
  /**
   * The voice session cannot continue. `closed`: ask the gateway for a fresh VOICE_SERVER_UPDATE.
   * `removed` (4014): a moderator disconnected or moved us, wait for the gateway; do not rejoin.
   * `replaced` / `full`: stop and tell the viewer.
   */
  fatal(e: VoiceError): void;
  /** Mic level in dBFS, ~30 ms cadence while capturing. */
  level(db: number): void;
  /** The capture device went away (unplugged / revoked). */
  micLost(): void;
  /** Someone's media stream arrived or left. */
  peer(userId: string, present: boolean): void;
  /** The SFU's egress governor reduced (or restored) the quality of this room (VOX-UI-014). */
  governor?(reduced: boolean): void;
  /** Someone's camera or screen stream arrived (`stream`) or went away (`null`). */
  remoteVideo?(userId: string, kind: 'camera' | 'screen', stream: MediaStream | null): void;
  /** Someone's music track arrived (`stream`) or went away (`null`); the engine already plays it (MUS-JB-001). */
  remoteMusic?(userId: string, stream: MediaStream | null): void;
  /** Someone started or stopped publishing a source (op 12 `published`), with its size and frame rate. */
  videoPublished?(
    userId: string,
    source: VideoSource,
    active: boolean,
    size: { width: number; height: number; fps: number },
  ): void;
  /** The SFU switched the simulcast layer it sends us for someone's source (op 12 `layer`). */
  videoLayer?(userId: string, source: VideoSource, layer: VideoLayer): void;
}

/** What a capture is, for the SFU's `publish` message and the sender's encodings. */
export interface PublishInfo {
  width: number;
  height: number;
  fps: number;
}

/** What the rest of the app depends on. */
export interface VoiceEngine {
  readonly status: EngineStatus;
  connect(params: ConnectParams): Promise<void>;
  disconnect(): void;
  /** Open or close the outgoing audio (mute / PTT / VAD result). Also announces SPEAKING. */
  setTransmitting(transmitting: boolean): void;
  setDeafened(deafened: boolean): void;
  setUserVolume(userId: string, percent: number): void;
  setUserMuted(userId: string, muted: boolean): void;
  setMasterVolume(percent: number): void;
  /** Optional: lower the other voices by this percent (attenuation while you speak). */
  setAttenuation?(percent: number): void;
  setInputVolume(percent: number): void;
  setInputDevice(opts: MicOptions): Promise<void>;
  setOutputDevice(deviceId: string): Promise<void>;
  /** Re-run ICE (network change). */
  restartIce(): void;
  /**
   * Send a camera or a screen (with its audio, when the stream has any) on their own lines. The camera goes out as
   * three simulcast layers; the SFU is told which line is which (op 12 `publish`). Replaces a running one.
   */
  publishVideo?(source: 'camera' | 'screen', stream: MediaStream, info: PublishInfo): void;
  /** Stop sending the camera or the screen (and its audio). */
  unpublishVideo?(source: 'camera' | 'screen'): void;
  /** Tell the SFU which sources of which participants to forward, and how big (op 12 `subscribe`). */
  setVideoSubscriptions?(hints: readonly VideoHint[]): void;
  /**
   * Send a stereo music track as a second audio line (MUS-JB-001): `stream` is the player's Web Audio graph
   * through a `MediaStreamAudioDestinationNode`. It rides the same per-sender frame encryptor as the
   * microphone (D-249), is announced to the SFU as `music` and is capped at `kbps` (default: the channel's).
   * Replaces a running one.
   */
  publishMusic?(stream: MediaStream, kbps?: number): void;
  /** Stop sending the music track. */
  unpublishMusic?(): void;
  /** Stop playing everyone's music here and ask the SFU not to send it (a listener mutes music separately). */
  setMusicMuted?(muted: boolean): void;
  /** The listener's music volume, 0..200 percent. */
  setMusicVolume?(percent: number): void;
  /** Mix a soundboard file into the outgoing audio (VOX-SB-003); false when it could not be played. */
  playSound?(data: ArrayBuffer, volume: number): Promise<boolean>;
}

export type EngineEventHandlers = Partial<EngineEvents>;
export type EngineFactory = (events: EngineEvents) => VoiceEngine;

/** The part of an RTCRtpTransceiver the engine reads: its mid (set by the first local description) and stop. */
export interface TransceiverLike {
  readonly mid: string | null;
  /** The RTCRtpSender (absent in some fakes). */
  readonly sender?: unknown;
  stop?(): void;
}

/** A narrow view of RTCPeerConnection (real or fake). */
export interface PeerLike {
  readonly connectionState: string;
  readonly signalingState: string;
  readonly iceGatheringState: string;
  readonly localDescription: { type: string; sdp?: string } | null;
  onconnectionstatechange: (() => void) | null;
  onicegatheringstatechange: (() => void) | null;
  ontrack:
    | ((ev: {
        track: MediaStreamTrack;
        streams: readonly MediaStream[];
        receiver?: unknown;
      }) => void)
    | null;
  addTransceiver(
    track: MediaStreamTrack,
    init: {
      direction: 'sendrecv' | 'sendonly';
      streams: MediaStream[];
      sendEncodings?: RTCRtpEncodingParameters[];
    },
  ): TransceiverLike | undefined;
  createOffer(o?: { iceRestart?: boolean }): Promise<{ type: string; sdp?: string }>;
  createAnswer(): Promise<{ type: string; sdp?: string }>;
  setLocalDescription(d?: { type: string; sdp?: string }): Promise<void>;
  setRemoteDescription(d: { type: string; sdp?: string }): Promise<void>;
  getStats(): Promise<{ forEach(cb: (s: Record<string, unknown>) => void): void }>;
  /** The activity data channel (ACT-CH-001); absent in fakes. */
  createDataChannel?(label: string, init?: { ordered?: boolean }): DataChannelLike;
  close(): void;
}

/** The part of an RTCDataChannel the engine uses. */
export interface DataChannelLike {
  binaryType: string;
  readonly readyState: string;
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  send(data: Uint8Array): void;
  close(): void;
}

export interface EngineDeps {
  createSocket: SocketFactory;
  createPeer(config: { iceServers: VoiceIceServer[]; [option: string]: unknown }): PeerLike;
  audio: AudioDeps;
  /** Browser online/offline hooks (network change -> ICE restart). */
  onOnline(cb: () => void): () => void;
}

export const browserEngineDeps = (): EngineDeps => ({
  createSocket: (url) => new WebSocket(url) as unknown as SocketLike,
  createPeer: (config) => new RTCPeerConnection(config) as unknown as PeerLike,
  audio: browserAudioDeps,
  onOnline: (cb) => {
    window.addEventListener('online', cb);
    return () => window.removeEventListener('online', cb);
  },
});

interface Outgoing {
  source: VideoSource;
  track: MediaStreamTrack;
  stream: MediaStream;
  info: PublishInfo;
  tx: TransceiverLike | null;
  /** The mid the SFU was last told this source is on. */
  announced: string | null;
}

export const CONNECT_TIMEOUT_MS = 15_000;
export const ICE_GATHER_TIMEOUT_MS = 2_500;
export const QUALITY_INTERVAL_MS = 2_000;
/** A peer connection that stays "disconnected" this long gets an ICE restart. */
export const DISCONNECT_GRACE_MS = 2_000;
export const MAX_ICE_RESTARTS = 3;
export const MAX_SOCKET_RETRIES = 5;

const OPEN = 1;

export class RtcVoiceEngine implements VoiceEngine {
  status: EngineStatus = 'idle';
  private params: ConnectParams | null = null;
  private ws: SocketLike | null = null;
  private pc: PeerLike | null = null;
  private mic: MicPipeline;
  private playback: RemoteAudio;
  private heartbeat: ReturnType<typeof setInterval> | undefined;
  private awaitingAck = false;
  private qualityTimer: ReturnType<typeof setInterval> | undefined;
  private graceTimer: ReturnType<typeof setTimeout> | undefined;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private connectTimer: ReturnType<typeof setTimeout> | undefined;
  private wsGen = 0;
  private socketRetries = 0;
  private iceRestarts = 0;
  private transmitting = false;
  private announced = false;
  private voiceSessionId = '';
  private resumeToken = '';
  private resolveConnect: (() => void) | null = null;
  private rejectConnect: ((e: Error) => void) | null = null;
  private offerPending = false;
  /** Our own offer must be (re)sent once the line is free / the socket is back. */
  private reoffer: { restart: boolean } | null = null;
  private chain: Promise<void> = Promise.resolve();
  private offLevel: (() => void) | null = null;
  private offLost: (() => void) | null = null;
  private offOnline: (() => void) | null = null;
  private userVolumes = new Map<string, number>();
  private userMuted = new Set<string>();
  private deafened = false;
  private remoteStreams = new Map<string, MediaStream>();
  /** The receivers of remote microphone lines, read for per-tile quality (VOX-UI-003). */
  private remoteReceivers = new Map<
    string,
    {
      receiver: StatsReceiver;
      last: InboundSample | null;
      quality: RemoteQuality;
    }
  >();
  /** Our own camera / screen / screen audio lines. */
  private outgoing = new Map<VideoSource, Outgoing>();
  /** The last subscription hints, sent again after a reconnect. */
  private hints: readonly VideoHint[] | null = null;
  /** The listener turned music off: playback is muted and the SFU is told not to send it (MUS-JB-001). */
  private musicMuted = false;
  /** People who published a music track, so the mute can name them. */
  private musicUsers = new Set<string>();
  /** The bitrate of our own music track, kbit/s. */
  private musicKbps = 256;
  /** Remote music streams by user id. */
  private remoteMusic = new Map<string, MediaStream>();
  /** Remote camera / screen streams by `<user>:<kind>`. */
  private remoteVideo = new Map<string, MediaStream>();

  constructor(
    private readonly events: EngineEvents,
    private readonly deps: EngineDeps = browserEngineDeps(),
  ) {
    this.mic = new MicPipeline(deps.audio);
    this.playback = new RemoteAudio(deps.audio);
    // e2e builds only (VITE_SR_E2E=1, like `__srE2ee`): a browser test plays a generated stereo tone as the music
    // track through this handle (MUS-JB-001).
    if ((import.meta.env as { VITE_SR_E2E?: string }).VITE_SR_E2E === '1')
      (globalThis as unknown as { __srVoiceEngine?: VoiceEngine }).__srVoiceEngine = this;
  }

  private setStatus(s: EngineStatus): void {
    if (this.status === s) return;
    this.status = s;
    this.events.status(s);
  }

  async connect(params: ConnectParams): Promise<void> {
    this.teardown();
    this.params = params;
    this.setStatus('connecting');
    try {
      await this.mic.start(params.mic);
    } catch (e) {
      this.teardown();
      const mic = e instanceof MicError ? e : undefined;
      throw new VoiceConnectError({
        code: 'mic',
        message: mic?.message ?? 'Could not start the microphone.',
        ...(mic ? { mic: mic.code } : {}),
      });
    }
    this.mic.setGate(this.transmitting);
    this.mic.setInputVolume(params.mic.inputVolume);
    this.offLevel = this.mic.onLevel((db) => this.events.level(db));
    this.offLost = this.mic.onLost(() => this.events.micLost());
    this.offOnline = this.deps.onOnline(() => this.restartIce());
    const done = new Promise<void>((resolve, reject) => {
      this.resolveConnect = resolve;
      this.rejectConnect = reject;
    });
    this.connectTimer = setTimeout(() => {
      this.failConnect(
        new VoiceConnectError({
          code: 'connect-timeout',
          message: 'Timed out connecting to voice.',
        }),
      );
    }, CONNECT_TIMEOUT_MS);
    this.openSocket(false);
    return done;
  }

  private failConnect(e: Error): void {
    const reject = this.rejectConnect;
    this.resolveConnect = this.rejectConnect = null;
    clearTimeout(this.connectTimer);
    this.teardown();
    reject?.(e);
  }

  private succeedConnect(): void {
    clearTimeout(this.connectTimer);
    const resolve = this.resolveConnect;
    this.resolveConnect = this.rejectConnect = null;
    this.setStatus('connected');
    this.applyBitrate();
    resolve?.();
  }

  // ---- socket ----

  private openSocket(resume: boolean): void {
    const p = this.params;
    if (!p) return;
    const gen = ++this.wsGen;
    const ws = this.deps.createSocket(voiceUrl(p.endpoint));
    this.ws = ws;
    ws.onopen = () => {
      if (gen !== this.wsGen) return;
      if (resume && this.voiceSessionId) {
        this.send(VoiceOp.RESUME, {
          session_id: this.voiceSessionId,
          resume_token: this.resumeToken,
        });
      } else {
        this.send(VoiceOp.IDENTIFY, { token: p.token });
      }
    };
    ws.onmessage = (m) => {
      if (gen === this.wsGen) this.onFrame(m.data);
    };
    ws.onclose = (ev) => {
      if (gen === this.wsGen) this.onSocketClose(ev.code);
    };
    ws.onerror = () => {
      /* followed by close */
    };
  }

  private send(op: number, d?: unknown): boolean {
    if (!this.ws || this.ws.readyState !== OPEN) return false;
    this.ws.send(encode(op, d));
    return true;
  }

  private startHeartbeat(interval: number): void {
    clearInterval(this.heartbeat);
    this.awaitingAck = false;
    this.heartbeat = setInterval(() => {
      if (this.awaitingAck) {
        // No ack for a whole interval: the socket is dead. Treat it like a heartbeat-timeout close.
        const ws = this.ws;
        this.wsGen++;
        this.ws = null;
        if (ws) {
          ws.onmessage = ws.onclose = ws.onerror = ws.onopen = null;
          try {
            ws.close(4000, 'heartbeat ack timeout');
          } catch {
            /* already closed */
          }
        }
        this.onSocketClose(VoiceClose.HEARTBEAT_TIMEOUT);
        return;
      }
      this.awaitingAck = true;
      this.send(VoiceOp.HEARTBEAT, { t: Date.now() });
    }, interval);
  }

  private onFrame(raw: unknown): void {
    const f = parseFrame(raw);
    if (!f || !this.params) return;
    switch (f.op) {
      case VoiceOp.READY: {
        const d = f.d as ReadyData;
        this.voiceSessionId = d.session_id;
        this.resumeToken = d.resume_token;
        this.startHeartbeat(d.heartbeat_interval || 13_750);
        this.applyParticipants(d);
        this.enqueue(() => this.negotiate(false));
        break;
      }
      case VoiceOp.RESUMED: {
        const d = (f.d ?? {}) as Partial<ReadyData>;
        this.socketRetries = 0;
        this.startHeartbeat(d.heartbeat_interval || 13_750);
        this.applyParticipants(d);
        if (this.reoffer) this.enqueue(() => this.negotiate(this.reoffer?.restart ?? false));
        else if (this.pc?.connectionState === 'connected') this.setStatus('connected');
        break;
      }
      case VoiceOp.HEARTBEAT_ACK:
        this.awaitingAck = false;
        break;
      case VoiceOp.SDP_ANSWER:
        this.enqueue(() => this.onAnswer((f.d as SdpData).sdp));
        break;
      case VoiceOp.SDP_OFFER:
        this.enqueue(() => this.onServerOffer((f.d as SdpData).sdp));
        break;
      case VoiceOp.SPEAKING: {
        const d = f.d as SpeakingData | undefined;
        if (d?.user_id && d.user_id !== this.params.userId) {
          const priority = (d.speaking & SPEAKING_PRIORITY) !== 0;
          this.playback.setPriority(d.user_id, priority && d.speaking !== 0);
          this.events.speaking(d.user_id, (d.speaking & SPEAKING_VOICE) !== 0, priority);
        }
        break;
      }
      case VoiceOp.QUALITY: {
        const d = f.d as QualityData | undefined;
        if (typeof d?.reduced === 'boolean') this.events.governor?.(d.reduced);
        break;
      }
      case VoiceOp.VIDEO:
        this.onVideoFrame(f.d as VideoIn | undefined);
        break;
      case VoiceOp.CLIENT_DISCONNECT: {
        const id = (f.d as { user_id?: string } | undefined)?.user_id;
        if (id) this.dropPeer(id);
        break;
      }
      default:
        break;
    }
  }

  private applyParticipants(d: Partial<ReadyData>): void {
    for (const p of d.participants ?? []) {
      if (p.user_id === this.params?.userId) continue;
      this.playback.setPriority(p.user_id, (p.speaking & SPEAKING_PRIORITY) !== 0);
      this.events.speaking(p.user_id, (p.speaking & SPEAKING_VOICE) !== 0, false);
    }
  }

  private onSocketClose(code: number): void {
    clearInterval(this.heartbeat);
    this.ws = null;
    if (!this.params) return;
    const fail = (err: VoiceError) => {
      if (this.rejectConnect) this.failConnect(new VoiceConnectError(err));
      else {
        this.teardown();
        this.events.fatal(err);
      }
    };
    if (code === VoiceClose.DISCONNECTED) {
      fail({ code: 'removed', message: 'You were removed from the call.' });
    } else if (code === VoiceClose.SESSION_REPLACED) {
      fail({ code: 'replaced', message: 'You joined this voice channel from another window.' });
    } else if (code === VoiceClose.SERVER_FULL) {
      fail({ code: 'full', message: 'This voice server is full. Try again in a moment.' });
    } else if (REJOIN_VOICE_CLOSES.has(code) || (!this.voiceSessionId && this.rejectConnect)) {
      fail({ code: 'closed', message: `Voice session ended (${code}).` });
    } else if (this.socketRetries >= MAX_SOCKET_RETRIES) {
      fail({ code: 'closed', message: 'Lost the voice connection.' });
    } else {
      this.setStatus('reconnecting');
      const delay = Math.min(8000, 500 * 2 ** this.socketRetries++);
      clearTimeout(this.retryTimer);
      this.retryTimer = setTimeout(() => this.openSocket(true), delay);
    }
  }

  // ---- WebRTC ----

  private buildPeer(): PeerLike {
    const e2ee = this.params?.e2ee;
    const pc = this.deps.createPeer({
      iceServers: this.params?.iceServers ?? [],
      ...(e2ee?.peerConfig ?? {}),
    });
    // ACT-CH-001: the activity data channel rides the same peer connection, sealed with the call's keys.
    if (e2ee?.sealData && pc.createDataChannel) {
      activityDataPort.attach(pc.createDataChannel(DATA_LABEL, { ordered: true }), e2ee);
    }
    const out = this.mic.outStream;
    const track = this.mic.track;
    if (track && out) {
      const tx = pc.addTransceiver(track, {
        direction: 'sendonly',
        streams: [out],
        ...(this.params?.highPriority
          ? {
              sendEncodings: [
                { priority: 'high', networkPriority: 'high' } as RTCRtpEncodingParameters,
              ],
            }
          : {}),
      });
      if (e2ee) e2ee.attachSender(tx?.sender, 'audio');
    }
    for (const o of this.outgoing.values()) this.attach(pc, o);
    pc.ontrack = (ev) => {
      const stream = ev.streams[0];
      if (!stream) return;
      // The SFU labels each forwarded line's msid stream: the speaker's user id for the mic (spec 4.4),
      // `<user>-camera` and `<user>-screen` for video (the screen's audio shares the screen's stream).
      const id = parseStreamId(stream.id);
      if (e2ee)
        e2ee.attachReceiver(ev.receiver, ev.track.kind === 'video' ? 'video' : 'audio', id.userId);
      if (id.kind === 'music') {
        this.addRemoteMusic(id.userId, stream);
      } else if (id.kind !== 'mic') {
        this.addRemoteVideo(id.userId, id.kind, stream);
      } else if (ev.track.kind === 'audio') {
        const r = ev.receiver as Partial<StatsReceiver> | undefined;
        if (r?.getStats) {
          this.remoteReceivers.set(id.userId, {
            receiver: r as StatsReceiver,
            last: null,
            quality: UNKNOWN_REMOTE,
          });
        }
        this.addPeer(id.userId, stream);
      }
    };
    pc.onconnectionstatechange = () => this.onPeerState(pc);
    return pc;
  }

  /** Serialize SDP work so offers, answers and rollbacks never interleave. */
  private enqueue(fn: () => Promise<void>): void {
    this.chain = this.chain.then(fn).catch((e: unknown) => {
      if (this.resolveConnect) {
        this.failConnect(e instanceof Error ? e : new Error(String(e)));
      }
    });
  }

  private async negotiate(restart: boolean): Promise<void> {
    if (!this.params) return;
    if (!this.pc) this.pc = this.buildPeer();
    const pc = this.pc;
    if (this.offerPending) {
      // One outstanding offer at a time: ask again once the answer is in.
      this.reoffer = { restart: restart || (this.reoffer?.restart ?? false) };
      return;
    }
    this.reoffer = null;
    this.offerPending = true;
    const offer = await pc.createOffer(restart ? { iceRestart: true } : undefined);
    // Opus DTX / FEC / mono go into the local description too, so what we sent is what we run.
    // The music line (when one is attached) gets its own stereo Opus payload type and profile; every other audio
    // line, the microphone first, stays mono with no stereo fmtp (MUS-JB-001).
    const music = this.outgoing.get('music');
    await pc.setLocalDescription({
      type: 'offer',
      sdp: tuneOpusOffer(
        offer.sdp ?? '',
        this.params.maxBitrate,
        music
          ? { streamId: music.stream.id, mid: music.tx?.mid ?? null, kbps: this.musicKbps }
          : null,
      ),
    });
    await this.gathered(pc);
    const sdp = pc.localDescription?.sdp ?? offer.sdp ?? '';
    if (!this.send(VoiceOp.SDP_OFFER, { sdp })) {
      // Socket is down: the resume handler sends it when the socket is back.
      this.offerPending = false;
      this.reoffer = { restart };
      await pc.setLocalDescription({ type: 'rollback' });
    } else {
      this.announceOutgoing();
      if (this.hints) this.sendHints(this.hints);
    }
  }

  private gathered(pc: PeerLike): Promise<void> {
    if (pc.iceGatheringState === 'complete') return Promise.resolve();
    return new Promise((resolve) => {
      const t = setTimeout(done, ICE_GATHER_TIMEOUT_MS);
      function done() {
        clearTimeout(t);
        pc.onicegatheringstatechange = null;
        resolve();
      }
      pc.onicegatheringstatechange = () => {
        if (pc.iceGatheringState === 'complete') done();
      };
    });
  }

  private async onAnswer(sdp: string): Promise<void> {
    const pc = this.pc;
    if (!pc || !this.offerPending) return;
    this.offerPending = false;
    await pc.setRemoteDescription({ type: 'answer', sdp });
    if (pc.connectionState === 'connected') this.onPeerState(pc);
    if (this.reoffer) await this.negotiate(this.reoffer.restart);
  }

  /**
   * The SFU renegotiates when a remote line is added or removed. On glare the client is the impolite peer
   * (spec 4.4): while our own offer is outstanding a server offer is ignored, the server abandons it, answers
   * ours and sends a fresh offer afterwards.
   */
  private async onServerOffer(sdp: string): Promise<void> {
    const pc = this.pc;
    if (!pc) return;
    if (this.offerPending || pc.signalingState === 'have-local-offer') return;
    await pc.setRemoteDescription({ type: 'offer', sdp });
    const answer = await pc.createAnswer();
    // Keep `stereo=1` on the music payload type so the decoder outputs two channels.
    await pc.setLocalDescription(
      answer.sdp ? { type: answer.type, sdp: stereoAnswer(answer.sdp) } : answer,
    );
    this.send(VoiceOp.SDP_ANSWER, { sdp: pc.localDescription?.sdp ?? answer.sdp });
    if (this.reoffer) await this.negotiate(this.reoffer.restart);
  }

  private onPeerState(pc: PeerLike): void {
    if (pc !== this.pc) return;
    const s = pc.connectionState;
    clearTimeout(this.graceTimer);
    if (s === 'connected') {
      this.iceRestarts = 0;
      this.socketRetries = 0;
      this.startQuality();
      if (this.status !== 'connected' || this.resolveConnect) this.succeedConnect();
    } else if (s === 'disconnected') {
      this.setStatus('reconnecting');
      this.graceTimer = setTimeout(() => this.restartIce(), DISCONNECT_GRACE_MS);
    } else if (s === 'failed') {
      this.restartIce();
    }
  }

  restartIce(): void {
    if (!this.pc || !this.params) return;
    if (this.iceRestarts >= MAX_ICE_RESTARTS) {
      this.teardown();
      this.events.fatal({ code: 'closed', message: 'The voice connection could not be restored.' });
      return;
    }
    this.iceRestarts++;
    this.setStatus('reconnecting');
    this.enqueue(() => this.negotiate(true));
  }

  /** Cap the Opus sender at the channel bitrate. The SFU trusts but does not enforce it in v1. */
  private applyBitrate(): void {
    const max = this.params?.maxBitrate;
    const pc = this.pc as (PeerLike & { getSenders?: () => RTCRtpSender[] }) | null;
    if (!max || !pc?.getSenders) return;
    const musicSender = this.outgoing.get('music')?.tx?.sender;
    for (const sender of pc.getSenders()) {
      if (sender.track?.kind !== 'audio') continue;
      const p = sender.getParameters();
      p.encodings = p.encodings?.length ? p.encodings : [{}];
      // The microphone stays at the channel bitrate; the music line has its own ceiling.
      p.encodings[0]!.maxBitrate = sender === musicSender ? this.musicKbps * 1000 : max;
      void sender.setParameters(p).catch(() => undefined);
    }
  }

  /** Read each remote microphone line's receiver and tell the controller how it is doing (VOX-UI-003). */
  private pollRemote(): void {
    for (const [userId, entry] of this.remoteReceivers) {
      entry.receiver
        .getStats()
        .then((r) => {
          if (this.remoteReceivers.get(userId) !== entry) return;
          const cur = inboundSample(r, Date.now());
          if (!cur) return;
          const q = remoteQuality(entry.last, cur, entry.quality);
          entry.last = cur;
          entry.quality = q;
          this.events.remoteQuality?.(userId, q);
        })
        .catch(() => undefined);
    }
  }

  private startQuality(): void {
    if (this.qualityTimer) return;
    const poll = () => {
      const pc = this.pc;
      if (!pc) return;
      pc.getStats()
        .then((r) => this.events.quality(qualityFromStats(r)))
        .catch(() => undefined);
      this.pollRemote();
    };
    poll();
    this.qualityTimer = setInterval(poll, QUALITY_INTERVAL_MS);
  }

  // ---- people ----

  private addPeer(userId: string, stream: MediaStream): void {
    this.remoteStreams.set(userId, stream);
    this.playback.add(
      userId,
      stream,
      this.userVolumes.get(userId) ?? 100,
      this.userMuted.has(userId),
    );
    this.playback.setDeafened(this.deafened);
    this.events.peer(userId, true);
  }

  private dropPeer(userId: string): void {
    this.dropRemoteMusic(userId);
    this.musicUsers.delete(userId);
    this.remoteStreams.delete(userId);
    this.playback.remove(userId);
    this.events.peer(userId, false);
    if (this.remoteReceivers.delete(userId)) this.events.remoteQuality?.(userId, null);
    for (const key of [...this.remoteVideo.keys()]) {
      if (key.startsWith(`${userId}:`)) this.dropRemoteVideo(key);
    }
  }

  // ---- video ----

  private addRemoteVideo(userId: string, kind: 'camera' | 'screen', stream: MediaStream): void {
    const key = `${userId}:${kind}`;
    if (this.remoteVideo.get(key) !== stream) {
      this.remoteVideo.set(key, stream);
      this.events.remoteVideo?.(userId, kind, stream);
      // The line is removed (the publisher stopped) when the stream loses its last track.
      stream.addEventListener?.('removetrack', () => {
        if (stream.getTracks().length === 0 && this.remoteVideo.get(key) === stream) {
          this.dropRemoteVideo(key);
        }
      });
    }
  }

  private dropRemoteVideo(key: string): void {
    if (!this.remoteVideo.delete(key)) return;
    const [userId, kind] = key.split(':') as [string, 'camera' | 'screen'];
    this.events.remoteVideo?.(userId, kind, null);
  }

  private onVideoFrame(m: VideoIn | undefined): void {
    if (!m || typeof m !== 'object') return;
    if (m.t === 'published') {
      this.events.videoPublished?.(m.user_id, m.source, m.active, {
        width: m.width ?? 0,
        height: m.height ?? 0,
        fps: m.fps ?? 0,
      });
      if (m.source === 'music') {
        if (m.active) this.musicUsers.add(m.user_id);
        else this.dropRemoteMusic(m.user_id);
        // A listener who muted music asks the SFU for none of the new publisher's either.
        if (m.active && this.musicMuted) this.sendHints(this.hints ?? []);
      } else if (!m.active && m.source !== 'screen_audio') {
        this.dropRemoteVideo(`${m.user_id}:${m.source}`);
      }
    } else if (m.t === 'layer') {
      this.events.videoLayer?.(m.user_id, m.source, m.layer);
    }
  }

  publishVideo(source: 'camera' | 'screen', stream: MediaStream, info: PublishInfo): void {
    this.unpublishVideo(source, false);
    const video = stream.getVideoTracks()[0];
    if (!video) return;
    const added: Outgoing[] = [{ source, track: video, stream, info, tx: null, announced: null }];
    const audio = source === 'screen' ? stream.getAudioTracks()[0] : undefined;
    if (audio) {
      added.push({ source: 'screen_audio', track: audio, stream, info, tx: null, announced: null });
    }
    for (const o of added) this.outgoing.set(o.source, o);
    if (this.pc) {
      for (const o of added) this.attach(this.pc, o);
      this.enqueue(() => this.negotiate(false));
    }
  }

  unpublishVideo(source: 'camera' | 'screen', renegotiate = true): void {
    const sources: VideoSource[] = source === 'screen' ? ['screen', 'screen_audio'] : [source];
    let changed = false;
    for (const s of sources) {
      const o = this.outgoing.get(s);
      if (!o) continue;
      this.outgoing.delete(s);
      if (o.announced !== null) {
        this.sendVideo({
          t: 'publish',
          mid: o.announced,
          source: s,
          active: false,
          width: 0,
          height: 0,
          fps: 0,
        });
      }
      if (o.tx) {
        try {
          o.tx.stop?.();
        } catch {
          /* already stopped */
        }
        changed = true;
      }
    }
    if (changed && renegotiate && this.pc) this.enqueue(() => this.negotiate(false));
  }

  setVideoSubscriptions(hints: readonly VideoHint[]): void {
    this.hints = hints;
    this.sendHints(hints);
  }

  private sendHints(hints: readonly VideoHint[]): void {
    // The music mute rides along with the caller's hints (the SFU replaces the whole list on each message).
    const mutes: VideoHint[] = this.musicMuted
      ? [...this.musicUsers].map((user_id) => ({ user_id, source: 'music', enabled: false }))
      : [];
    this.send(VoiceOp.VIDEO, { t: 'subscribe', subscriptions: [...hints, ...mutes] });
  }

  // ---- music (MUS-JB-001) ----

  publishMusic(stream: MediaStream, kbps?: number): void {
    const track = stream.getAudioTracks()[0];
    if (!track) return;
    this.unpublishMusic(false);
    this.musicKbps = clampMusicKbps(kbps ?? this.params?.musicKbps);
    const o: Outgoing = {
      source: 'music',
      track,
      stream,
      info: { width: 0, height: 0, fps: 0 },
      tx: null,
      announced: null,
    };
    this.outgoing.set('music', o);
    if (this.pc) {
      this.attach(this.pc, o);
      this.enqueue(() => this.negotiate(false));
    }
  }

  unpublishMusic(renegotiate = true): void {
    const o = this.outgoing.get('music');
    if (!o) return;
    this.outgoing.delete('music');
    if (o.announced !== null) {
      this.sendVideo({
        t: 'publish',
        mid: o.announced,
        source: 'music',
        active: false,
        width: 0,
        height: 0,
        fps: 0,
      });
    }
    let changed = false;
    if (o.tx) {
      try {
        o.tx.stop?.();
      } catch {
        /* already stopped */
      }
      changed = true;
    }
    if (changed && renegotiate && this.pc) this.enqueue(() => this.negotiate(false));
  }

  setMusicMuted(muted: boolean): void {
    this.musicMuted = muted;
    this.playback.setMusicMuted(muted);
    this.sendHints(this.hints ?? []);
  }

  setMusicVolume(percent: number): void {
    this.playback.setMusicVolume(percent);
  }

  private addRemoteMusic(userId: string, stream: MediaStream): void {
    if (this.remoteMusic.get(userId) === stream) return;
    this.remoteMusic.set(userId, stream);
    this.musicUsers.add(userId);
    this.playback.addMusic(userId, stream);
    this.playback.setDeafened(this.deafened);
    this.events.remoteMusic?.(userId, stream);
    // The line is removed (the DJ stopped) when the stream loses its last track.
    stream.addEventListener?.('removetrack', () => {
      if (stream.getTracks().length === 0 && this.remoteMusic.get(userId) === stream) {
        this.dropRemoteMusic(userId);
      }
    });
  }

  private dropRemoteMusic(userId: string): void {
    if (!this.remoteMusic.delete(userId)) return;
    this.playback.removeMusic(userId);
    this.events.remoteMusic?.(userId, null);
  }

  private sendVideo(m: VideoPublishOut): void {
    this.send(VoiceOp.VIDEO, m);
  }

  /** Put one of our captures on a sendonly line of the peer. The camera is three simulcast layers. */
  private attach(pc: PeerLike, o: Outgoing): void {
    if (o.tx) return;
    const { info } = o;
    const sendEncodings =
      o.source === 'camera'
        ? simulcastEncodings({ height: info.height, fps: info.fps })
        : o.source === 'screen'
          ? [{ maxFramerate: info.fps, maxBitrate: screenBitrate(info) }]
          : o.source === 'music'
            ? [{ maxBitrate: this.musicKbps * 1000, priority: 'high' } as RTCRtpEncodingParameters]
            : undefined;
    o.tx =
      pc.addTransceiver(o.track, {
        direction: 'sendonly',
        streams: [o.stream],
        ...(sendEncodings ? { sendEncodings } : {}),
      }) ?? null;
    this.params?.e2ee?.attachSender(o.tx?.sender, o.track.kind === 'audio' ? 'audio' : 'video');
    o.announced = null;
  }

  /** Name our lines to the SFU once the offer has given them mids (op 12 `publish`). */
  private announceOutgoing(): void {
    for (const o of this.outgoing.values()) {
      const mid = o.tx?.mid;
      if (!mid || o.announced === mid) continue;
      o.announced = mid;
      this.sendVideo({
        t: 'publish',
        mid,
        source: o.source,
        active: true,
        width: o.info.width,
        height: o.info.height,
        fps: o.info.fps,
      });
    }
  }

  // ---- controls ----

  setTransmitting(transmitting: boolean): void {
    const was = this.transmitting;
    this.transmitting = transmitting;
    this.mic.setGate(transmitting);
    if (transmitting !== was || !this.announced) {
      this.announced = true;
      // Spec 4.3: PTT must report 1 while held and 0 after the release delay (the SFU enforces it).
      this.send(VoiceOp.SPEAKING, { speaking: transmitting ? SPEAKING_VOICE : 0 });
    }
  }

  setDeafened(deafened: boolean): void {
    this.deafened = deafened;
    this.playback.setDeafened(deafened);
  }
  setUserVolume(userId: string, percent: number): void {
    this.userVolumes.set(userId, percent);
    this.playback.setVolume(userId, percent);
  }
  setUserMuted(userId: string, muted: boolean): void {
    if (muted) this.userMuted.add(userId);
    else this.userMuted.delete(userId);
    this.playback.setMuted(userId, muted);
  }
  setMasterVolume(percent: number): void {
    this.playback.setMasterVolume(percent);
  }
  setAttenuation(percent: number): void {
    this.playback.setAttenuation(percent);
  }
  setInputVolume(percent: number): void {
    this.mic.setInputVolume(percent);
  }
  playSound(data: ArrayBuffer, volume: number): Promise<boolean> {
    return this.mic.playSound(data, volume);
  }
  setOutputDevice(deviceId: string): Promise<void> {
    return this.playback.setOutputDevice(deviceId);
  }

  /** Swap the capture device mid-call: the new track replaces the sender's, no renegotiation. */
  async setInputDevice(opts: MicOptions): Promise<void> {
    if (this.status === 'idle') return;
    const pc = this.pc as (PeerLike & { getSenders?: () => RTCRtpSender[] }) | null;
    await this.mic.start(opts);
    this.mic.setGate(this.transmitting);
    const track = this.mic.track;
    // The microphone is the first audio sender; the music and screen-audio lines are not.
    const others = new Set<unknown>(
      [this.outgoing.get('music'), this.outgoing.get('screen_audio')].map((o) => o?.tx?.sender),
    );
    const sender = pc
      ?.getSenders?.()
      .find((s) => (s.track?.kind === 'audio' || s.track === null) && !others.has(s));
    if (sender && track) await sender.replaceTrack(track);
    this.offLevel?.();
    this.offLost?.();
    this.offLevel = this.mic.onLevel((db) => this.events.level(db));
    this.offLost = this.mic.onLost(() => this.events.micLost());
  }

  disconnect(): void {
    this.teardown();
  }

  private teardown(): void {
    this.wsGen++;
    clearInterval(this.heartbeat);
    clearInterval(this.qualityTimer);
    clearTimeout(this.graceTimer);
    clearTimeout(this.retryTimer);
    clearTimeout(this.connectTimer);
    this.qualityTimer = undefined;
    this.remoteReceivers.clear();
    this.offLevel?.();
    this.offLost?.();
    this.offOnline?.();
    this.offLevel = this.offLost = this.offOnline = null;
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      ws.onmessage = ws.onclose = ws.onerror = ws.onopen = null;
      try {
        ws.close(1000, 'client leaving');
      } catch {
        /* already closed */
      }
    }
    if (this.pc) {
      activityDataPort.detach();
      this.pc.onconnectionstatechange = this.pc.onicegatheringstatechange = this.pc.ontrack = null;
      this.pc.close();
      this.pc = null;
    }
    this.mic.stop();
    for (const id of [...this.remoteStreams.keys()]) this.dropPeer(id);
    for (const key of [...this.remoteVideo.keys()]) this.dropRemoteVideo(key);
    for (const id of [...this.remoteMusic.keys()]) this.dropRemoteMusic(id);
    this.musicUsers.clear();
    this.outgoing.clear();
    this.hints = null;
    this.playback.dispose();
    this.params = null;
    this.socketRetries = this.iceRestarts = 0;
    this.offerPending = false;
    this.reoffer = null;
    this.chain = Promise.resolve();
    this.voiceSessionId = this.resumeToken = '';
    this.announced = false;
    const rej = this.rejectConnect;
    this.resolveConnect = this.rejectConnect = null;
    rej?.(new Error('Voice connection closed'));
    this.setStatus('idle');
  }
}

/** A screen share's bitrate cap: the camera's ladder, a little richer for text and slides. */
function screenBitrate(info: PublishInfo): number {
  const kbps = info.height <= 480 ? 1200 : info.height <= 720 ? 2500 : 5000;
  return Math.round(kbps * (info.fps > 30 ? 1.4 : 1)) * 1000;
}
