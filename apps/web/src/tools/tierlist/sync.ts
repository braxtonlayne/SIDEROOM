import { StaleRoom, type RoomApi, type ToolRoom, type ToolRoomEvent } from './roomApi';

/** The latest keys of a room as this client sees them. */
export type RoomView = { title: string; state: Record<string, unknown>; version: number };

export type SyncStatus = 'live' | 'saving' | 'offline' | 'gone';

interface Layer {
  set: Record<string, unknown>;
  title?: string;
}

/**
 * The client half of the version rule (UTL-COL-001). The server holds the room and a version; an edit sets
 * whole keys against the version the client last saw. Here:
 *
 *  - an edit applies locally at once (`pending`), and is sent as one patch at a time (`inflight`);
 *  - the confirmed room moves on the server's answer or on a `TOOL_ROOM_UPDATE` that is the next version;
 *  - a gap in versions refetches; a 409 swaps in the current room and re-sends our keys on top of it, so
 *    the last writer wins per key and nobody's other keys are lost.
 *
 * The view is the confirmed room with our in-flight and pending keys laid over it.
 */
export class RoomSync {
  private confirmed: ToolRoom;
  private inflight: Layer | null = null;
  private pending: Layer = { set: {} };
  private busy = false;
  private status: SyncStatus = 'live';
  private listeners = new Set<() => void>();
  private cache: RoomView | null = null;

  constructor(
    room: ToolRoom,
    private readonly api: RoomApi,
  ) {
    this.confirmed = room;
  }

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => {
      this.listeners.delete(l);
    };
  };

  private emit(): void {
    this.cache = null;
    for (const l of [...this.listeners]) l();
  }

  getStatus = (): SyncStatus => this.status;

  /** Stable between changes (for `useSyncExternalStore`). */
  getView = (): RoomView => {
    if (this.cache) return this.cache;
    const state = { ...this.confirmed.state };
    let title = this.confirmed.title;
    for (const layer of [this.inflight, this.pending]) {
      if (!layer) continue;
      for (const [k, v] of Object.entries(layer.set)) {
        if (v === null) delete state[k];
        else state[k] = v;
      }
      if (layer.title !== undefined) title = layer.title;
    }
    this.cache = { title, state, version: this.confirmed.version };
    return this.cache;
  };

  get room(): ToolRoom {
    return this.confirmed;
  }

  /** Whether edits are still waiting to reach the server. */
  get dirty(): boolean {
    return (
      this.inflight !== null ||
      Object.keys(this.pending.set).length > 0 ||
      this.pending.title !== undefined
    );
  }

  /** Set (or with `null` remove) keys; a title renames the room. */
  edit(set: Record<string, unknown>, title?: string): Promise<void> {
    Object.assign(this.pending.set, set);
    if (title !== undefined) this.pending.title = title;
    this.emit();
    return this.flush();
  }

  /** A `TOOL_ROOM_UPDATE` for this room. */
  async receive(ev: ToolRoomEvent): Promise<void> {
    if (ev.deleted) {
      this.status = 'gone';
      this.emit();
      return;
    }
    if (ev.version <= this.confirmed.version) return; // our own echo, or already known
    if (ev.version === this.confirmed.version + 1 && ev.set) {
      const state = { ...this.confirmed.state };
      for (const [k, v] of Object.entries(ev.set)) {
        if (v === null) delete state[k];
        else state[k] = v;
      }
      this.confirmed = { ...this.confirmed, state, title: ev.title, version: ev.version };
      this.emit();
      return;
    }
    await this.refetch();
  }

  async refetch(): Promise<void> {
    try {
      const room = await this.api.get(this.confirmed.channel_id, this.confirmed.id);
      if (room.version >= this.confirmed.version) this.confirmed = room;
      if (this.status === 'offline') this.status = 'live';
    } catch {
      this.status = 'offline';
    }
    this.emit();
  }

  /** Send pending edits, one patch at a time. Resolves when nothing is left to send (or a send failed). */
  async flush(): Promise<void> {
    if (this.busy || this.isGone()) return;
    this.busy = true;
    try {
      while (!this.isGone()) {
        if (!this.inflight) {
          if (!this.dirty) break;
          this.inflight = this.pending;
          this.pending = { set: {} };
        }
        this.status = 'saving';
        this.emit();
        const sending = this.inflight;
        try {
          const room = await this.api.patch(this.confirmed.channel_id, this.confirmed.id, {
            base_version: this.confirmed.version,
            set: sending.set,
            ...(sending.title !== undefined ? { title: sending.title } : {}),
          });
          if (room.version >= this.confirmed.version) this.confirmed = room;
          this.inflight = null;
        } catch (e) {
          if (e instanceof StaleRoom) {
            // Last writer wins per key: take the current room and send our keys again on top of it.
            if (e.current.version >= this.confirmed.version) this.confirmed = e.current;
            continue;
          }
          // Keep the edits visible and retryable: fold them back so the next flush resends them.
          const title = this.pending.title ?? sending.title;
          this.pending = {
            set: { ...sending.set, ...this.pending.set },
            ...(title !== undefined ? { title } : {}),
          };
          this.inflight = null;
          this.status = 'offline';
          this.emit();
          return;
        }
        this.status = this.dirty ? 'saving' : 'live';
        this.emit();
      }
    } finally {
      this.busy = false;
    }
  }

  // A method, so the compiler does not narrow `status` across the awaits of a flush.
  private isGone(): boolean {
    return this.status === 'gone';
  }

  /** Try again after an offline stretch. */
  retry(): void {
    if (this.status === 'offline') this.status = 'live';
    void this.refetch().then(() => this.flush());
  }
}
