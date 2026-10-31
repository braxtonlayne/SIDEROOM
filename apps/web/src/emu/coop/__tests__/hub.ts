import type { CoopCore, CoopTransport } from '../transport';

/** A relay like the SFU's: broadcast to the other joined members, with the sender's id attached. */
export class Hub {
  private readonly members = new Map<
    string,
    { joined: boolean; cbs: Set<(sender: string, b: Uint8Array) => void> }
  >();
  /** Everything that crossed the relay, for assertions: [sender, bytes]. */
  readonly wire: Array<[string, Uint8Array]> = [];

  transport(id: string): CoopTransport {
    const me = { joined: false, cbs: new Set<(s: string, b: Uint8Array) => void>() };
    this.members.set(id, me);
    return {
      join: () => {
        me.joined = true;
      },
      leave: () => {
        me.joined = false;
      },
      send: (bytes) => {
        if (!me.joined) return Promise.resolve(false);
        const copy = bytes.slice();
        this.wire.push([id, copy]);
        for (const [other, m] of this.members) {
          if (other === id || !m.joined) continue;
          for (const cb of [...m.cbs]) cb(id, copy.slice());
        }
        return Promise.resolve(true);
      },
      onMessage: (cb) => {
        me.cbs.add(cb);
        return () => me.cbs.delete(cb);
      },
    };
  }
}

/** A core that records what it is told. */
export class RecordingCore implements CoopCore {
  readonly calls: Array<[port: number, button: number, value: number]> = [];
  readonly down = new Map<number, Set<number>>();
  setInput(port: number, button: number, value: 0 | 1): void {
    this.calls.push([port, button, value]);
    const set = this.down.get(port) ?? new Set<number>();
    if (value) set.add(button);
    else set.delete(button);
    this.down.set(port, set);
  }
}
