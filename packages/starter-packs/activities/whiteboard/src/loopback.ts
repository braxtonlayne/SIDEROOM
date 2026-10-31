// State adapters that need no host: an in-memory hub (tests, and the stand-alone page with `?loopback`) and a
// BroadcastChannel one so two tabs of the dev server can draw together while the SDK relay is not there.
// Both keep the SDK's rules: last writer wins per key, `null` deletes, and only the host writes `host:*`.

import type { StateAdapter } from './sync';

type Listener = (changed: Record<string, unknown>) => void;

export class LoopbackHub {
  private readonly doc = new Map<string, unknown>();
  private readonly members = new Map<string, { host: boolean; listeners: Set<Listener> }>();
  private queue: Array<{ to: string; changed: Record<string, unknown> }> = [];
  /** When true, deliveries wait for `flush()` (and can be reordered). */
  manual: boolean;
  /** Writes refused because a non-host wrote a `host:*` key. */
  refused: string[] = [];

  constructor(opts: { manual?: boolean } = {}) {
    this.manual = opts.manual ?? false;
  }

  join(id: string, host: boolean): StateAdapter {
    const listeners = new Set<Listener>();
    this.members.set(id, { host, listeners });
    return {
      snapshot: () => Object.fromEntries(this.doc),
      set: (key, value) => this.write(id, key, value),
      onChange: (cb) => {
        listeners.add(cb);
        return () => listeners.delete(cb);
      },
    };
  }

  private write(from: string, key: string, value: unknown): void {
    const m = this.members.get(from);
    if (!m) return;
    if (key.startsWith('host:') && !m.host) {
      this.refused.push(key);
      return;
    }
    if (value === null || value === undefined) this.doc.delete(key);
    else this.doc.set(key, JSON.parse(JSON.stringify(value)));
    const changed = { [key]: value ?? null };
    for (const to of this.members.keys()) {
      if (to === from) continue; // the writer applies its own write locally, as the SDK does
      this.queue.push({ to, changed });
    }
    if (!this.manual) this.flush();
  }

  /** Deliver queued changes. `reverse` delivers newest first, to test out-of-order arrival. */
  flush(opts: { reverse?: boolean } = {}): void {
    const q = this.queue;
    this.queue = [];
    if (opts.reverse) q.reverse();
    for (const { to, changed } of q) {
      for (const cb of this.members.get(to)?.listeners ?? [])
        cb(JSON.parse(JSON.stringify(changed)));
    }
  }

  get keys(): string[] {
    return [...this.doc.keys()];
  }

  get(key: string): unknown {
    return this.doc.get(key);
  }
}

/** Two tabs of one origin, host chosen by the caller. Late joiners ask the others for the document. */
export function broadcastAdapter(room: string, host: boolean): StateAdapter {
  const doc = new Map<string, unknown>();
  const listeners = new Set<Listener>();
  const ch = new BroadcastChannel(`sideroom-whiteboard-${room}`);
  const notify = (changed: Record<string, unknown>) => {
    for (const cb of listeners) cb(changed);
  };
  const apply = (changed: Record<string, unknown>) => {
    for (const [k, v] of Object.entries(changed)) {
      if (v === null) doc.delete(k);
      else doc.set(k, v);
    }
  };
  ch.onmessage = (ev: MessageEvent) => {
    const m = ev.data as { t?: string; changed?: Record<string, unknown>; from?: boolean };
    if (m.t === 'hello') {
      ch.postMessage({ t: 'sync', changed: Object.fromEntries(doc) });
    } else if (m.t === 'sync' || m.t === 'set') {
      const changed = m.changed ?? {};
      // Host keys are accepted only from the host tab.
      const ok: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(changed)) {
        if (k.startsWith('host:') && m.t === 'set' && m.from !== true) continue;
        ok[k] = v;
      }
      apply(ok);
      notify(ok);
    }
  };
  ch.postMessage({ t: 'hello' });
  return {
    snapshot: () => Object.fromEntries(doc),
    set: (key, value) => {
      if (key.startsWith('host:') && !host) return;
      apply({ [key]: value ?? null });
      ch.postMessage({ t: 'set', changed: { [key]: value ?? null }, from: host });
    },
    onChange: (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}
