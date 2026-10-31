import { GuestControls } from './controls';
import { CoopGuest, type GuestSnapshot } from './guest';
import { CoopHost, type HostSnapshot } from './host';
import { dataPortTransport, type CoopTransport, type HostableGame } from './transport';

export interface CoopSnapshot {
  host: HostSnapshot | null;
  guest: GuestSnapshot | null;
  /** The guest's keys and pad are being read (the switch in the status chip). */
  controls: boolean;
}

const EMPTY: CoopSnapshot = { host: null, guest: null, controls: false };

/**
 * One co-op role per window: hosting a game or looking for / playing in somebody else's. The voice panel
 * reads it with `useCoop`; leaving the call tears it down.
 */
export class CoopSession {
  private host: CoopHost | null = null;
  private guest: CoopGuest | null = null;
  private controls: GuestControls | null = null;
  private controlsOn = false;
  private cached: CoopSnapshot = EMPTY;
  private readonly listeners = new Set<() => void>();
  private unsub: Array<() => void> = [];

  constructor(private makeTransport: () => CoopTransport = dataPortTransport) {}

  /** Test seam: where the next role gets its transport from. */
  setTransportFactory(make: () => CoopTransport): void {
    this.makeTransport = make;
  }

  subscribe = (cb: () => void): (() => void) => {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  };

  getSnapshot = (): CoopSnapshot => this.cached;

  startHosting(opts: {
    game: HostableGame;
    selfId: string;
    getPingMs?: () => number | null;
  }): void {
    this.stop();
    const host = new CoopHost({ transport: this.makeTransport(), ...opts });
    this.host = host;
    this.unsub = [host.subscribe(() => this.refresh())];
    host.start();
    this.refresh();
  }

  /** Look for hosts in the call (the dialog is open). Does nothing while hosting. */
  watch(selfId: string): void {
    if (this.host || this.guest) return;
    const guest = new CoopGuest({ transport: this.makeTransport(), selfId });
    this.guest = guest;
    this.unsub = [guest.subscribe(() => this.refresh())];
    guest.open();
    this.refresh();
  }

  join(hostId: string): void {
    this.guest?.join(hostId);
    this.setControls(true);
  }

  /** The dialog closed without a seat: stop looking. A seated guest keeps playing. */
  unwatch(): void {
    const g = this.guest;
    if (!g) return;
    const p = g.snapshot().phase;
    if (p === 'seated' || p === 'joining') return;
    this.stop();
  }

  setControls(on: boolean): void {
    const g = this.guest;
    if (on && g) {
      this.controls ??= new GuestControls(g);
      this.controls.attach();
      this.controlsOn = true;
    } else {
      this.controls?.detach();
      this.controlsOn = false;
    }
    this.refresh();
  }

  kick(userId: string): void {
    this.host?.kick(userId);
  }

  /** Leave a seat, stop hosting, stop looking. */
  stop(): void {
    this.controls?.detach();
    this.controls = null;
    this.controlsOn = false;
    this.unsub.forEach((u) => u());
    this.unsub = [];
    this.host?.stop();
    this.guest?.close();
    this.host = null;
    this.guest = null;
    this.refresh();
  }

  private refresh(): void {
    // A closed or kicked guest has nothing left to read the keys for.
    const phase = this.guest?.snapshot().phase;
    if (this.controlsOn && phase && phase !== 'seated' && phase !== 'joining') {
      this.controls?.detach();
      this.controlsOn = false;
    }
    this.cached =
      this.host || this.guest
        ? {
            host: this.host?.snapshot() ?? null,
            guest: this.guest?.snapshot() ?? null,
            controls: this.controlsOn,
          }
        : EMPTY;
    for (const l of [...this.listeners]) l();
  }
}

export const coopSession = new CoopSession();
