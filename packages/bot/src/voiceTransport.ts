// Bots in voice, the media transport (RDY-DEV-001). The TypeScript twin of `sr_sdk::voice_transport`
// (crates/sr-sdk/src/voice_transport.rs): the same names, events and reconnect rules, with the WebRTC leg on
// `werift`, the pure-TypeScript WebRTC stack for Node.
//
// The flow, with the gateway side done by the caller through the client's gateway connection:
//
//  1. send gateway op 4 (`voiceStateBody`, or `E2eeCall.joinState` of ./voice which also declares `sr_e2ee`);
//  2. the gateway answers with a `VOICE_SERVER_UPDATE` dispatch: parse it with `VoiceServerUpdate.fromDispatch`;
//  3. `VoiceSession.connect` opens the SFU WebSocket (IDENTIFY with the join token), offers one send-only audio
//     line and answers the SFU's offers for everyone else's audio;
//  4. `VoiceSession.pump` drives the connection and returns `VoiceEvent`s (incoming Opus frames by user, joins,
//     leaves, speaking); `sendOpus` publishes one 20 ms frame.
//
// End-to-end encryption: when the dispatch says `sr_e2ee: true` the session refuses to send a frame unless a
// `FrameTransform` is installed (`E2eeMedia` of ./voice is one), so audio is never sent in the clear into an
// encrypted call, and incoming frames are dropped rather than played when they cannot be decrypted.
//
// A bare `VoiceSession` does not reconnect; `VoiceLink` does what a client does: when the signalling WebSocket
// drops while the media connection is up it re-attaches with the SFU's RESUME (`resumed`), when the SFU no
// longer knows the session it joins again with the same token and keeps the transform (`reconnected`), and it
// gives up, with the SFU's close code, on a removal (4014), a replacement (4016), a refusal or after
// `ReconnectPolicy.maxAttempts`.
import type { Closeable } from './audio';
import {
  RTCPeerConnection,
  RtpHeader,
  RtpPacket,
  useOPUS,
  type MediaStreamTrack,
  type RTCRtpSender,
} from 'werift';

/** SFU WebSocket opcodes (docs/specs/lounge-voice-video.md section 4). */
const OP = {
  IDENTIFY: 0,
  READY: 1,
  HEARTBEAT: 2,
  SDP_OFFER: 4,
  SDP_ANSWER: 5,
  SPEAKING: 7,
  RESUME: 8,
  RESUMED: 9,
  CLIENT_CONNECT: 10,
  CLIENT_DISCONNECT: 11,
} as const;

const SESSION_INVALID = 4006;

/**
 * Whether a close code (0: dropped without one) is worth a reconnect. Removal from the call, a replacement by
 * another device, a refused token and protocol errors are not: the caller has to decide what to do next.
 */
export function closeIsRetryable(code: number): boolean {
  return [0, 4006, 4008, 4009, 4015].includes(code);
}

export type TransportErrorKind =
  | 'bad-endpoint'
  | 'socket'
  | 'refused'
  | 'no-ready'
  | 'rtc'
  | 'not-connected'
  | 'encryption-required'
  | 'transform';

/** What went wrong in the transport. */
export class TransportError extends Error {
  constructor(
    readonly kind: TransportErrorKind,
    message: string,
    /** The SFU's close code, for `refused`. */
    readonly code = 0,
  ) {
    super(message);
    this.name = 'TransportError';
  }
}

/** Encrypts the bot's own frames and decrypts everyone else's. Both throw when the frame must be dropped. */
export interface FrameTransform {
  encrypt(frame: Uint8Array): Uint8Array;
  /** `sender` is the user id of the frame's author. */
  decrypt(sender: string, frame: Uint8Array): Uint8Array;
}

/**
 * The gateway op 4 `d` body that joins (or, with a null `channelId`, leaves) a voice channel. Declares
 * `sr_e2ee`, which is what a call that is end-to-end encrypted expects of every member.
 */
export function voiceStateBody(
  guildId: string | null,
  channelId: string | null,
  selfMute: boolean,
  selfDeaf: boolean,
  e2ee: boolean,
): Record<string, unknown> {
  const d: Record<string, unknown> = {
    channel_id: channelId,
    self_mute: selfMute,
    self_deaf: selfDeaf,
    sr_e2ee: e2ee,
  };
  if (guildId !== null) d.guild_id = guildId;
  return d;
}

/** The `VOICE_SERVER_UPDATE` dispatch: where the SFU is and the short-lived token that admits this session. */
export class VoiceServerUpdate {
  constructor(
    readonly token: string,
    /** WebSocket URL of the SFU node. */
    readonly endpoint: string,
    readonly guildId: string | null,
    readonly channelId: string,
    /**
     * The call is end-to-end encrypted (`sr_e2ee`). Absent counts as encrypted: voice E2EE is on by default
     * (D-249, D-263), and a bot must not fall back to plaintext because a field is missing.
     */
    readonly e2ee: boolean,
  ) {}

  /** Parse the dispatch's `d`. */
  static fromDispatch(d: Record<string, unknown>): VoiceServerUpdate | null {
    const s = (k: string) => (typeof d[k] === 'string' ? (d[k] as string) : null);
    const token = s('token');
    const endpoint = s('endpoint');
    const channelId = s('channel_id');
    if (token === null || endpoint === null || channelId === null) return null;
    return new VoiceServerUpdate(
      token,
      endpoint,
      s('guild_id'),
      channelId,
      typeof d.sr_e2ee === 'boolean' ? d.sr_e2ee : true,
    );
  }

  /** The join token never shows up in logs. */
  toJSON(): Record<string, unknown> {
    return { endpoint: this.endpoint, channelId: this.channelId, e2ee: this.e2ee };
  }
}

/** What a session reports. */
export type VoiceEvent =
  /** ICE and DTLS are up; audio can be sent. */
  | { kind: 'connected' }
  /** One Opus frame (already decrypted when a transform is installed). */
  | { kind: 'audio'; user: string; frame: Uint8Array }
  /** A frame that could not be decrypted was dropped. */
  | { kind: 'undecryptable'; user: string }
  | { kind: 'joined'; user: string }
  | { kind: 'left'; user: string }
  /** `speaking` is the bitmask of the voice protocol (1 microphone); 0 is silence. */
  | { kind: 'speaking'; user: string; speaking: number }
  /** The SFU closed the connection (code 0: dropped without a code). Do not keep pumping. */
  | { kind: 'closed'; code: number }
  /** `VoiceLink`: the signalling WebSocket was re-attached with RESUME; the media connection never went down. */
  | { kind: 'resumed'; participants: string[] }
  /** `VoiceLink`: the SFU had forgotten the session, so the link joined again (same frame transform). */
  | { kind: 'reconnected'; participants: string[] };

export interface ConnectOptions {
  /** The local address media is sent from (the ICE host candidate); use the interface the SFU can reach. */
  bindIp: string;
}

interface Waiter {
  resolve: () => void;
}

const userOf = (v: unknown): string | null => {
  const id = (v as { user_id?: unknown } | null)?.user_id;
  return typeof id === 'string' ? id : null;
};

const participantsOf = (ready: Record<string, unknown>): string[] =>
  Array.isArray(ready.participants)
    ? (ready.participants as unknown[]).map(userOf).filter((u): u is string => u !== null)
    : [];

/** Next text message of `ws`, or a refusal / failure; used for READY and RESUMED. */
function waitForOp(
  ws: WebSocket,
  wanted: number,
  timeoutMs: number,
): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cleanup();
      reject(new TransportError('no-ready', 'the SFU did not answer in time'));
    }, timeoutMs);
    const onMessage = (ev: MessageEvent) => {
      let v: { op?: number; d?: Record<string, unknown> };
      try {
        v = JSON.parse(String(ev.data)) as typeof v;
      } catch {
        return;
      }
      if (v.op === wanted) {
        cleanup();
        resolve(v.d ?? {});
      }
    };
    const onClose = (ev: CloseEvent) => {
      cleanup();
      reject(
        new TransportError('refused', `the SFU refused the join (close code ${ev.code})`, ev.code),
      );
    };
    const onError = () => {
      cleanup();
      reject(new TransportError('socket', 'voice websocket error'));
    };
    const cleanup = () => {
      clearTimeout(timer);
      ws.removeEventListener('message', onMessage);
      ws.removeEventListener('close', onClose);
      ws.removeEventListener('error', onError);
    };
    ws.addEventListener('message', onMessage);
    ws.addEventListener('close', onClose);
    ws.addEventListener('error', onError);
  });
}

function openSocket(endpoint: string): Promise<WebSocket> {
  if (!/^wss?:\/\//.test(endpoint)) {
    return Promise.reject(
      new TransportError('bad-endpoint', 'the voice endpoint is not a ws:// or wss:// URL'),
    );
  }
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(endpoint);
    ws.addEventListener('open', () => resolve(ws), { once: true });
    ws.addEventListener(
      'error',
      () => reject(new TransportError('socket', 'voice websocket error')),
      {
        once: true,
      },
    );
  });
}

/** One bot's connection to one voice channel. */
export class VoiceSession {
  private ws: WebSocket;
  private readonly pc: RTCPeerConnection;
  private readonly sender: RTCRtpSender;
  private events: VoiceEvent[] = [];
  private waiters: Waiter[] = [];
  private transform: FrameTransform | null = null;
  private codecs = new Set<Closeable>();
  private closedCode: number | null = null;
  private reportedUp = false;
  private heartbeat: ReturnType<typeof setInterval> | null = null;
  private beats = 0;
  private seq = 0;
  private readonly started = Date.now();
  private pendingOffer = false;
  /** Which user speaks on each of the SFU's audio lines, by mid (read from its offers). */
  private readonly userByMid = new Map<string, string>();
  private negotiating: Promise<void> = Promise.resolve();
  private sessionId: string;
  private resumeToken: string;
  /** Users who were in the channel when we joined (from READY). */
  initialParticipants: string[];

  private constructor(
    private readonly endpoint: string,
    ws: WebSocket,
    pc: RTCPeerConnection,
    sender: RTCRtpSender,
    private readonly e2ee: boolean,
    private readonly heartbeatMs: number,
    ready: Record<string, unknown>,
  ) {
    this.ws = ws;
    this.pc = pc;
    this.sender = sender;
    this.sessionId = typeof ready.session_id === 'string' ? ready.session_id : '';
    this.resumeToken = typeof ready.resume_token === 'string' ? ready.resume_token : '';
    this.initialParticipants = participantsOf(ready);
  }

  /**
   * Join: open the SFU WebSocket, IDENTIFY, wait for READY and send the offer. `bindIp` is the local address
   * media is sent from.
   */
  static async connect(update: VoiceServerUpdate, opts: ConnectOptions): Promise<VoiceSession> {
    const ws = await openSocket(update.endpoint);
    try {
      ws.send(JSON.stringify({ op: OP.IDENTIFY, d: { token: update.token } }));
      const ready = await waitForOp(ws, OP.READY, 10_000);
      const pc = new RTCPeerConnection({
        codecs: { audio: [useOPUS()] },
        iceInterfaceAddresses: { udp4: opts.bindIp },
        iceAdditionalHostAddresses: [opts.bindIp],
        iceUseIpv6: false,
        iceUseTcp: false,
      });
      const transceiver = pc.addTransceiver('audio', { direction: 'sendonly' });
      const heartbeatMs = Math.max(
        1000,
        Math.min(
          60_000,
          typeof ready.heartbeat_interval === 'number' ? ready.heartbeat_interval : 13_750,
        ),
      );
      const s = new VoiceSession(
        update.endpoint,
        ws,
        pc,
        transceiver.sender,
        update.e2ee,
        (heartbeatMs * 4) / 5,
        ready,
      );
      s.wire();
      await s.offer();
      return s;
    } catch (e) {
      ws.close();
      throw e;
    }
  }

  private wire(): void {
    this.attachSocket(this.ws);
    this.pc.connectionStateChange.subscribe((state) => {
      if (state === 'connected' && !this.reportedUp) {
        this.reportedUp = true;
        this.push({ kind: 'connected' });
      }
    });
    this.pc.onTrack.subscribe((track) => this.onTrack(track));
    this.heartbeat = setInterval(() => {
      this.beats += 1;
      this.sendWs({ op: OP.HEARTBEAT, d: { t: this.beats } });
    }, this.heartbeatMs);
    this.heartbeat.unref?.();
  }

  private attachSocket(ws: WebSocket): void {
    ws.addEventListener('message', (ev) => {
      if (ws === this.ws) void this.onWs(String(ev.data));
    });
    ws.addEventListener('close', (ev) => {
      if (ws === this.ws) this.markClosed(ev.code === 1005 || ev.code === 1006 ? 0 : ev.code);
    });
    ws.addEventListener('error', () => {
      if (ws === this.ws) this.markClosed(0);
    });
  }

  private markClosed(code: number): void {
    if (this.closedCode === null) {
      this.closedCode = code;
      this.wake();
    }
  }

  private async offer(): Promise<void> {
    const offer = await this.pc.createOffer();
    await this.pc.setLocalDescription(offer);
    this.pendingOffer = true;
    this.sendWs({ op: OP.SDP_OFFER, d: { sdp: this.pc.localDescription?.sdp ?? offer.sdp } });
  }

  private onTrack(track: MediaStreamTrack): void {
    // The SFU names each incoming audio line after its speaker's user id (the stream id of the line's msid in
    // its offer). werift does not copy that onto the track, so find the line by its transceiver and read the
    // id from the offer we saw.
    const line = this.pc.getTransceivers().find((t) => t.sender.trackId === track.id);
    const mid = line?.mid ?? null;
    const user = (mid !== null ? this.userByMid.get(mid) : undefined) ?? track.streamId ?? '';
    if (!/^\d+$/.test(user)) return;
    track.onReceiveRtp.subscribe((rtp: RtpPacket) => {
      const data = new Uint8Array(rtp.payload);
      const t = this.transform;
      if (t) {
        try {
          this.push({ kind: 'audio', user, frame: t.decrypt(user, data) });
        } catch {
          this.push({ kind: 'undecryptable', user });
        }
      } else if (this.e2ee) {
        // An encrypted call without a transform: never hand ciphertext to the application as audio.
        this.push({ kind: 'undecryptable', user });
      } else {
        this.push({ kind: 'audio', user, frame: data });
      }
    });
  }

  private onWs(raw: string): Promise<void> {
    let v: { op?: number; d?: Record<string, unknown> };
    try {
      v = JSON.parse(raw) as typeof v;
    } catch {
      return Promise.resolve();
    }
    const d = v.d ?? {};
    switch (v.op) {
      case OP.SDP_ANSWER:
        if (this.pendingOffer && typeof d.sdp === 'string') {
          this.pendingOffer = false;
          const sdp = d.sdp;
          this.negotiating = this.negotiating
            .then(() => this.pc.setRemoteDescription({ type: 'answer', sdp }))
            .catch(() => undefined);
        }
        return this.negotiating;
      case OP.SDP_OFFER:
        if (typeof d.sdp === 'string') {
          const sdp = d.sdp;
          this.noteLines(sdp);
          this.negotiating = this.negotiating
            .then(async () => {
              await this.pc.setRemoteDescription({ type: 'offer', sdp });
              const answer = await this.pc.createAnswer();
              await this.pc.setLocalDescription(answer);
              this.sendWs({
                op: OP.SDP_ANSWER,
                d: { sdp: this.pc.localDescription?.sdp ?? answer.sdp },
              });
            })
            .catch(() => undefined);
        }
        return this.negotiating;
      case OP.CLIENT_CONNECT: {
        const u = userOf(d);
        if (u) this.push({ kind: 'joined', user: u });
        break;
      }
      case OP.CLIENT_DISCONNECT: {
        const u = userOf(d);
        if (u) this.push({ kind: 'left', user: u });
        break;
      }
      case OP.SPEAKING: {
        const u = userOf(d);
        if (u && typeof d.speaking === 'number') {
          this.push({ kind: 'speaking', user: u, speaking: d.speaking });
        }
        break;
      }
    }
    return Promise.resolve();
  }

  /** Remember `a=msid:<user> <track>` of each media section of an SFU offer. */
  private noteLines(sdp: string): void {
    for (const section of sdp.split(/\r?\nm=/).slice(1)) {
      const mid = /a=mid:(\S+)/.exec(section)?.[1];
      const user = /a=msid:(\d+)\s/.exec(section)?.[1];
      if (mid !== undefined && user !== undefined) this.userByMid.set(mid, user);
    }
  }

  private sendWs(m: unknown): void {
    if (this.ws.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(m));
  }

  private push(e: VoiceEvent): void {
    this.events.push(e);
    this.wake();
  }

  private wake(): void {
    const w = this.waiters;
    this.waiters = [];
    for (const x of w) x.resolve();
  }

  /** Install the frame transform (end-to-end encryption). */
  setTransform(t: FrameTransform | null): void {
    this.transform = t;
  }

  /**
   * Hand an encoder or decoder (`./audio`) to the session: `leave()` closes it, which gives back its
   * WebAssembly memory (DEV-FU-002). A `VoiceLink` rejoin moves it to the new session.
   */
  adopt<T extends Closeable>(codec: T): T {
    this.codecs.add(codec);
    return codec;
  }

  takeCodecs(): Set<Closeable> {
    const c = this.codecs;
    this.codecs = new Set();
    return c;
  }

  takeTransform(): FrameTransform | null {
    const t = this.transform;
    this.transform = null;
    return t;
  }

  isConnected(): boolean {
    return this.pc.connectionState === 'connected';
  }

  /**
   * Publish one Opus frame (20 ms). Unlike the Rust SDK there is no audio level argument: this transport does
   * not negotiate the audio-level header extension, so the speaking indicator follows `setSpeaking`. In an
   * encrypted call this fails without a transform.
   */
  async sendOpus(frame: Uint8Array): Promise<void> {
    if (!this.isConnected())
      throw new TransportError('not-connected', 'the connection is not up yet');
    let payload: Uint8Array;
    if (this.transform) {
      try {
        payload = this.transform.encrypt(frame);
      } catch (e) {
        throw new TransportError('transform', e instanceof Error ? e.message : String(e));
      }
    } else if (this.e2ee) {
      throw new TransportError(
        'encryption-required',
        'this call is end-to-end encrypted: install a frame transform before sending audio',
      );
    } else {
      payload = frame;
    }
    this.seq = (this.seq + 1) & 0xffff;
    const timestamp = Math.floor(((Date.now() - this.started) * 48) % 0x1_0000_0000);
    const header = new RtpHeader({
      payloadType: 111,
      sequenceNumber: this.seq,
      timestamp,
      ssrc: 0,
    });
    await this.sender.sendRtp(new RtpPacket(header, Buffer.from(payload)));
  }

  /** Tell the SFU (and through it the channel) that the bot starts or stops speaking. */
  setSpeaking(speaking: boolean): void {
    this.sendWs({ op: OP.SPEAKING, d: { speaking: speaking ? 1 : 0 } });
  }

  /**
   * Wait for up to `maxMs` and return what happened. Returns early as soon as there is at least one event.
   */
  async pump(maxMs: number): Promise<VoiceEvent[]> {
    if (this.events.length === 0 && this.closedCode === null) {
      await new Promise<void>((resolve) => {
        const timer = setTimeout(resolve, maxMs);
        this.waiters.push({
          resolve: () => {
            clearTimeout(timer);
            resolve();
          },
        });
      });
    }
    const out = this.events;
    this.events = [];
    if (this.closedCode !== null) {
      out.push({ kind: 'closed', code: this.closedCode });
      this.closedCode = null;
    }
    return out;
  }

  /**
   * Re-attach the signalling WebSocket with the SFU's RESUME. The media connection is untouched, so audio keeps
   * flowing. Fails with `refused` (4006) when the session is gone (the resume window passed): join again.
   */
  async resume(): Promise<string[]> {
    if (!this.sessionId || !this.resumeToken) {
      throw new TransportError('refused', 'no session to resume', SESSION_INVALID);
    }
    const ws = await openSocket(this.endpoint);
    try {
      ws.send(
        JSON.stringify({
          op: OP.RESUME,
          d: { session_id: this.sessionId, resume_token: this.resumeToken },
        }),
      );
      const ready = await waitForOp(ws, OP.RESUMED, 10_000);
      if (typeof ready.resume_token === 'string') this.resumeToken = ready.resume_token;
      const old = this.ws;
      this.ws = ws;
      this.attachSocket(ws);
      old.close();
      this.closedCode = null;
      return participantsOf(ready);
    } catch (e) {
      ws.close();
      throw e;
    }
  }

  /**
   * Drop the signalling WebSocket without leaving, as a network loss would. The media connection stays up.
   * Meant for tests of reconnect logic and for callers that detect a dead network.
   */
  abandonSignalling(): void {
    this.ws.close();
    this.markClosed(0);
  }

  /** Leave: close the WebSocket and the media connection. Send gateway op 4 with a null channel too. */
  async leave(): Promise<void> {
    if (this.heartbeat) clearInterval(this.heartbeat);
    this.ws.close();
    for (const c of this.takeCodecs()) c.close();
    await this.pc.close();
  }
}

/** How a `VoiceLink` retries. */
export interface ReconnectPolicy {
  /** Attempts (a resume or a join each count one) before the link reports `closed`. */
  maxAttempts: number;
  /** Wait before the first attempt; doubles per attempt up to `maxBackoffMs`. */
  minBackoffMs: number;
  maxBackoffMs: number;
}

export const defaultReconnectPolicy: ReconnectPolicy = {
  maxAttempts: 6,
  minBackoffMs: 500,
  maxBackoffMs: 8000,
};

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** A `VoiceSession` that comes back by itself; see the module comment for the rules. */
export class VoiceLink {
  private constructor(
    private current: VoiceSession,
    private update: VoiceServerUpdate,
    private readonly opts: ConnectOptions,
    private readonly policy: ReconnectPolicy,
  ) {}

  /** Join like `VoiceSession.connect` and keep the update for later rejoins. */
  static async connect(
    update: VoiceServerUpdate,
    opts: ConnectOptions,
    policy: ReconnectPolicy = defaultReconnectPolicy,
  ): Promise<VoiceLink> {
    return new VoiceLink(await VoiceSession.connect(update, opts), update, opts, policy);
  }

  /** The live session: `sendOpus`, `setSpeaking`, `setTransform`. A rejoin replaces it. */
  session(): VoiceSession {
    return this.current;
  }

  /**
   * Join again with a fresh `VOICE_SERVER_UPDATE` (after `closed` with a refused token, or because the gateway
   * moved the call to another node), keeping the frame transform.
   */
  async rejoin(update: VoiceServerUpdate): Promise<void> {
    const fresh = await VoiceSession.connect(update, this.opts);
    fresh.setTransform(this.current.takeTransform());
    for (const c of this.current.takeCodecs()) fresh.adopt(c);
    void this.current.leave();
    this.current = fresh;
    this.update = update;
  }

  /**
   * `VoiceSession.pump` with reconnects: a lost or retryable close never reaches the caller; it shows up as
   * `resumed` or `reconnected`. The call can take longer than `maxMs` while it backs off between attempts.
   */
  async pump(maxMs: number): Promise<VoiceEvent[]> {
    const events = await this.current.pump(maxMs);
    const i = events.findIndex((e) => e.kind === 'closed');
    if (i < 0) return events;
    const closed = events[i] as { kind: 'closed'; code: number };
    events.splice(i, 1);
    events.push(...(await this.recover(closed.code)));
    return events;
  }

  private async recover(firstCode: number): Promise<VoiceEvent[]> {
    let code = firstCode;
    if (!closeIsRetryable(code)) return [{ kind: 'closed', code }];
    let backoff = this.policy.minBackoffMs;
    for (let attempt = 0; attempt < this.policy.maxAttempts; attempt++) {
      await sleep(backoff);
      backoff = Math.min(backoff * 2, this.policy.maxBackoffMs);
      // The media connection is up: re-attach only the signalling. Otherwise (or when the SFU forgot us)
      // there is nothing to resume.
      let rejoin = !this.current.isConnected();
      if (!rejoin) {
        try {
          return [{ kind: 'resumed', participants: await this.current.resume() }];
        } catch (e) {
          if (e instanceof TransportError && e.kind === 'refused') {
            if (e.code === SESSION_INVALID) rejoin = true;
            else if (!closeIsRetryable(e.code)) return [{ kind: 'closed', code: e.code }];
            else {
              code = e.code;
              continue;
            }
          } else {
            continue;
          }
        }
      }
      if (rejoin) {
        try {
          const fresh = await VoiceSession.connect(this.update, this.opts);
          fresh.setTransform(this.current.takeTransform());
          for (const c of this.current.takeCodecs()) fresh.adopt(c);
          void this.current.leave();
          this.current = fresh;
          return [{ kind: 'reconnected', participants: fresh.initialParticipants }];
        } catch (e) {
          // An expired token, a removal or a full SFU: the caller has to act.
          if (e instanceof TransportError && e.kind === 'refused' && !closeIsRetryable(e.code)) {
            return [{ kind: 'closed', code: e.code }];
          }
        }
      }
    }
    return [{ kind: 'closed', code }];
  }
}
