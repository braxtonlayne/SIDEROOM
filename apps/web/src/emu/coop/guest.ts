import { NO_SEAT, SEAT_NOT_ALLOWED, decode, encode, type CoopMessage } from './protocol';
import type { CoopTransport } from './transport';

export const PING_EVERY_MS = 2000;
/** While a button is held the whole pad is re-sent this often, so the host's watchdog does not release it. */
export const KEEPALIVE_MS = 1000;
/** No ANNOUNCE for this long and a host is forgotten. */
export const HOST_STALE_MS = 7000;

export interface HostInfo {
  userId: string;
  title: string;
  free: number;
  hostPingMs: number;
  seenAt: number;
}

export type GuestPhase = 'idle' | 'joining' | 'seated' | 'full' | 'denied' | 'closed' | 'kicked';

export interface GuestSnapshot {
  phase: GuestPhase;
  hosts: HostInfo[];
  hostId: string | null;
  /** 0-based core port; 1..3 are players 2..4. */
  port: number | null;
  /** Round trip guest -> SFU -> host -> SFU -> guest of the last ping answers (smoothed), ms. */
  rttMs: number | null;
}

export interface CoopGuestOptions {
  transport: CoopTransport;
  selfId: string;
  /** Monotonic ms (performance.now); the PING timestamp is its low 32 bits. */
  now?: () => number;
}

/**
 * The guest side: finds hosts in the call, asks one for a seat, then sends the whole pad on every change.
 * Latency shown to the player is the measured round trip to the host over the same encrypted data channel;
 * the picture they play on is the host's screen share, so the real delay is this plus the video's own.
 */
export class CoopGuest {
  private hosts = new Map<string, HostInfo>();
  private phase: GuestPhase = 'idle';
  private hostId: string | null = null;
  private port: number | null = null;
  private rttMs: number | null = null;
  private seq = 0;
  private buttons = 0;
  private listening = false;
  private off: (() => void) | null = null;
  private timers: ReturnType<typeof setInterval>[] = [];
  private readonly listeners = new Set<() => void>();
  private cached: GuestSnapshot;
  private readonly now: () => number;

  constructor(private readonly o: CoopGuestOptions) {
    this.now = o.now ?? (() => performance.now());
    this.cached = this.build();
  }

  /** Start listening for hosts (and ask the ones already here to announce themselves). */
  open(): void {
    if (this.listening) return;
    this.listening = true;
    this.o.transport.join();
    this.off = this.o.transport.onMessage((sender, bytes) => this.receive(sender, bytes));
    this.timers = [setInterval(() => this.tick(), 1000)];
    void this.send({ type: 'discover' });
    this.emit();
  }

  /** Stop: leaves the seat if one is held and the data channel instance. */
  close(): void {
    if (!this.listening) return;
    this.leave();
    this.timers.forEach(clearInterval);
    this.timers = [];
    this.off?.();
    this.off = null;
    this.o.transport.leave();
    this.listening = false;
    this.hosts.clear();
    this.emit();
  }

  join(hostId: string): void {
    if (!this.listening) return;
    if (this.hostId && this.hostId !== hostId) this.leave();
    this.hostId = hostId;
    this.phase = 'joining';
    void this.send({ type: 'join', host: hostId });
    this.emit();
  }

  leave(): void {
    if (this.hostId && (this.phase === 'seated' || this.phase === 'joining')) {
      void this.send({ type: 'leave', host: this.hostId });
    }
    this.reset('idle');
    this.emit();
  }

  /** The whole pad, as the player holds it now. Sent only while seated, and only when it changed. */
  setButtons(mask: number): void {
    const next = mask & 0xffff;
    if (next === this.buttons) return;
    this.buttons = next;
    this.sendPad();
  }

  snapshot(): GuestSnapshot {
    return this.cached;
  }

  subscribe(cb: () => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  // ---- internals ----

  private build(): GuestSnapshot {
    return {
      phase: this.phase,
      hosts: [...this.hosts.values()],
      hostId: this.hostId,
      port: this.port,
      rttMs: this.rttMs === null ? null : Math.round(this.rttMs),
    };
  }

  private emit(): void {
    this.cached = this.build();
    for (const l of [...this.listeners]) l();
  }

  private send(m: CoopMessage): Promise<boolean> {
    return this.o.transport.send(encode(m));
  }

  private sendPad(): void {
    if (this.phase !== 'seated' || !this.hostId) return;
    this.seq = (this.seq + 1) & 0xffff;
    void this.send({ type: 'pad', host: this.hostId, seq: this.seq, buttons: this.buttons });
    this.lastPadAt = this.now();
  }

  private lastPadAt = 0;
  private lastPingAt = 0;

  private reset(phase: GuestPhase): void {
    this.phase = phase;
    if (phase === 'idle') this.hostId = null;
    this.port = null;
    this.rttMs = null;
    this.buttons = 0;
  }

  private tick(): void {
    const t = this.now();
    let changed = false;
    for (const [id, h] of this.hosts) {
      if (t - h.seenAt > HOST_STALE_MS) {
        this.hosts.delete(id);
        changed = true;
      }
    }
    if (this.phase === 'seated' && this.hostId) {
      if (this.buttons !== 0 && t - this.lastPadAt >= KEEPALIVE_MS) this.sendPad();
      if (t - this.lastPingAt >= PING_EVERY_MS) {
        this.lastPingAt = t;
        void this.send({ type: 'ping', host: this.hostId, t: Math.floor(t) >>> 0 });
      }
    }
    if (changed) this.emit();
  }

  private receive(sender: string, bytes: Uint8Array): void {
    if (!this.listening || sender === this.o.selfId) return;
    const m = decode(bytes);
    if (!m) return;
    switch (m.type) {
      case 'announce':
        this.hosts.set(sender, {
          userId: sender,
          title: m.title,
          free: m.free,
          hostPingMs: m.hostPingMs,
          seenAt: this.now(),
        });
        this.emit();
        return;
      case 'seat':
        // Only the host we asked, and only a seat meant for us.
        if (m.guest !== this.o.selfId || sender !== this.hostId || this.phase !== 'joining') return;
        if (m.port === NO_SEAT) {
          this.reset('full');
        } else if (m.port === SEAT_NOT_ALLOWED) {
          this.reset('denied');
        } else if (m.port < 1 || m.port > 3) {
          return; // not a guest port
        } else {
          this.phase = 'seated';
          this.port = m.port;
          this.lastPingAt = this.now() - PING_EVERY_MS; // ping at the next tick
          this.seq = 0;
          this.buttons = 0;
        }
        this.emit();
        return;
      case 'pong': {
        if (m.guest !== this.o.selfId || sender !== this.hostId || this.phase !== 'seated') return;
        const rtt = ((Math.floor(this.now()) >>> 0) - m.t) >>> 0;
        if (rtt > 60_000) return; // a stale or forged timestamp
        this.rttMs = this.rttMs === null ? rtt : this.rttMs * 0.7 + rtt * 0.3;
        this.emit();
        return;
      }
      case 'kick':
        if (m.guest !== this.o.selfId || sender !== this.hostId) return;
        this.reset('kicked');
        this.emit();
        return;
      case 'close':
        this.hosts.delete(sender);
        if (sender === this.hostId) this.reset('closed');
        this.emit();
        return;
      default:
        return; // DISCOVER, JOIN, PAD, LEAVE, PING are for hosts
    }
  }
}
