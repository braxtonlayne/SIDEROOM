import { asTickInput, MAX_TICKS, TICK_SOURCES, type CrashTick, type TickSource } from './allowList';

/**
 * The in-memory ring buffer of the last 60 deterministic ticks (OPS-CRASH-001). A tick is a time offset, a source
 * and an input from the closed enum of `allowList.ts`: an Activity bridge command, a game's button, an emulator
 * input. Never a payload. The buffer lives in memory only, is never persisted, and leaves the page only inside a
 * crash report after an unhandled failure.
 */
export class TickRing {
  private readonly slots: (CrashTick | undefined)[];
  private next = 0;
  private size = 0;

  constructor(
    private readonly capacity: number = MAX_TICKS,
    private readonly clock: () => number = () =>
      typeof performance !== 'undefined' ? Math.round(performance.now()) : 0,
  ) {
    this.slots = new Array<CrashTick | undefined>(capacity);
  }

  /** Record one tick. An unknown source is ignored; an input outside the enum is recorded as `other`. */
  record(src: TickSource, input: unknown): void {
    if (!TICK_SOURCES.includes(src)) return;
    this.slots[this.next] = { t: Math.max(0, this.clock()), src, input: asTickInput(input) };
    this.next = (this.next + 1) % this.capacity;
    this.size = Math.min(this.size + 1, this.capacity);
  }

  /** The recorded ticks, oldest first. */
  snapshot(): CrashTick[] {
    const out: CrashTick[] = [];
    const start = this.size < this.capacity ? 0 : this.next;
    for (let i = 0; i < this.size; i++) {
      const tick = this.slots[(start + i) % this.capacity];
      if (tick) out.push({ ...tick });
    }
    return out;
  }

  clear(): void {
    this.slots.fill(undefined);
    this.next = 0;
    this.size = 0;
  }
}

/** The page's ring: every recorder writes here and a crash report reads it. */
export const ticks = new TickRing();

/** Record a tick in the page's ring. */
export function recordTick(src: TickSource, input: unknown): void {
  ticks.record(src, input);
}

/** The name of an Activity bridge command as a tick input (`getChannel` becomes `bridge_get_channel`). */
export function bridgeInput(cmd: string): string {
  const snake = cmd.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
  const name = `bridge_${snake}`;
  const known: Record<string, string> = {
    bridge_get_instance_connected_participants: 'bridge_get_participants',
    bridge_open_external_link: 'bridge_open_link',
  };
  return known[name] ?? name;
}
