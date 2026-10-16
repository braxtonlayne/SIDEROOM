// Upload manager (REV-UP-002): resumable multipart uploads of chapter files. The file is cut into the
// parts the server announced (64 MB), up to four parts go out at once, a failed part is retried with a
// pause, and an IndexedDB ledger keeps the file and the upload slot, so after a refresh the manager asks the
// server which parts arrived and sends only the missing ones. The server completes the upload itself
// from the parts it holds, so the browser never needs the part ETags.
import type * as P from './apiP13';

export const PART_CONCURRENCY = 4;
export const PART_RETRIES = 3;

export type TaskState = 'queued' | 'running' | 'paused' | 'failed' | 'done';

export interface LedgerEntry {
  id: string;
  submissionId: string;
  chapterId: string;
  uploadId: string;
  filename: string;
  size: number;
  partSize: number;
  partCount: number;
  /** Part numbers the server confirmed. */
  done: number[];
  state: TaskState;
  error?: string;
  /** The file itself, so a refresh can continue without choosing it again. */
  file: File | Blob;
}

export interface Ledger {
  put(e: LedgerEntry): Promise<void>;
  all(): Promise<LedgerEntry[]>;
  remove(id: string): Promise<void>;
}

/** A ledger that lives as long as the page (tests, or browsers without IndexedDB). */
export class MemoryLedger implements Ledger {
  private rows = new Map<string, LedgerEntry>();
  async put(e: LedgerEntry) {
    this.rows.set(e.id, { ...e });
  }
  async all() {
    return [...this.rows.values()].map((r) => ({ ...r }));
  }
  async remove(id: string) {
    this.rows.delete(id);
  }
}

/** IndexedDB ledger. Every call falls back quietly: a blocked database must never break an upload. */
export class IdbLedger implements Ledger {
  private db: Promise<IDBDatabase | null>;
  constructor(name = 'sideroom-uploads') {
    this.db = new Promise((resolve) => {
      try {
        const req = indexedDB.open(name, 1);
        req.onupgradeneeded = () => req.result.createObjectStore('uploads', { keyPath: 'id' });
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => resolve(null);
      } catch {
        resolve(null);
      }
    });
  }
  private async run<T>(
    mode: IDBTransactionMode,
    f: (s: IDBObjectStore) => IDBRequest<T>,
  ): Promise<T | null> {
    const db = await this.db;
    if (!db) return null;
    return new Promise((resolve) => {
      try {
        const req = f(db.transaction('uploads', mode).objectStore('uploads'));
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => resolve(null);
      } catch {
        resolve(null);
      }
    });
  }
  async put(e: LedgerEntry) {
    await this.run('readwrite', (s) => s.put(e));
  }
  async all() {
    return (await this.run('readonly', (s) => s.getAll() as IDBRequest<LedgerEntry[]>)) ?? [];
  }
  async remove(id: string) {
    await this.run('readwrite', (s) => s.delete(id));
  }
}

export interface UploadApi {
  begin(
    submissionId: string,
    chapterId: string,
    b: { filename: string; size_bytes: number },
  ): Promise<P.MultipartUploadSlot>;
  parts(submissionId: string, uploadId: string): Promise<P.UploadParts>;
  complete(submissionId: string, uploadId: string): Promise<unknown>;
  abort(submissionId: string, uploadId: string): Promise<unknown>;
}

export interface Options {
  concurrency?: number;
  retries?: number;
  /** Sends one part; rejects on any failure. */
  put: (url: string, body: Blob, signal: AbortSignal) => Promise<void>;
  /** Pause between attempts (tests make it instant). */
  wait?: (attempt: number) => Promise<void>;
}

export interface UploadTask {
  id: string;
  submissionId: string;
  chapterId: string;
  filename: string;
  size: number;
  state: TaskState;
  /** 0 to 1. */
  progress: number;
  error?: string;
}

interface Live {
  entry: LedgerEntry;
  urls: Map<number, string>;
  abort: AbortController;
  /** Resolves when the run ends (done, failed or paused). */
  finished?: Promise<void>;
  settle?: (ok: boolean) => void;
}

const defaultWait = (attempt: number) =>
  new Promise<void>((r) => setTimeout(r, 500 * 2 ** attempt));

export class UploadManager {
  private live = new Map<string, Live>();
  private subs = new Set<() => void>();
  private cache: UploadTask[] = [];
  constructor(
    private api: UploadApi,
    private ledger: Ledger,
    private o: Options,
  ) {}

  subscribe(fn: () => void): () => void {
    this.subs.add(fn);
    return () => this.subs.delete(fn);
  }

  /** Stable between changes, so it can back `useSyncExternalStore`. */
  snapshot(): UploadTask[] {
    return this.cache;
  }

  /** Overall progress of the unfinished uploads, 0 to 1. */
  total(): number {
    const open = this.cache.filter((t) => t.state !== 'done');
    const size = open.reduce((n, t) => n + t.size, 0);
    return size === 0 ? 1 : open.reduce((n, t) => n + t.progress * t.size, 0) / size;
  }

  private emit() {
    this.cache = [...this.live.values()].map(({ entry }) => ({
      id: entry.id,
      submissionId: entry.submissionId,
      chapterId: entry.chapterId,
      filename: entry.filename,
      size: entry.size,
      state: entry.state,
      progress: entry.partCount === 0 ? 0 : this.bytes(entry) / Math.max(1, entry.size),
      ...(entry.error ? { error: entry.error } : {}),
    }));
    for (const f of this.subs) f();
  }

  private bytes(e: LedgerEntry): number {
    return e.done.reduce(
      (n, p) => n + (p === e.partCount ? e.size - e.partSize * (e.partCount - 1) : e.partSize),
      0,
    );
  }

  /** Start uploading `file` into a chapter; resolves when it is complete (rejects when it fails or is cancelled). */
  async enqueue(submissionId: string, chapterId: string, file: File): Promise<void> {
    const slot = await this.api.begin(submissionId, chapterId, {
      filename: file.name,
      size_bytes: file.size,
    });
    if (!slot.multipart) throw new Error('This server does not offer resumable uploads.');
    const entry: LedgerEntry = {
      id: slot.upload.id,
      submissionId,
      chapterId,
      uploadId: slot.upload.id,
      filename: file.name,
      size: file.size,
      partSize: slot.multipart.part_size,
      partCount: slot.multipart.part_count,
      done: [],
      state: 'queued',
      file,
    };
    await this.ledger.put(entry);
    const urls = new Map(slot.multipart.parts.map((p) => [p.part_number, p.url]));
    return this.start(entry, urls);
  }

  /** After a refresh: continue every upload the ledger remembers that was not finished. */
  async resumeAll(): Promise<UploadTask[]> {
    for (const e of await this.ledger.all()) {
      if (e.state === 'done' || this.live.has(e.id)) continue;
      // Paused uploads stay paused; the rest continue on their own.
      const entry: LedgerEntry = { ...e, state: e.state === 'paused' ? 'paused' : 'queued' };
      this.live.set(e.id, { entry, urls: new Map(), abort: new AbortController() });
      if (entry.state === 'queued') void this.start(entry, null).catch(() => undefined);
    }
    this.emit();
    return this.cache;
  }

  private start(entry: LedgerEntry, urls: Map<number, string> | null): Promise<void> {
    const prev = this.live.get(entry.id);
    const live: Live = { entry, urls: urls ?? new Map(), abort: new AbortController() };
    this.live.set(entry.id, live);
    if (prev) live.abort = prev.abort.signal.aborted ? new AbortController() : prev.abort;
    const done = new Promise<void>((resolve, reject) => {
      live.settle = (ok) =>
        ok ? resolve() : reject(new Error(live.entry.error ?? live.entry.state));
    });
    done.catch(() => undefined);
    void this.run(live, urls === null);
    return done;
  }

  private async persist(live: Live, patch: Partial<LedgerEntry> = {}) {
    live.entry = { ...live.entry, ...patch };
    await this.ledger.put(live.entry);
    this.emit();
  }

  private async run(live: Live, needUrls: boolean): Promise<void> {
    const e = live.entry;
    try {
      await this.persist(live, { state: 'running', error: undefined as never });
      if (needUrls) {
        const p = await this.api.parts(e.submissionId, e.uploadId);
        live.entry = { ...live.entry, done: p.received.map((r) => r.part_number) };
        live.urls = new Map(p.missing.map((m) => [m.part_number, m.url]));
      }
      const todo = [...live.urls.keys()]
        .filter((n) => !live.entry.done.includes(n))
        .sort((a, b) => a - b);
      let next = 0;
      let failure: string | null = null;
      const worker = async () => {
        while (!failure && !live.abort.signal.aborted) {
          const n = todo[next++];
          if (n === undefined) return;
          const url = live.urls.get(n)!;
          const start = (n - 1) * live.entry.partSize;
          const body = (live.entry.file as Blob).slice(
            start,
            Math.min(live.entry.size, start + live.entry.partSize),
          );
          let ok = false;
          for (let attempt = 0; attempt <= (this.o.retries ?? PART_RETRIES); attempt++) {
            if (live.abort.signal.aborted) return;
            try {
              await this.o.put(url, body, live.abort.signal);
              ok = true;
              break;
            } catch (err) {
              if (live.abort.signal.aborted) return;
              failure = err instanceof Error ? err.message : String(err);
              if (attempt < (this.o.retries ?? PART_RETRIES)) {
                failure = null;
                await (this.o.wait ?? defaultWait)(attempt);
              }
            }
          }
          if (!ok) return;
          await this.persist(live, { done: [...live.entry.done, n] });
        }
      };
      await Promise.all(
        Array.from(
          { length: Math.min(this.o.concurrency ?? PART_CONCURRENCY, todo.length) },
          worker,
        ),
      );
      if (live.abort.signal.aborted) return;
      if (failure) throw new Error(failure);
      await this.api.complete(e.submissionId, e.uploadId);
      await this.persist(live, { state: 'done' });
      await this.ledger.remove(e.id);
      live.settle?.(true);
    } catch (err) {
      if (live.abort.signal.aborted) return;
      await this.persist(live, {
        state: 'failed',
        error: err instanceof Error ? err.message : String(err),
      });
      live.settle?.(false);
    }
  }

  /** Stop sending; the parts that arrived stay on the server, and the ledger keeps the rest. */
  async pause(id: string): Promise<void> {
    const live = this.live.get(id);
    if (!live || live.entry.state !== 'running') return;
    live.abort.abort();
    await this.persist(live, { state: 'paused' });
  }

  /** Continue a paused or failed upload from what the server already holds. */
  async resume(id: string): Promise<void> {
    const live = this.live.get(id);
    if (!live || !['paused', 'failed'].includes(live.entry.state)) return;
    await this.start({ ...live.entry, state: 'queued' }, null).catch(() => undefined);
  }

  /** Give the upload up: the server frees its parts and the ledger forgets it. */
  async cancel(id: string): Promise<void> {
    const live = this.live.get(id);
    if (!live) return;
    live.abort.abort();
    live.settle?.(false);
    try {
      await this.api.abort(live.entry.submissionId, live.entry.uploadId);
    } catch {
      /* the server expires an unused slot on its own */
    }
    await this.ledger.remove(id);
    this.live.delete(id);
    this.emit();
  }
}

/** A part `PUT` with fetch; the browser needs CORS to the storage endpoint for this. */
export async function putPartWithFetch(
  url: string,
  body: Blob,
  signal: AbortSignal,
): Promise<void> {
  const r = await fetch(url, { method: 'PUT', body, signal });
  if (!r.ok) throw new Error(`The storage answered ${r.status}.`);
}
