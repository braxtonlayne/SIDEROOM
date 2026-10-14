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
  tuneOpus,
  VoiceClose,
  VoiceOp,
  voiceUrl,
  type ReadyData,
  type SdpData,
  type QualityData,
  type SpeakingData,
} from './protocol';
import { qualityFromStats, type ConnectionQuality } from './quality';

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
}

export interface EngineEvents {
  status(s: EngineStatus): void;
  speaking(userId: string, speaking: boolean, priority: boolean): void;
  quality(q: ConnectionQuality): void;
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
  readonly signalingState: string;
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
    const pc = this.deps.createPeer({ iceServers: this.params?.iceServers ?? [] });
    const out = this.mic.outStream;
    const track = this.mic.track;
    if (track && out) pc.addTransceiver(track, { direction: 'sendonly', streams: [out] });
    pc.ontrack = (ev) => {
      const stream = ev.streams[0];
      if (!stream || ev.track.kind !== 'audio') return;
      // The SFU labels each forwarded line's msid stream with the speaker's user id (spec 4.4).
      this.addPeer(stream.id, stream);
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
    await pc.setLocalDescription({
      type: 'offer',
      sdp: tuneOpus(offer.sdp ?? '', this.params.maxBitrate),
    });
    await this.gathered(pc);
    const sdp = pc.localDescription?.sdp ?? offer.sdp ?? '';
    if (!this.send(VoiceOp.SDP_OFFER, { sdp })) {
      // Socket is down: the resume handler sends it when the socket is back.
      this.offerPending = false;
      this.reoffer = { restart };
      await pc.setLocalDescription({ type: 'rollback' });
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
    await pc.setLocalDescription(answer);
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
    for (const sender of pc.getSenders()) {
      if (sender.track?.kind !== 'audio') continue;
      const p = sender.getParameters();
      p.encodings = p.encodings?.length ? p.encodings : [{}];
      p.encodings[0]!.maxBitrate = max;
      void sender.setParameters(p).catch(() => undefined);
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
  setInputVolume(percent: number): void {
    this.mic.setInputVolume(percent);
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
      ws.onmessage = ws.onclose = ws.onerror = ws.onopen = null;
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
