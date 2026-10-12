// FND-API-007: gateway client. Implements HELLO -> IDENTIFY/RESUME, heartbeat with ack timeout,
// RECONNECT / INVALID_SESSION handling, sequence tracking and jittered exponential backoff.
import type { ConnectionStatus } from '../store/types';
import {
  FATAL_CLOSE_CODES,
  type GatewayPayload,
  Intents,
  Op,
  SESSION_LOST_CLOSE_CODES,
} from './protocol';

export type GatewayHandler = (event: string, data: unknown, seq: number) => void;

/** The subset of the WebSocket API the client uses (so tests can inject a fake). */
export interface SocketLike {
  readonly readyState: number;
  binaryType: string;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: { code: number; reason?: string }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  send(data: string): void;
  close(code?: number, reason?: string): void;
}
export type SocketFactory = (url: string) => SocketLike;

export interface GatewayOptions {
  /** Gateway URL. Default: same-origin `/gateway?v=1&encoding=json`. */
  url?: string;
  /** IDENTIFY token. The web app authenticates by cookie, so the default is the empty string. */
  getToken?: () => string | Promise<string>;
  intents?: number;
  properties?: Record<string, unknown>;
  onDispatch: GatewayHandler;
  onStatus?: (status: ConnectionStatus) => void;
  /** Called on a close code that cannot be recovered by reconnecting (bad auth, bad intents). */
  onFatal?: (code: number) => void;
  /**
   * Decode a binary frame (zstd) to text. When provided, `&compress=zstd-stream` is requested.
   * No decoder ships by default: see docs/open-questions.md.
   */
  decompress?: (data: ArrayBuffer) => string;
  createSocket?: SocketFactory;
  random?: () => number;
  baseBackoffMs?: number;
  maxBackoffMs?: number;
}

const OPEN = 1;
const DEFAULT_INTENTS =
  Intents.GUILDS |
  Intents.GUILD_MEMBERS |
  Intents.GUILD_PRESENCES |
  Intents.GUILD_MESSAGES |
  Intents.GUILD_MESSAGE_TYPING |
  Intents.DIRECT_MESSAGES |
  Intents.DIRECT_MESSAGE_TYPING |
  Intents.MESSAGE_CONTENT;

function defaultUrl(): string {
  const loc = globalThis.location;
  const scheme = loc?.protocol === 'https:' ? 'wss' : 'ws';
  return `${scheme}://${loc?.host ?? 'localhost'}/gateway?v=1&encoding=json`;
}

export class GatewayClient {
  private ws: SocketLike | null = null;
  private attempt = 0;
  private seq: number | null = null;
  private sessionId: string | null = null;
  private resumeUrl: string | null = null;
  private stopped = true;
  private ackPending = false;
  private heartbeatTimer: ReturnType<typeof setTimeout> | undefined;
  private reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  private sessionTimer: ReturnType<typeof setTimeout> | undefined;
  private everConnected = false;
  private generation = 0;
  private readonly opts: GatewayOptions;

  constructor(opts: GatewayOptions) {
    this.opts = opts;
  }

  get lastSequence(): number {
    return this.seq ?? 0;
  }
  get session(): string | null {
    return this.sessionId;
  }

  /** Deterministic exponential cap (1 s doubling, 30 s max); `delayMs` adds jitter. */
  static backoffMs(attempt: number, base = 1000, max = 30_000): number {
    return Math.min(max, base * 2 ** attempt);
  }

  /** "Equal jitter": half the exponential delay plus a random share of the other half. */
  delayMs(attempt: number): number {
    const exp = GatewayClient.backoffMs(
      attempt,
      this.opts.baseBackoffMs ?? 1000,
      this.opts.maxBackoffMs ?? 30_000,
    );
    return Math.round(exp / 2 + (this.opts.random ?? Math.random)() * (exp / 2));
  }

  connect(): void {
    this.stopped = false;
    this.clearTimers();
    this.open();
  }

  /** Close for good (logout). Drops the session. */
  close(): void {
    this.stopped = true;
    this.clearTimers();
    this.generation++;
    const ws = this.ws;
    this.ws = null;
    this.sessionId = null;
    this.seq = null;
    this.setStatus('closed');
    if (ws) {
      detach(ws);
      ws.close(1000, 'client closing');
    }
  }

  /** Send any client op (PRESENCE_UPDATE, REQUEST_GUILD_MEMBERS, ...). False if not open. */
  send(op: number, d: unknown): boolean {
    if (!this.ws || this.ws.readyState !== OPEN) return false;
    this.ws.send(JSON.stringify({ op, d }));
    return true;
  }

  private setStatus(s: ConnectionStatus) {
    this.opts.onStatus?.(s);
  }

  private open(): void {
    const resuming = this.sessionId !== null && this.seq !== null;
    this.setStatus(this.everConnected || this.attempt > 0 ? 'reconnecting' : 'connecting');
    let url = (resuming && this.resumeUrl) || this.opts.url || defaultUrl();
    if (this.opts.decompress && !/[?&]compress=/.test(url)) {
      url += `${url.includes('?') ? '&' : '?'}compress=zstd-stream`;
    }
    const gen = ++this.generation;
    const ws = (this.opts.createSocket ?? ((u) => new WebSocket(u) as unknown as SocketLike))(url);
    ws.binaryType = 'arraybuffer';
    this.ws = ws;
    ws.onmessage = (m) => {
      if (gen === this.generation) void this.onFrame(m.data);
    };
    ws.onclose = (ev) => {
      if (gen === this.generation) this.onClose(ev.code);
    };
    ws.onerror = () => {
      /* always followed by close; handled there */
    };
  }

  private async onFrame(data: unknown): Promise<void> {
    let text: string;
    try {
      if (typeof data === 'string') text = data;
      else if (this.opts.decompress && data instanceof ArrayBuffer)
        text = this.opts.decompress(data);
      else throw new Error('binary frame without decoder');
    } catch {
      this.ws?.close(4002, 'decode error');
      return;
    }
    let p: GatewayPayload;
    try {
      p = JSON.parse(text) as GatewayPayload;
    } catch {
      return;
    }
    await this.handle(p);
  }

  private async handle(p: GatewayPayload): Promise<void> {
    switch (p.op) {
      case Op.HELLO: {
        const interval = (p.d as { heartbeat_interval: number }).heartbeat_interval;
        this.startHeartbeat(interval);
        await this.identifyOrResume();
        break;
      }
      case Op.HEARTBEAT:
        this.beat();
        break;
      case Op.HEARTBEAT_ACK:
        this.ackPending = false;
        break;
      case Op.RECONNECT:
        // Keep the session; reconnect immediately and RESUME.
        this.dropSocket(4000, 'reconnect requested');
        this.scheduleReconnect(0);
        break;
      case Op.INVALID_SESSION: {
        if (p.d !== true) {
          this.sessionId = null;
          this.seq = null;
        }
        // Discord asks for a random 1-5 s wait before retrying on the same socket.
        const wait = 1000 + (this.opts.random ?? Math.random)() * 4000;
        clearTimeout(this.sessionTimer);
        this.sessionTimer = setTimeout(() => void this.identifyOrResume(), wait);
        break;
      }
      case Op.DISPATCH:
        this.onDispatchFrame(p);
        break;
    }
  }

  private onDispatchFrame(p: GatewayPayload): void {
    const t = p.t;
    if (!t) return;
    if (typeof p.s === 'number') {
      if (this.seq !== null && p.s <= this.seq) return; // duplicate after resume
      this.seq = p.s;
    }
    if (t === 'READY') {
      const d = p.d as { session_id?: string; resume_gateway_url?: string };
      this.sessionId = d.session_id ?? null;
      this.resumeUrl = d.resume_gateway_url ?? null;
      this.markConnected();
    } else if (t === 'RESUMED') {
      this.markConnected();
    }
    this.opts.onDispatch(t, p.d, this.seq ?? 0);
  }

  private markConnected() {
    this.attempt = 0;
    this.everConnected = true;
    this.setStatus('connected');
  }

  private async identifyOrResume(): Promise<void> {
    const ws = this.ws;
    if (!ws || ws.readyState !== OPEN) return;
    const token = await (this.opts.getToken?.() ?? '');
    if (ws !== this.ws || ws.readyState !== OPEN) return;
    if (this.sessionId !== null && this.seq !== null) {
      this.send(Op.RESUME, { token, session_id: this.sessionId, seq: this.seq });
    } else {
      this.send(Op.IDENTIFY, {
        token,
        intents: this.opts.intents ?? DEFAULT_INTENTS,
        properties: this.opts.properties ?? { os: 'web', browser: 'sideroom', device: 'sideroom' },
      });
    }
  }

  private startHeartbeat(interval: number) {
    clearTimeout(this.heartbeatTimer);
    this.ackPending = false;
    const rnd = (this.opts.random ?? Math.random)();
    const tick = () => {
      if (this.ackPending) {
        // Zombie connection: no ack for a full interval. Reconnect and resume.
        this.dropSocket(4000, 'heartbeat ack timeout');
        this.scheduleReconnect(0);
        return;
      }
      this.beat();
      this.heartbeatTimer = setTimeout(tick, interval);
    };
    this.heartbeatTimer = setTimeout(tick, Math.floor(interval * rnd));
  }

  private beat() {
    this.ackPending = true;
    this.send(Op.HEARTBEAT, this.seq);
  }

  /** Detach and close the current socket without going through onClose. */
  private dropSocket(code: number, reason: string) {
    clearTimeout(this.heartbeatTimer);
    clearTimeout(this.sessionTimer);
    this.generation++;
    const ws = this.ws;
    this.ws = null;
    if (ws) {
      detach(ws);
      try {
        ws.close(code, reason);
      } catch {
        /* already closed */
      }
    }
  }

  private onClose(code: number) {
    clearTimeout(this.heartbeatTimer);
    clearTimeout(this.sessionTimer);
    this.ws = null;
    if (this.stopped) return;
    if (FATAL_CLOSE_CODES.has(code)) {
      this.stopped = true;
      this.sessionId = null;
      this.seq = null;
      this.setStatus('closed');
      this.opts.onFatal?.(code);
      return;
    }
    if (SESSION_LOST_CLOSE_CODES.has(code)) {
      this.sessionId = null;
      this.seq = null;
    }
    this.scheduleReconnect(this.delayMs(this.attempt++));
  }

  private scheduleReconnect(delay: number) {
    if (this.stopped) return;
    this.setStatus('reconnecting');
    clearTimeout(this.reconnectTimer);
    this.reconnectTimer = setTimeout(() => this.open(), delay);
  }

  private clearTimers() {
    clearTimeout(this.heartbeatTimer);
    clearTimeout(this.reconnectTimer);
    clearTimeout(this.sessionTimer);
  }
}

function detach(ws: SocketLike) {
  ws.onopen = ws.onmessage = ws.onclose = ws.onerror = null;
}
