// The board's replication layer (ACT-WB-001). It sits on a `StateAdapter`, the small slice of the Embedded App
// SDK's `sdk.state` the whiteboard uses: a flat JSON document, last-writer-wins per key, where only the host may
// write `host:*` keys.
//
// Keys:
//   op:<author>:<counter>        one log entry, written once by its author
//   host:base:meta               { gen, chunks, through }   the compaction snapshot's header (host only)
//   host:base:<gen>:<i>          the snapshot's entries, split into chunks (host only)
//
// A reader's board is the snapshot's entries plus every `op:` entry the snapshot does not already cover
// (`through[author]` is the highest counter folded in). The host compacts: it folds the covered entries,
// writes the new snapshot chunks, then the header, then deletes the folded `op:` keys and the old chunks.
// A client never forgets an entry because its key disappeared; it drops entries only when a loaded snapshot
// covers them, so out-of-order delivery of the deletions cannot make strokes vanish.

import {
  compareEntries,
  compactEntries,
  entryId,
  fold,
  parseEntry,
  redoTarget,
  undoTarget,
  type Entry,
  type Payload,
  type View,
} from './log';

export interface StateAdapter {
  /** Every key currently in the shared document. */
  snapshot(): Record<string, unknown>;
  /** Write a key. `null` deletes it. */
  set(key: string, value: unknown): void;
  /** Called with the keys that changed (value `null` for a deleted key). Returns an unsubscribe function. */
  onChange(cb: (changed: Record<string, unknown>) => void): () => void;
}

export const OP_PREFIX = 'op:';
export const BASE_META = 'host:base:meta';
export const BASE_CHUNK_PREFIX = 'host:base:';

export interface BoardOptions {
  /** The local member's id. */
  me: string;
  /** Whether this member launched the activity: only the host compacts. */
  isHost: boolean;
  adapter: StateAdapter;
  /** Compact when this many live `op:` entries have piled up (host). */
  compactAt?: number;
  /** Do not fold entries first seen less than this long ago (host); keeps a fresh undo working. */
  minAgeMs?: number;
  /** Wait this long after a change before a compaction runs (host). 0 disables the timer. */
  compactDelayMs?: number;
  /** Largest serialized size of one snapshot chunk. */
  chunkChars?: number;
  now?: () => number;
}

interface BaseMeta {
  gen: number;
  chunks: number;
  through: Record<string, number>;
}

const opKey = (a: string, n: number): string => `${OP_PREFIX}${a}:${n}`;
const chunkKey = (gen: number, i: number): string => `${BASE_CHUNK_PREFIX}${gen}:${i}`;

function parseMeta(v: unknown): BaseMeta | null {
  if (typeof v !== 'object' || v === null) return null;
  const r = v as Record<string, unknown>;
  const gen = r.gen;
  const chunks = r.chunks;
  if (typeof gen !== 'number' || !Number.isInteger(gen) || gen < 1) return null;
  if (typeof chunks !== 'number' || !Number.isInteger(chunks) || chunks < 0 || chunks > 10_000) {
    return null;
  }
  const through: Record<string, number> = {};
  if (typeof r.through === 'object' && r.through !== null) {
    for (const [a, n] of Object.entries(r.through as Record<string, unknown>)) {
      if (typeof n === 'number' && Number.isInteger(n) && n >= 0) through[a] = n;
    }
  }
  return { gen, chunks, through };
}

export class Board {
  readonly me: string;
  readonly isHost: boolean;
  private readonly adapter: StateAdapter;
  private readonly opts: Required<Pick<BoardOptions, 'compactAt' | 'minAgeMs' | 'chunkChars'>> & {
    compactDelayMs: number;
  };
  private readonly now: () => number;
  private readonly live = new Map<string, Entry>();
  private readonly firstSeen = new Map<string, number>();
  private baseEntries: Entry[] = [];
  private baseGen = 0;
  private through: Record<string, number> = {};
  private pendingMeta: BaseMeta | null = null;
  private readonly chunks = new Map<string, Entry[]>();
  private clock = 0;
  private counter = 0;
  private cached: View | null = null;
  private readonly listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private unsub: () => void;
  private closed = false;
  /** How many compactions this board ran (host) or loaded (everyone). */
  compactions = 0;

  constructor(o: BoardOptions) {
    this.me = o.me;
    this.isHost = o.isHost;
    this.adapter = o.adapter;
    this.now = o.now ?? Date.now;
    this.opts = {
      compactAt: o.compactAt ?? 150,
      minAgeMs: o.minAgeMs ?? 20_000,
      chunkChars: o.chunkChars ?? 30_000,
      compactDelayMs: o.compactDelayMs ?? 2_000,
    };
    this.unsub = this.adapter.onChange((c) => this.ingest(c));
    this.ingest(this.adapter.snapshot());
  }

  close(): void {
    this.closed = true;
    this.unsub();
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.listeners.clear();
  }

  subscribe(cb: () => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  /** The visible board. Cached until the log changes. */
  view(): View {
    if (!this.cached) this.cached = fold(this.allEntries()).view;
    return this.cached;
  }

  /** Entries in force: the snapshot's plus the live ones it does not cover. */
  allEntries(): Entry[] {
    const out = new Map<string, Entry>();
    for (const e of this.baseEntries) out.set(entryId(e), e);
    for (const e of this.live.values()) {
      if (e.n <= (this.through[e.a] ?? 0)) continue;
      out.set(entryId(e), e);
    }
    return [...out.values()];
  }

  /** Number of live `op:` entries not yet folded into a snapshot. */
  liveCount(): number {
    let n = 0;
    for (const e of this.live.values()) if (e.n > (this.through[e.a] ?? 0)) n++;
    return n;
  }

  // --- local actions ---------------------------------------------------------------------------------------

  /** Append an entry authored here. Returns its id. */
  add(p: Payload): string {
    this.counter += 1;
    this.clock += 1;
    const e: Entry = { a: this.me, n: this.counter, ts: this.clock, p };
    const key = opKey(e.a, e.n);
    this.adapter.set(key, e);
    this.ingest({ [key]: e });
    return entryId(e);
  }

  /** Undo the author's latest drawn object. Returns false when there is nothing to undo. */
  undo(): boolean {
    const t = undoTarget(this.allEntries(), this.me);
    if (!t) return false;
    this.add({ k: 'undo', t });
    return true;
  }

  redo(): boolean {
    const t = redoTarget(this.allEntries(), this.me);
    if (!t) return false;
    this.add({ k: 'redo', t });
    return true;
  }

  // --- replication -----------------------------------------------------------------------------------------

  private ingest(changed: Record<string, unknown>): void {
    if (this.closed) return;
    let dirty = false;
    let baseTouched = false;
    for (const [key, value] of Object.entries(changed)) {
      if (key.startsWith(OP_PREFIX)) {
        if (value === null || value === undefined) continue; // deletions never drop an entry
        const e = parseEntry(value);
        if (!e || opKey(e.a, e.n) !== key) continue;
        const id = entryId(e);
        if (this.live.has(id)) continue;
        this.live.set(id, e);
        this.firstSeen.set(id, this.now());
        this.clock = Math.max(this.clock, e.ts);
        if (e.a === this.me) this.counter = Math.max(this.counter, e.n);
        dirty = true;
      } else if (key === BASE_META) {
        const m = parseMeta(value);
        if (m && (!this.pendingMeta || m.gen >= this.pendingMeta.gen)) {
          this.pendingMeta = m;
          baseTouched = true;
        }
      } else if (key.startsWith(BASE_CHUNK_PREFIX)) {
        if (Array.isArray(value)) {
          const list: Entry[] = [];
          for (const raw of value) {
            const e = parseEntry(raw);
            if (e) list.push(e);
          }
          this.chunks.set(key, list);
          baseTouched = true;
        }
      }
    }
    if (baseTouched && this.tryLoadBase()) dirty = true;
    if (dirty) {
      this.cached = null;
      for (const cb of [...this.listeners]) cb();
      this.scheduleCompaction();
    }
  }

  private tryLoadBase(): boolean {
    const m = this.pendingMeta;
    if (!m || m.gen <= this.baseGen) return false;
    const entries: Entry[] = [];
    for (let i = 0; i < m.chunks; i++) {
      const c = this.chunks.get(chunkKey(m.gen, i));
      if (!c) return false; // not all here yet; keep showing the previous snapshot
      entries.push(...c);
    }
    this.baseEntries = entries;
    this.baseGen = m.gen;
    this.through = m.through;
    this.compactions += 1;
    // Everything the snapshot covers can go.
    for (const [id, e] of this.live) {
      if (e.n <= (m.through[e.a] ?? 0)) {
        this.live.delete(id);
        this.firstSeen.delete(id);
      }
    }
    for (const key of [...this.chunks.keys()]) {
      if (!key.startsWith(`${BASE_CHUNK_PREFIX}${m.gen}:`)) this.chunks.delete(key);
    }
    for (const e of entries) {
      this.clock = Math.max(this.clock, e.ts);
      if (e.a === this.me) this.counter = Math.max(this.counter, e.n);
    }
    this.counter = Math.max(this.counter, m.through[this.me] ?? 0);
    return true;
  }

  // --- compaction (host) -----------------------------------------------------------------------------------

  private scheduleCompaction(): void {
    if (!this.isHost || this.opts.compactDelayMs <= 0 || this.timer) return;
    if (this.liveCount() < this.opts.compactAt) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      if (!this.closed) this.compact();
    }, this.opts.compactDelayMs);
  }

  /**
   * Fold the log into a new snapshot (host only). Returns the number of live entries folded, 0 when nothing
   * was done. Only a contiguous run of each author's counters is folded, and nothing younger than `minAgeMs`.
   */
  compact(): number {
    if (!this.isHost || this.closed) return 0;
    const t = this.now();
    const through: Record<string, number> = { ...this.through };
    const byAuthor = new Map<string, Map<number, Entry>>();
    for (const e of this.live.values()) {
      let m = byAuthor.get(e.a);
      if (!m) byAuthor.set(e.a, (m = new Map()));
      m.set(e.n, e);
    }
    let folded = 0;
    const foldedEntries: Entry[] = [];
    for (const [a, m] of byAuthor) {
      let n = through[a] ?? 0;
      for (;;) {
        const next = m.get(n + 1);
        if (!next) break;
        const seen = this.firstSeen.get(entryId(next)) ?? t;
        if (t - seen < this.opts.minAgeMs) break;
        n += 1;
        foldedEntries.push(next);
        folded += 1;
      }
      through[a] = n;
    }
    if (folded === 0) return 0;

    const kept = compactEntries([...this.baseEntries, ...foldedEntries]);
    const gen = Math.max(this.baseGen, this.pendingMeta?.gen ?? 0) + 1;

    const chunks: Entry[][] = [];
    let cur: Entry[] = [];
    let size = 0;
    for (const e of kept.sort(compareEntries)) {
      const len = JSON.stringify(e).length;
      if (cur.length > 0 && size + len > this.opts.chunkChars) {
        chunks.push(cur);
        cur = [];
        size = 0;
      }
      cur.push(e);
      size += len;
    }
    if (cur.length > 0) chunks.push(cur);

    const writes: Record<string, unknown> = {};
    chunks.forEach((c, i) => {
      writes[chunkKey(gen, i)] = c;
    });
    const meta: BaseMeta = { gen, chunks: chunks.length, through };
    // Chunks first, then the header that makes them current, then the clean-up.
    for (const [k, v] of Object.entries(writes)) this.adapter.set(k, v);
    this.adapter.set(BASE_META, meta);
    this.ingest({ ...writes, [BASE_META]: meta });
    for (const e of foldedEntries) this.adapter.set(opKey(e.a, e.n), null);
    const prefix = `${BASE_CHUNK_PREFIX}${gen}:`;
    for (const key of Object.keys(this.adapter.snapshot())) {
      if (key.startsWith(BASE_CHUNK_PREFIX) && key !== BASE_META && !key.startsWith(prefix)) {
        this.adapter.set(key, null);
      }
    }
    return folded;
  }
}
