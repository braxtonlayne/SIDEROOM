import { diffMasks } from './input';
import { COOP_VERSION, NO_SEAT, decode, encode, type CoopMessage } from './protocol';
import { SeatTable } from './seats';
import type { CoopTransport, HostableGame } from './transport';

export const ANNOUNCE_EVERY_MS = 2000;
export const WATCHDOG_EVERY_MS = 1000;
/** A held pad with no packet for this long is released (the guest's tab froze, or its keyup was lost). */
export const STUCK_PAD_MS = 3000;
/** A guest silent for this long loses the seat (guests ping every 2 s, so this is several missed pings). */
export const SEAT_IDLE_MS = 15000;

export interface HostSeat {
  userId: string;
  /** 0-based core port: 1..3 are players 2..4. */
  port: number;
  buttons: number;
}

export interface HostSnapshot {
  running: boolean;
  title: string;
  seats: HostSeat[];
}

interface SeatState {
  buttons: number;
  seq: number | null;
  lastPadAt: number;
  lastSeenAt: number;
}

export interface CoopHostOptions {
  transport: CoopTransport;
  game: HostableGame;
  /** The host's own user id (the guests' PAD packets name it as the recipient). */
  selfId: string;
  /** The host's measured ping to the SFU, announced so guests can show the whole path. */
  getPingMs?: () => number | null;
  now?: () => number;
}

/**
 * The host side of co-op play: runs on the machine that runs the core. Guests ask for a seat, the host hands
 * out ports 2 to 4 and turns each received pad into `core.setInput(port, button, value)` for that guest's port
 * only. There is no rollback and no lockstep: inputs are applied as they arrive.
 */
export class CoopHost {
  private readonly seats = new SeatTable();
  private readonly state = new Map<string, SeatState>();
  private readonly listeners = new Set<() => void>();
  private timers: ReturnType<typeof setInterval>[] = [];
  private off: (() => void) | null = null;
  private running = false;
  private cached: HostSnapshot;
  private readonly now: () => number;

  constructor(private readonly o: CoopHostOptions) {
    this.now = o.now ?? (() => Date.now());
    this.cached = this.build();
  }

  start(): void {
    if (this.running) return;
    this.running = true;
    this.o.transport.join();
    this.off = this.o.transport.onMessage((sender, bytes) => this.receive(sender, bytes));
    this.timers = [
      setInterval(() => this.announce(), ANNOUNCE_EVERY_MS),
      setInterval(() => this.watchdog(), WATCHDOG_EVERY_MS),
    ];
    this.announce();
    this.emit();
  }

  /** End the session: every guest's buttons are released, they are told, the data channel instance is left. */
  stop(): void {
    if (!this.running) return;
    this.running = false;
    this.timers.forEach(clearInterval);
    this.timers = [];
    this.off?.();
    this.off = null;
    for (const { userId } of this.seats.entries()) this.dropSeat(userId);
    void this.o.transport.send(encode({ type: 'close' }));
    this.o.transport.leave();
    this.emit();
  }

  kick(userId: string): void {
    if (this.seats.portOf(userId) === null) return;
    this.dropSeat(userId);
    void this.send({ type: 'kick', guest: userId });
    this.announce();
    this.emit();
  }

  snapshot(): HostSnapshot {
    return this.cached;
  }

  subscribe(cb: () => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  // ---- internals ----

  private build(): HostSnapshot {
    return {
      running: this.running,
      title: this.o.game.title,
      seats: this.seats
        .entries()
        .map((s) => ({ ...s, buttons: this.state.get(s.userId)?.buttons ?? 0 })),
    };
  }

  private emit(): void {
    this.cached = this.build();
    for (const l of [...this.listeners]) l();
  }

  private send(m: CoopMessage): Promise<boolean> {
    return this.o.transport.send(encode(m));
  }

  private announce(): void {
    if (!this.running) return;
    void this.send({
      type: 'announce',
      version: COOP_VERSION,
      free: this.seats.free,
      hostPingMs: this.o.getPingMs?.() ?? 0,
      title: this.o.game.title,
    });
  }

  private receive(sender: string, bytes: Uint8Array): void {
    if (!this.running || sender === this.o.selfId) return;
    const m = decode(bytes);
    if (!m) return;
    const at = this.now();
    switch (m.type) {
      case 'discover':
        this.announce();
        return;
      case 'join': {
        if (m.host !== this.o.selfId) return;
        const port = this.seats.assign(sender);
        if (port === null) {
          void this.send({ type: 'seat', guest: sender, port: NO_SEAT });
          return;
        }
        const st = this.state.get(sender);
        if (st) st.lastSeenAt = at;
        else this.state.set(sender, { buttons: 0, seq: null, lastPadAt: at, lastSeenAt: at });
        void this.send({ type: 'seat', guest: sender, port });
        this.announce();
        this.emit();
        return;
      }
      case 'leave':
        if (m.host !== this.o.selfId || this.seats.portOf(sender) === null) return;
        this.dropSeat(sender);
        this.announce();
        this.emit();
        return;
      case 'ping': {
        const st = this.state.get(sender);
        if (m.host !== this.o.selfId || !st) return;
        st.lastSeenAt = at;
        void this.send({ type: 'pong', guest: sender, t: m.t });
        return;
      }
      case 'pad': {
        // The port comes from the seat table, never from the packet: a guest cannot reach player 1.
        const port = this.seats.portOf(sender);
        const st = this.state.get(sender);
        if (m.host !== this.o.selfId || port === null || !st) return;
        if (st.seq !== null) {
          const ahead = (m.seq - st.seq) & 0xffff;
          if (ahead === 0 || ahead >= 0x8000) return; // a duplicate or an old packet
        }
        st.seq = m.seq;
        st.lastPadAt = st.lastSeenAt = at;
        this.apply(port, st, m.buttons);
        this.emit();
        return;
      }
      default:
        return; // ANNOUNCE, SEAT, PONG, CLOSE, KICK are not for a host
    }
  }

  private apply(port: number, st: SeatState, buttons: number): void {
    for (const [button, pressed] of diffMasks(st.buttons, buttons)) {
      this.o.game.setInput(port, button, pressed ? 1 : 0);
    }
    st.buttons = buttons & 0xffff;
  }

  private dropSeat(userId: string): void {
    const port = this.seats.release(userId);
    const st = this.state.get(userId);
    this.state.delete(userId);
    if (port !== null && st) this.apply(port, st, 0);
  }

  private watchdog(): void {
    const now = this.now();
    let changed = false;
    for (const { userId, port } of this.seats.entries()) {
      const st = this.state.get(userId);
      if (!st) continue;
      if (now - st.lastSeenAt > SEAT_IDLE_MS) {
        this.dropSeat(userId);
        void this.send({ type: 'kick', guest: userId });
        changed = true;
      } else if (st.buttons !== 0 && now - st.lastPadAt > STUCK_PAD_MS) {
        this.apply(port, st, 0);
        changed = true;
      }
    }
    if (changed) {
      this.announce();
      this.emit();
    }
  }
}
