import { GUEST_PORTS } from './protocol';

/**
 * Who plays which port. The port is decided here, by the host, from the sender the SFU authenticated; a guest's
 * packets never name a port, so no guest can type into player 1 or into another guest's seat.
 */
export class SeatTable {
  private readonly byUser = new Map<string, number>();

  /** The seat of `userId`: the one they already hold, else the lowest free one; `null` when all are taken. */
  assign(userId: string): number | null {
    const have = this.byUser.get(userId);
    if (have !== undefined) return have;
    const taken = new Set(this.byUser.values());
    const port = GUEST_PORTS.find((p) => !taken.has(p));
    if (port === undefined) return null;
    this.byUser.set(userId, port);
    return port;
  }

  portOf(userId: string): number | null {
    return this.byUser.get(userId) ?? null;
  }

  release(userId: string): number | null {
    const port = this.byUser.get(userId) ?? null;
    this.byUser.delete(userId);
    return port;
  }

  get free(): number {
    return GUEST_PORTS.length - this.byUser.size;
  }

  entries(): Array<{ userId: string; port: number }> {
    return [...this.byUser]
      .map(([userId, port]) => ({ userId, port }))
      .sort((a, b) => a.port - b.port);
  }
}
