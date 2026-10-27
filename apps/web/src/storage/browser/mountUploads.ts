// Resumable uploads into a bucket mount (BYOS-BRW-001). The shape follows review/uploadManager.ts (parts of
// 16 MiB, a few at once, a failed part retried with a pause, an IndexedDB row that keeps the file so a refresh
// continues) and reuses its constants, task states and the part `PUT`. What differs: the server hands out the
// part URL one part at a time (an hour each, so a long upload never holds an expired one), a folder of files is
// queued and only a couple of files are open at the server at once (it allows 50 per member), and the server
// completes the object itself from the parts the bucket holds.
import { ApiError } from '../../api/client';
import {
  defaultLedger,
  MemoryLedger,
  type Ledger,
  type MountUploadRow,
} from '../../lib/uploadLedger';
import {
  PART_CONCURRENCY,
  PART_RETRIES,
  putPartWithFetch,
  type TaskState,
} from '../../review/uploadManager';
import * as api from '../api';

/** Files in flight at the server at once. */
export const FILE_CONCURRENCY = 2;

export interface MountUploadApi {
  begin(
    mountId: string,
    b: { key: string; size: number; content_type?: string },
  ): Promise<api.UploadSlot>;
  status(mountId: string, uploadId: string): Promise<api.UploadStatus>;
  partUrl(mountId: string, uploadId: string, part: number): Promise<api.PartUrl>;
  complete(mountId: string, uploadId: string): Promise<unknown>;
  abort(mountId: string, uploadId: string): Promise<unknown>;
}

export const realMountUploadApi: MountUploadApi = {
  begin: api.beginUpload,
  status: api.uploadStatus,
  partUrl: api.partUrl,
  complete: api.completeUpload,
  abort: api.abortUpload,
};

export interface MountUploadOptions {
  concurrency?: number;
  files?: number;
  retries?: number;
  /** Sends one part; rejects on any failure. */
  put?: (url: string, body: Blob, signal: AbortSignal) => Promise<void>;
  /** Pause between attempts (tests make it instant). */
  wait?: (attempt: number) => Promise<void>;
  now?: () => number;
}

export interface MountUploadTask {
  /** Stable for the life of the task (a local id until the server has answered, then the upload id). */
  id: string;
  mountId: string;
  key: string;
  filename: string;
  size: number;
  state: TaskState;
  /** 0 to 1. */
  progress: number;
  error?: string;
  /** The server's code when it refused (`CORS_NOT_READY`, `EMPTY_FILE`, ...), for the screen's wording. */
  code?: string;
}

interface Live {
  id: string;
  mountId: string;
  key: string;
  file: File | Blob;
  /** Set once the server has the upload. */
  row: MountUploadRow | null;
  state: TaskState;
  error?: string;
  code?: string;
  abort: AbortController;
  settle?: (ok: boolean) => void;
}

const defaultWait = (attempt: number) =>
  new Promise<void>((r) => setTimeout(r, 500 * 2 ** attempt));

/** The error body's stable code: `errors.mount._errors[0].code` for 409s, else the field errors, else `code`. */
export function serverCode(e: unknown): string | undefined {
  if (!(e instanceof ApiError)) return undefined;
  const nested = Object.values(e.body?.errors ?? {})[0]?._errors?.[0]?.code;
  if (nested) return nested;
  return typeof e.body?.code === 'string' ? e.body.code : undefined;
}

let localCounter = 0;

export class MountUploadManager {
  private live = new Map<string, Live>();
  private order: string[] = [];
  private subs = new Set<() => void>();
  private cache: MountUploadTask[] = [];
  constructor(
    private server: MountUploadApi,
    private ledger: Ledger<MountUploadRow>,
    private o: MountUploadOptions = {},
  ) {}

  subscribe(fn: () => void): () => void {
    this.subs.add(fn);
    return () => this.subs.delete(fn);
  }

  /** Stable between changes, so it can back `useSyncExternalStore`. */
  snapshot(): MountUploadTask[] {
    return this.cache;
  }

  /** Overall progress of the unfinished uploads, 0 to 1. */
  total(): number {
    const open = this.cache.filter((t) => t.state !== 'done');
    const size = open.reduce((n, t) => n + t.size, 0);
    return size === 0 ? 1 : open.reduce((n, t) => n + t.progress * t.size, 0) / size;
  }

  private emit() {
    this.cache = this.order
      .map((id) => this.live.get(id))
      .filter((l): l is Live => !!l)
      .map((l) => ({
        id: l.id,
        mountId: l.mountId,
        key: l.key,
        filename: l.key.split('/').pop() ?? l.key,
        size: l.file.size,
        state: l.state,
        progress: l.state === 'done' ? 1 : this.bytes(l) / Math.max(1, l.file.size),
        ...(l.error ? { error: l.error } : {}),
        ...(l.code ? { code: l.code } : {}),
      }));
    for (const f of this.subs) f();
  }

  private bytes(l: Live): number {
    const r = l.row;
    if (!r) return 0;
    return r.done.reduce(
      (n, p) => n + (p === r.partCount ? r.size - r.partSize * (r.partCount - 1) : r.partSize),
      0,
    );
  }

  /**
   * Queue `file` to be uploaded as `key` in the mount. Resolves when the object exists; rejects when the upload
   * fails or is cancelled. The server hears of it when its turn comes, so a folder of hundreds of files never
   * holds hundreds of open uploads.
   */
  enqueue(mountId: string, key: string, file: File | Blob): Promise<void> {
    const id = `local-${++localCounter}`;
    const live: Live = {
      id,
      mountId,
      key,
      file,
      row: null,
      state: 'queued',
      abort: new AbortController(),
    };
    this.live.set(id, live);
    this.order.push(id);
    const done = new Promise<void>((resolve, reject) => {
      live.settle = (ok) => (ok ? resolve() : reject(new Error(live.error ?? live.state)));
    });
    done.catch(() => undefined);
    this.emit();
    this.pump();
    return done;
  }

  /** After a refresh: continue what the ledger remembers for this mount (paused uploads stay paused). */
  async resumeAll(mountId?: string): Promise<void> {
    for (const row of await this.ledger.all()) {
      if (row.state === 'done' || this.live.has(row.id)) continue;
      if (mountId !== undefined && row.mountId !== mountId) continue;
      const live: Live = {
        id: row.id,
        mountId: row.mountId,
        key: row.key,
        file: row.file,
        row,
        state: row.state === 'paused' ? 'paused' : 'queued',
        abort: new AbortController(),
      };
      this.live.set(row.id, live);
      this.order.push(row.id);
    }
    this.emit();
    this.pump();
  }

  private running(): number {
    return [...this.live.values()].filter((l) => l.state === 'running').length;
  }

  private pump() {
    for (const id of this.order) {
      if (this.running() >= (this.o.files ?? FILE_CONCURRENCY)) return;
      const l = this.live.get(id);
      if (l?.state === 'queued') void this.run(l);
    }
  }

  private async persist(l: Live, patch: Partial<MountUploadRow> = {}) {
    if (l.row) {
      l.row = { ...l.row, ...patch, state: l.state, savedAt: (this.o.now ?? Date.now)() };
      await this.ledger.put(l.row);
    }
    this.emit();
  }

  private fail(l: Live, e: unknown, fallback: string) {
    l.state = 'failed';
    l.code = serverCode(e);
    l.error = e instanceof Error && !(e instanceof ApiError) ? e.message : fallback;
    if (e instanceof ApiError) l.error = e.message || fallback;
  }

  private async run(l: Live): Promise<void> {
    l.state = 'running';
    l.error = undefined;
    l.code = undefined;
    this.emit();
    try {
      let done: number[] = [];
      if (!l.row) {
        // First turn: the server hears of the file now.
        const slot = await this.server.begin(l.mountId, {
          key: l.key,
          size: l.file.size,
          ...(l.file.type ? { content_type: l.file.type } : {}),
        });
        // The task keeps its place under the server's id, so a reload finds the same row.
        const old = l.id;
        l.id = slot.id;
        this.live.delete(old);
        this.live.set(l.id, l);
        this.order = this.order.map((x) => (x === old ? l.id : x));
        l.row = {
          id: slot.id,
          mountId: l.mountId,
          key: l.key,
          size: slot.size,
          partSize: slot.part_size,
          partCount: slot.part_count,
          done: [],
          state: 'running',
          savedAt: (this.o.now ?? Date.now)(),
          file: l.file,
        };
        await this.ledger.put(l.row);
        this.emit();
      } else {
        // A resumed upload asks the server what arrived; a upload it no longer knows is over.
        const st = await this.server.status(l.mountId, l.row.id);
        done = st.parts.map((p) => p.number);
        l.row = { ...l.row, done };
        await this.persist(l);
      }
      const row = l.row;
      const todo = Array.from({ length: row.partCount }, (_, i) => i + 1).filter(
        (n) => !row.done.includes(n),
      );
      let next = 0;
      let failure: unknown = null;
      const put = this.o.put ?? putPartWithFetch;
      const retries = this.o.retries ?? PART_RETRIES;
      const worker = async () => {
        while (failure === null && !l.abort.signal.aborted) {
          const n = todo[next++];
          if (n === undefined) return;
          const start = (n - 1) * row.partSize;
          const body = l.file.slice(start, Math.min(row.size, start + row.partSize));
          let ok = false;
          for (let attempt = 0; attempt <= retries; attempt++) {
            if (l.abort.signal.aborted) return;
            try {
              const { url } = await this.server.partUrl(l.mountId, row.id, n);
              await put(url, body, l.abort.signal);
              ok = true;
              break;
            } catch (err) {
              if (l.abort.signal.aborted) return;
              // The server said no (quota, a closed upload): another try cannot help.
              if (err instanceof ApiError && [400, 403, 404, 409, 429].includes(err.status)) {
                failure = err;
                return;
              }
              if (attempt < retries) await (this.o.wait ?? defaultWait)(attempt);
              else failure = err;
            }
          }
          if (!ok) return;
          await this.persist(l, { done: [...(l.row?.done ?? []), n] });
        }
      };
      await Promise.all(
        Array.from(
          { length: Math.min(this.o.concurrency ?? PART_CONCURRENCY, todo.length) },
          worker,
        ),
      );
      if (l.abort.signal.aborted) return;
      if (failure !== null) throw failure;
      await this.server.complete(l.mountId, row.id);
      l.state = 'done';
      await this.persist(l);
      await this.ledger.remove(row.id);
      l.settle?.(true);
    } catch (err) {
      if (l.abort.signal.aborted) return;
      this.fail(l, err, 'The upload failed.');
      // The server no longer knows it (swept, aborted elsewhere): nothing to resume.
      if (l.row && err instanceof ApiError && err.status === 404)
        await this.ledger.remove(l.row.id);
      else await this.persist(l);
      this.emit();
      l.settle?.(false);
    } finally {
      this.pump();
    }
  }

  /** Stop sending; the parts that arrived stay in the bucket and the ledger keeps the rest. */
  async pause(id: string): Promise<void> {
    const l = this.live.get(id);
    if (!l || l.state !== 'running') return;
    l.abort.abort();
    l.state = 'paused';
    await this.persist(l);
    this.pump();
  }

  /** Continue a paused or failed upload from what the bucket already holds. */
  async resume(id: string): Promise<void> {
    const l = this.live.get(id);
    if (!l || !['paused', 'failed'].includes(l.state)) return;
    l.abort = new AbortController();
    l.state = 'queued';
    l.error = undefined;
    l.code = undefined;
    const done = new Promise<void>((resolve, reject) => {
      l.settle = (ok) => (ok ? resolve() : reject(new Error(l.error ?? l.state)));
    });
    done.catch(() => undefined);
    this.emit();
    this.pump();
    await Promise.resolve();
  }

  /** Give the upload up: the bucket frees its parts and the ledger forgets it. */
  async cancel(id: string): Promise<void> {
    const l = this.live.get(id);
    if (!l) return;
    l.abort.abort();
    l.settle?.(false);
    if (l.row) {
      try {
        await this.server.abort(l.mountId, l.row.id);
      } catch {
        /* the server sweeps an unused upload after 24 h */
      }
      await this.ledger.remove(l.row.id);
    }
    this.live.delete(id);
    this.order = this.order.filter((x) => x !== id);
    this.emit();
    this.pump();
  }

  /** Drop the finished tasks from the list. */
  clearFinished(): void {
    for (const [id, l] of this.live) {
      if (l.state === 'done') {
        this.live.delete(id);
        this.order = this.order.filter((x) => x !== id);
      }
    }
    this.emit();
  }
}

let shared: MountUploadManager | null = null;

/** The page-wide manager. Browsers without IndexedDB keep the ledger in memory (no resume after a refresh). */
export function getMountUploads(): MountUploadManager {
  shared ??= new MountUploadManager(
    realMountUploadApi,
    typeof indexedDB === 'undefined'
      ? new MemoryLedger<MountUploadRow>()
      : defaultLedger<MountUploadRow>('mount'),
  );
  return shared;
}

/** Replace the manager (tests). */
export function setMountUploads(m: MountUploadManager | null) {
  shared = m;
}
