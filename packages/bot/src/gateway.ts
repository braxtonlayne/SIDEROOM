/** One gateway shard: HELLO, IDENTIFY/RESUME, heartbeats, reconnect with backoff. */

export interface Socket {
  send(data: string): void;
  close(code?: number): void;
  onMessage(cb: (data: string) => void): void;
  /** Called once when the socket ends, with the close code if there was one. */
  onClose(cb: (code: number | undefined) => void): void;
}

export type SocketFactory = (url: string) => Socket;

/** The default factory: the global `WebSocket` (Node 22+, browsers, Deno, Bun). */
export const webSocketFactory: SocketFactory = (rawUrl) => {
  const url = rawUrl.includes('?') ? rawUrl : `${rawUrl.replace(/\/+$/, '')}?v=10&encoding=json`;
  const ws = new WebSocket(url);
  return {
    send: (d) => ws.send(d),
    close: (c) => ws.close(c),
    onMessage: (cb) =>
      ws.addEventListener('message', (e) => {
        if (typeof e.data === 'string') cb(e.data);
      }),
    onClose: (cb) => {
      let done = false;
      const fire = (code: number | undefined) => {
        if (!done) {
          done = true;
          cb(code);
        }
      };
      ws.addEventListener('close', (e) => fire(e.code));
      ws.addEventListener('error', () => fire(undefined));
    },
  };
};

const OP = {
  DISPATCH: 0,
  HEARTBEAT: 1,
  IDENTIFY: 2,
  PRESENCE_UPDATE: 3,
  VOICE_STATE_UPDATE: 4,
  RESUME: 6,
  RECONNECT: 7,
  REQUEST_GUILD_MEMBERS: 8,
  INVALID_SESSION: 9,
  HELLO: 10,
  HEARTBEAT_ACK: 11,
  E2EE_MLS: 101,
} as const;

/** Close codes a reconnect cannot cure (bad token, bad shard, bad or disallowed intents). */
export const GatewayOp = OP;

export const FATAL_CLOSE: Readonly<Record<number, string>> = {
  4004: 'authentication failed',
  4010: 'invalid shard',
  4011: 'sharding required',
  4012: 'invalid API version',
  4013: 'invalid intents',
  4014: 'disallowed intents',
};

/** The lower-case host of a URL, or undefined when it does not parse. */
export function urlHost(url: string): string | undefined {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return undefined;
  }
}

export interface ShardOptions {
  token: string;
  intents: number;
  /** `[shard id, shard count]` */
  shard: [number, number];
  url: string;
  createSocket?: SocketFactory;
  onDispatch: (shard: number, name: string, data: unknown) => void;
  onFatal: (code: number, reason: string) => void;
  minBackoffMs?: number;
  maxBackoffMs?: number;
  /** Hosts a resume URL may point at (D-234); the host of `url` is always allowed. */
  allowedHosts?: string[];
}

export class Shard {
  private socket: Socket | undefined;
  private sessionId: string | undefined;
  private resumeUrl: string | undefined;
  private seq: number | null = null;
  /**
   * The last op 4 body that joined a voice channel as sent (DEV-FU-001, U-06 (a)). The server forgets voice state
   * with a session that cannot be resumed, so it is sent again after the fresh READY that follows; a leave
   * (null `channel_id`) clears it. Same as the Rust shard (`crates/sr-sdk/src/gateway.rs`).
   */
  private voiceJoin: string | undefined;
  private acked = true;
  private heartbeat: ReturnType<typeof setInterval> | undefined;
  private retry: ReturnType<typeof setTimeout> | undefined;
  private backoff: number;
  private stopped = false;
  private readonly createSocket: SocketFactory;

  constructor(private readonly o: ShardOptions) {
    this.createSocket = o.createSocket ?? webSocketFactory;
    this.backoff = o.minBackoffMs ?? 1000;
  }

  connect(): void {
    if (this.stopped) return;
    const url =
      this.sessionId && this.resumeUrl && this.hostAllowed(this.resumeUrl)
        ? this.resumeUrl
        : this.o.url;
    const socket = this.createSocket(url);
    this.socket = socket;
    socket.onMessage((raw) => {
      if (this.socket === socket) this.handle(socket, raw);
    });
    socket.onClose((code) => {
      if (this.socket === socket) this.closed(code);
    });
  }

  private hostAllowed(url: string): boolean {
    const h = urlHost(url);
    if (h === undefined) return false;
    const allowed = [urlHost(this.o.url), ...(this.o.allowedHosts ?? [])];
    return allowed.some((a) => a !== undefined && a.toLowerCase() === h);
  }

  get id(): number {
    return this.o.shard[0];
  }

  /** Send an op (for example REQUEST_GUILD_MEMBERS) on the open connection. False if not open. */
  send(op: number, d: unknown): boolean {
    if (!this.socket || !this.sessionId) return false;
    const frame = JSON.stringify({ op, d });
    this.socket.send(frame);
    if (op === OP.VOICE_STATE_UPDATE) {
      const channel = (d as { channel_id?: unknown } | null | undefined)?.channel_id;
      this.voiceJoin = channel === null || channel === undefined ? undefined : frame;
    }
    return true;
  }

  destroy(): void {
    this.stopped = true;
    this.clearTimers();
    this.socket?.close(1000);
    this.socket = undefined;
  }

  private clearTimers(): void {
    if (this.heartbeat) clearInterval(this.heartbeat);
    if (this.retry) clearTimeout(this.retry);
    this.heartbeat = undefined;
    this.retry = undefined;
  }

  private handle(socket: Socket, raw: string): void {
    let p: { op: number; d?: unknown; s?: number | null; t?: string | null };
    try {
      p = JSON.parse(raw) as typeof p;
    } catch {
      return;
    }
    switch (p.op) {
      case OP.HELLO: {
        const ms =
          (p.d as { heartbeat_interval?: number } | undefined)?.heartbeat_interval ?? 41250;
        this.acked = true;
        this.heartbeat = setInterval(
          () => {
            if (!this.acked) {
              this.reconnect(true);
              return;
            }
            this.acked = false;
            socket.send(JSON.stringify({ op: OP.HEARTBEAT, d: this.seq }));
          },
          Math.max(1, ms),
        );
        if (this.sessionId) {
          socket.send(
            JSON.stringify({
              op: OP.RESUME,
              d: { token: this.o.token, session_id: this.sessionId, seq: this.seq ?? 0 },
            }),
          );
        } else {
          socket.send(
            JSON.stringify({
              op: OP.IDENTIFY,
              d: {
                token: this.o.token,
                intents: this.o.intents,
                shard: this.o.shard,
                properties: {
                  os: process.platform,
                  browser: 'sideroom-bot',
                  device: 'sideroom-bot',
                },
              },
            }),
          );
        }
        return;
      }
      case OP.DISPATCH: {
        if (typeof p.s === 'number') this.seq = p.s;
        const name = p.t ?? '';
        if (name === 'READY') {
          const d = p.d as { session_id?: string; resume_gateway_url?: string };
          this.sessionId = d.session_id;
          this.resumeUrl = d.resume_gateway_url;
          this.backoff = this.o.minBackoffMs ?? 1000;
        }
        this.o.onDispatch(this.o.shard[0], name, p.d);
        // READY is only ever the answer to an IDENTIFY (a RESUME is answered by RESUMED), so this is a session
        // without the old voice state: join the channel again.
        if (name === 'READY' && this.voiceJoin !== undefined && this.socket === socket) {
          socket.send(this.voiceJoin);
        }
        return;
      }
      case OP.HEARTBEAT:
        socket.send(JSON.stringify({ op: OP.HEARTBEAT, d: this.seq }));
        return;
      case OP.HEARTBEAT_ACK:
        this.acked = true;
        return;
      case OP.RECONNECT:
        this.reconnect(true);
        return;
      case OP.INVALID_SESSION:
        this.reconnect(p.d === true);
        return;
    }
  }

  private closed(code: number | undefined): void {
    const fatal = code === undefined ? undefined : FATAL_CLOSE[code];
    if (fatal !== undefined && code !== undefined) {
      this.stopped = true;
      this.clearTimers();
      this.socket = undefined;
      this.o.onFatal(code, fatal);
      return;
    }
    this.reconnect(!(code === 4007 || code === 4009), false);
  }

  /** Drop the connection and open a new one after the backoff. */
  private reconnect(resume: boolean, closeSocket = true): void {
    if (this.stopped) return;
    const old = this.socket;
    this.socket = undefined;
    this.clearTimers();
    if (closeSocket) old?.close(4000);
    if (!resume) {
      this.sessionId = undefined;
      this.seq = null;
    }
    const wait = this.backoff;
    this.backoff = Math.min(this.backoff * 2, this.o.maxBackoffMs ?? 60_000);
    this.retry = setTimeout(() => this.connect(), wait);
  }
}
