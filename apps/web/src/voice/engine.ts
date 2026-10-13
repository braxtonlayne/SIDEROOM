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
  FATAL_VOICE_CLOSES,
  parseFrame,
  SPEAKING_VOICE,
  tuneOpus,
  VoiceOp,
  voiceUrl,
  type HelloData,
  type ReadyData,
  type SessionDescriptionData,
  type SpeakingData,
} from './protocol';
import { qualityFromStats, type ConnectionQuality } from './quality';

export type EngineStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting';

export interface VoiceError {
  code: 'mic' | 'connect-timeout' | 'server' | 'closed';
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
  guildId: string;
  userId: string;
  sessionId: string;
  iceServers?: VoiceIceServer[];
  mic: MicOptions;
}

export interface EngineEvents {
  status(s: EngineStatus): void;
  speaking(userId: string, speaking: boolean): void;
  quality(q: ConnectionQuality): void;
  /** The voice session cannot continue: ask the gateway for a fresh VOICE_SERVER_UPDATE. */
  fatal(e: VoiceError): void;
  /** Mic level in dBFS, ~30 ms cadence while capturing. */
  level(db: number): void;
  /** The capture device went away (unplugged / revoked). */
  micLost(): void;
  /** Someone's media stream arrived or left. */
  peer(userId: string, present: boolean): void;
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
  setInputVolume(percent: number): void;
  setInputDevice(opts: MicOptions): Promise<void>;
  setOutputDevice(deviceId: string): Promise<void>;
  /** Re-run ICE (network change). */
  restartIce(): void;
}

export type EngineEventHandlers = Partial<EngineEvents>;
export type EngineFactory = (events: EngineEvents) => VoiceEngine;

/** A narrow view of RTCPeerConnection (real or fake). */
export interface PeerLike {
  readonly connectionState: string;
  readonly iceGatheringState: string;
  readonly localDescription: { type: string; sdp?: string } | null;
  onconnectionstatechange: (() => void) | null;
  onicegatheringstatechange: (() => void) | null;
  ontrack: ((ev: { track: MediaStreamTrack; streams: readonly MediaStream[] }) => void) | null;
  addTransceiver(
    track: MediaStreamTrack,
    init: { direction: 'sendrecv' | 'sendonly'; streams: MediaStream[] },
  ): unknown;
  createOffer(o?: { iceRestart?: boolean }): Promise<{ type: string; sdp?: string }>;
  createAnswer(): Promise<{ type: string; sdp?: string }>;
  setLocalDescription(d?: { type: string; sdp?: string }): Promise<void>;
  setRemoteDescription(d: { type: string; sdp?: string }): Promise<void>;
  getStats(): Promise<{ forEach(cb: (s: Record<string, unknown>) => void): void }>;
  close(): void;
}

export interface EngineDeps {
  createSocket: SocketFactory;
  createPeer(config: { iceServers: VoiceIceServer[] }): PeerLike;
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
  private qualityTimer: ReturnType<typeof setInterval> | undefined;
  private graceTimer: ReturnType<typeof setTimeout> | undefined;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private connectTimer: ReturnType<typeof setTimeout> | undefined;
  private wsGen = 0;
  private socketRetries = 0;
  private iceRestarts = 0;
  private transmitting = false;
  private announced = false;
  private ssrc = 0;
  private resolveConnect: (() => void) | null = null;
  private rejectConnect: ((e: Error) => void) | null = null;
  private pendingRestart = false;
  private offLevel: (() => void) | null = null;
  private offLost: (() => void) | null = null;
  private offOnline: (() => void) | null = null;
  private userVolumes = new Map<string, number>();
  private userMuted = new Set<string>();
  private deafened = false;
  private remoteStreams = new Map<string, MediaStream>();

  constructor(
    private readonly events: EngineEvents,
    private readonly deps: EngineDeps = browserEngineDeps(),
  ) {
    this.mic = new MicPipeline(deps.audio);
    this.playback = new RemoteAudio(deps.audio);
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
    resolve?.();
  }

  // ---- socket ----

  private openSocket(resume: boolean): void {
    const p = this.params;
    if (!p) return;
    const gen = ++this.wsGen;
    const ws = this.deps.createSocket(voiceUrl(p.endpoint));
    this.ws = ws;
    ws.onmessage = (m) => {
      if (gen === this.wsGen) this.onFrame(m.data, resume);
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

  private onFrame(raw: unknown, resuming: boolean): void {
    const f = parseFrame(raw);
    const p = this.params;
    if (!f || !p) return;
    switch (f.op) {
      case VoiceOp.HELLO: {
        const interval = (f.d as HelloData | undefined)?.heartbeat_interval ?? 15_000;
        clearInterval(this.heartbeat);
        this.heartbeat = setInterval(() => this.send(VoiceOp.HEARTBEAT, Date.now()), interval);
        if (resuming && this.pc) {
          this.send(VoiceOp.RESUME, {
            server_id: p.guildId,
            session_id: p.sessionId,
            token: p.token,
          });
        } else {
          this.send(VoiceOp.IDENTIFY, {
            server_id: p.guildId,
            user_id: p.userId,
            session_id: p.sessionId,
            token: p.token,
          });
        }
        break;
      }
      case VoiceOp.READY: {
        const d = (f.d ?? {}) as ReadyData;
        this.ssrc = d.ssrc ?? 0;
        void this.negotiate(false, d.ice_servers).catch((e) =>
          this.failConnect(e instanceof Error ? e : new Error(String(e))),
        );
        break;
      }
      case VoiceOp.RESUMED:
        this.socketRetries = 0;
        if (this.pendingRestart) void this.negotiate(true).catch(() => undefined);
        else if (this.pc?.connectionState === 'connected') this.setStatus('connected');
        break;
      case VoiceOp.SESSION_DESCRIPTION:
        void this.onDescription((f.d ?? {}) as SessionDescriptionData & { type?: string }).catch(
          (e) => {
            if (this.resolveConnect)
              this.failConnect(e instanceof Error ? e : new Error(String(e)));
          },
        );
        break;
      case VoiceOp.SPEAKING: {
        const d = f.d as SpeakingData | undefined;
        if (d?.user_id) this.events.speaking(d.user_id, (d.speaking & SPEAKING_VOICE) !== 0);
        break;
      }
      case VoiceOp.CLIENT_DISCONNECT: {
        const id = (f.d as { user_id?: string } | undefined)?.user_id;
        if (id) this.dropPeer(id);
        break;
      }
      default:
        break;
    }
  }

  private onSocketClose(code: number): void {
    clearInterval(this.heartbeat);
    this.ws = null;
    if (!this.params) return;
    if (FATAL_VOICE_CLOSES.has(code)) {
      const wasConnecting = !!this.rejectConnect;
      const err: VoiceError = { code: 'closed', message: `Voice session ended (${code}).` };
      if (wasConnecting) this.failConnect(new VoiceConnectError(err));
      else {
        this.teardown();
        this.events.fatal(err);
      }
      return;
    }
    if (this.socketRetries >= MAX_SOCKET_RETRIES) {
      this.teardown();
      this.events.fatal({ code: 'closed', message: 'Lost the voice connection.' });
      return;
    }
    this.setStatus('reconnecting');
    const delay = Math.min(8000, 500 * 2 ** this.socketRetries++);
    clearTimeout(this.retryTimer);
    this.retryTimer = setTimeout(() => this.openSocket(true), delay);
  }

  // ---- WebRTC ----

  private iceServers(extra?: ReadyData['ice_servers']): VoiceIceServer[] {
    return [...(this.params?.iceServers ?? []), ...(extra ?? [])];
  }

  private buildPeer(extra?: ReadyData['ice_servers']): PeerLike {
    const pc = this.deps.createPeer({ iceServers: this.iceServers(extra) });
    const out = this.mic.outStream;
    const track = this.mic.track;
    if (track && out) pc.addTransceiver(track, { direction: 'sendrecv', streams: [out] });
    pc.ontrack = (ev) => {
      const stream = ev.streams[0];
      if (!stream || ev.track.kind !== 'audio') return;
      // Convention: the SFU labels each forwarded stream with the sender's user id.
      this.addPeer(stream.id, stream);
    };
    pc.onconnectionstatechange = () => this.onPeerState(pc);
    return pc;
  }

  private async negotiate(restart: boolean, extraIce?: ReadyData['ice_servers']): Promise<void> {
    if (!this.pc) this.pc = this.buildPeer(extraIce);
    const pc = this.pc;
    this.pendingRestart = false;
    const offer = await pc.createOffer(restart ? { iceRestart: true } : undefined);
    await pc.setLocalDescription(offer);
    await this.gathered(pc);
    const sdp = tuneOpus(pc.localDescription?.sdp ?? offer.sdp ?? '');
    const sent = this.send(VoiceOp.SELECT_PROTOCOL, {
      protocol: 'webrtc',
      type: 'offer',
      sdp,
      data: sdp,
      codecs: [{ name: 'opus', type: 'audio', priority: 1000, payload_type: 111 }],
      ssrc: this.ssrc || undefined,
    });
    if (!sent) this.pendingRestart = true;
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

  private async onDescription(d: SessionDescriptionData & { type?: string }): Promise<void> {
    const pc = this.pc;
    if (!pc || !d.sdp) return;
    if (d.type === 'offer') {
      // The SFU renegotiates when streams are added or removed: answer on the same channel.
      await pc.setRemoteDescription({ type: 'offer', sdp: d.sdp });
      const answer = await pc.createAnswer();
      await pc.setLocalDescription(answer);
      this.send(VoiceOp.SELECT_PROTOCOL, {
        protocol: 'webrtc',
        type: 'answer',
        sdp: pc.localDescription?.sdp ?? answer.sdp,
      });
      return;
    }
    await pc.setRemoteDescription({ type: 'answer', sdp: d.sdp });
    if (pc.connectionState === 'connected') this.onPeerState(pc);
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
    void this.negotiate(true).catch(() => undefined);
  }

  private startQuality(): void {
    if (this.qualityTimer) return;
    const poll = () => {
      const pc = this.pc;
      if (!pc) return;
      pc.getStats()
        .then((r) => this.events.quality(qualityFromStats(r)))
        .catch(() => undefined);
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
    this.remoteStreams.delete(userId);
    this.playback.remove(userId);
    this.events.peer(userId, false);
  }

  // ---- controls ----

  setTransmitting(transmitting: boolean): void {
    const was = this.transmitting;
    this.transmitting = transmitting;
    this.mic.setGate(transmitting);
    if (transmitting !== was || !this.announced) {
      this.announced = true;
      this.send(VoiceOp.SPEAKING, {
        speaking: transmitting ? SPEAKING_VOICE : 0,
        delay: 0,
        ssrc: this.ssrc,
      });
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
  setInputVolume(percent: number): void {
    this.mic.setInputVolume(percent);
  }
  setOutputDevice(deviceId: string): Promise<void> {
    return this.playback.setOutputDevice(deviceId);
  }

  /** Swap the capture device mid-call: new track replaces the sender's, no renegotiation. */
  async setInputDevice(opts: MicOptions): Promise<void> {
    if (this.status === 'idle') return;
    const pc = this.pc as (PeerLike & { getSenders?: () => RTCRtpSender[] }) | null;
    await this.mic.start(opts);
    this.mic.setGate(this.transmitting);
    const track = this.mic.track;
    const sender = pc?.getSenders?.().find((s) => s.track?.kind === 'audio' || s.track === null);
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
    this.offLevel?.();
    this.offLost?.();
    this.offOnline?.();
    this.offLevel = this.offLost = this.offOnline = null;
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      ws.onmessage = ws.onclose = ws.onerror = null;
      try {
        ws.close(1000, 'client leaving');
      } catch {
        /* already closed */
      }
    }
    if (this.pc) {
      this.pc.onconnectionstatechange = this.pc.onicegatheringstatechange = this.pc.ontrack = null;
      this.pc.close();
      this.pc = null;
    }
    this.mic.stop();
    for (const id of [...this.remoteStreams.keys()]) this.dropPeer(id);
    this.playback.dispose();
    this.params = null;
    this.socketRetries = this.iceRestarts = 0;
    this.pendingRestart = false;
    this.announced = false;
    const rej = this.rejectConnect;
    this.resolveConnect = this.rejectConnect = null;
    rej?.(new Error('Voice connection closed'));
    this.setStatus('idle');
  }
}
