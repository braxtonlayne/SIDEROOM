// Browser ledger for resumable uploads (D-271). One IndexedDB database, `sideroom-uploads`, with a store per
// area: `uploads` for the Library upload manager (review/uploadManager.ts) and `chat` for multipart chat
// attachments (lounge/msg/upload.ts). A row keeps the File itself, so a reload continues without choosing it
// again. Every call falls back quietly: a blocked or full database must never break an upload; `put` reports
// whether the row was stored so the caller can use its fallback.

import { apiFetch, getCsrf } from '../api/client';

export const DB_NAME = 'sideroom-uploads';
/** Version 2 added the `chat` store; version 1 only had `uploads`, whose rows survive the upgrade. */
export const DB_VERSION = 2;
export type StoreName = 'uploads' | 'chat';
const STORES: StoreName[] = ['uploads', 'chat'];

export interface Ledger<T extends { id: string }> {
  /** Stores the row; false when it could not be stored (quota, blocked database, no IndexedDB). */
  put(e: T): Promise<boolean>;
  all(): Promise<T[]>;
  remove(id: string): Promise<void>;
}

/** A ledger that lives as long as the page (tests). */
export class MemoryLedger<T extends { id: string }> implements Ledger<T> {
  private rows = new Map<string, T>();
  async put(e: T) {
    this.rows.set(e.id, { ...e });
    return true;
  }
  async all() {
    return [...this.rows.values()].map((r) => ({ ...r }));
  }
  async remove(id: string) {
    this.rows.delete(id);
  }
}

/** A ledger that stores nothing (no IndexedDB): every `put` answers false so callers use their fallback. */
export class UnavailableLedger<T extends { id: string }> implements Ledger<T> {
  async put() {
    return false;
  }
  async all() {
    return [];
  }
  async remove() {
    /* nothing stored */
  }
}

/** IndexedDB ledger over one store of the shared database. */
export class IdbLedger<T extends { id: string }> implements Ledger<T> {
  private db: Promise<IDBDatabase | null>;
  constructor(
    private readonly store: StoreName,
    name = DB_NAME,
  ) {
    this.db = new Promise((resolve) => {
      try {
        const req = indexedDB.open(name, DB_VERSION);
        req.onupgradeneeded = () => {
          for (const s of STORES) {
            if (!req.result.objectStoreNames.contains(s)) {
              req.result.createObjectStore(s, { keyPath: 'id' });
            }
          }
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => resolve(null);
        req.onblocked = () => resolve(null);
      } catch {
        resolve(null);
      }
    });
  }
  /** Runs one request in its own transaction; `ok` is false when the transaction failed or aborted. */
  private async run<R>(
    mode: IDBTransactionMode,
    f: (s: IDBObjectStore) => IDBRequest<R>,
  ): Promise<{ ok: boolean; value: R | null }> {
    const db = await this.db;
    if (!db) return { ok: false, value: null };
    return new Promise((resolve) => {
      try {
        const tx = db.transaction(this.store, mode);
        const req = f(tx.objectStore(this.store));
        let value: R | null = null;
        req.onsuccess = () => {
          value = req.result;
        };
        tx.oncomplete = () => resolve({ ok: true, value });
        tx.onabort = () => resolve({ ok: false, value: null });
        tx.onerror = () => resolve({ ok: false, value: null });
      } catch {
        resolve({ ok: false, value: null });
      }
    });
  }
  async put(e: T) {
    return (await this.run('readwrite', (s) => s.put(e))).ok;
  }
  async all() {
    return (await this.run('readonly', (s) => s.getAll() as IDBRequest<T[]>)).value ?? [];
  }
  async remove(id: string) {
    await this.run('readwrite', (s) => s.delete(id));
  }
}

/** The ledger a browser should use for `store`: IndexedDB when there is one, else a ledger that stores nothing. */
export function defaultLedger<T extends { id: string }>(store: StoreName): Ledger<T> {
  return typeof indexedDB === 'undefined' ? new UnavailableLedger<T>() : new IdbLedger<T>(store);
}

// ---- Chat store -------------------------------------------------------------------------------

export interface ChatUploadRow {
  /** The upload id of the multipart slot. */
  id: string;
  channelId: string;
  uploadFilename: string;
  size: number;
  partSize: number;
  partCount: number;
  /** Part numbers the server confirmed. */
  done: number[];
  savedAt: number;
  file: File | Blob;
  /** Server-side limit for the slot, kept for the message reference. */
  maxSize?: number;
  /** The description typed before sending, so the reattached file keeps it. */
  description?: string;
}

/** At most this many chat rows per browser profile; the oldest is dropped (and its slot aborted) first. */
export const CHAT_ROW_CAP = 5;
/** A row older than this is dropped: the server sweeps abandoned slots at 24 h. */
export const CHAT_ROW_MAX_AGE_MS = 20 * 3600 * 1000;

/**
 * The rules around the `chat` store: cap, expiry, removal. `abort` is called for a row that is dropped
 * without the user's say (evicted or expired) so its server slot does not linger; it must not throw.
 */
export class ChatUploads {
  constructor(
    private readonly ledger: Ledger<ChatUploadRow>,
    private readonly abort: (row: ChatUploadRow) => void,
    private readonly now: () => number = Date.now,
  ) {}

  private async sweep(rows: ChatUploadRow[]): Promise<ChatUploadRow[]> {
    const live: ChatUploadRow[] = [];
    for (const r of rows) {
      if (this.now() - r.savedAt > CHAT_ROW_MAX_AGE_MS) {
        await this.ledger.remove(r.id);
        this.abort(r);
      } else live.push(r);
    }
    return live;
  }

  /** The rows of one channel, expired rows dropped (checked on open). */
  async forChannel(channelId: string): Promise<ChatUploadRow[]> {
    const live = await this.sweep(await this.ledger.all());
    return live.filter((r) => r.channelId === channelId).sort((a, b) => a.savedAt - b.savedAt);
  }

  /** Every live row (used to abort the open slots on sign-out). */
  async all(): Promise<ChatUploadRow[]> {
    return this.sweep(await this.ledger.all());
  }

  /** Adds a row; false when it could not be stored (the caller then uses its localStorage record). */
  async add(row: ChatUploadRow): Promise<boolean> {
    const live = await this.sweep(await this.ledger.all());
    const others = live.filter((r) => r.id !== row.id).sort((a, b) => a.savedAt - b.savedAt);
    for (const old of others.slice(0, Math.max(0, others.length - (CHAT_ROW_CAP - 1)))) {
      await this.ledger.remove(old.id);
      this.abort(old);
    }
    return this.ledger.put(row);
  }

  /** Records the parts the server confirmed. Expiry is checked on every write. */
  async setDone(row: ChatUploadRow, done: number[]): Promise<boolean> {
    row.done = done;
    const live = await this.sweep(await this.ledger.all());
    if (!live.some((r) => r.id === row.id)) return false; // removed or expired meanwhile: do not bring it back
    return this.ledger.put({ ...row, done });
  }

  /** The upload completed, was cancelled, or the server no longer knows it. */
  async remove(id: string): Promise<void> {
    await this.ledger.remove(id);
  }

  /** Drops every row (sign-out). */
  async clear(): Promise<void> {
    for (const r of await this.ledger.all()) await this.ledger.remove(r.id);
  }
}

// ---- The browser's chat store ------------------------------------------------------------------

/** Best-effort `DELETE` of an unattached chat slot; `keepalive` lets it outlive a page that is going away. */
export async function abortChatSlot(
  row: Pick<ChatUploadRow, 'channelId' | 'id'>,
  keepalive = false,
): Promise<void> {
  const path = `/channels/${row.channelId}/attachments/${row.id}`;
  try {
    if (!keepalive) {
      await apiFetch(path, { method: 'DELETE', retries: 0 });
      return;
    }
    const token = await getCsrf();
    await fetch(`/api/v1${path}`, {
      method: 'DELETE',
      credentials: 'include',
      keepalive: true,
      ...(token ? { headers: { 'X-SR-CSRF': token } } : {}),
    });
  } catch {
    /* the server sweeps abandoned slots after 24 h */
  }
}

let chat: ChatUploads | null = null;

/** The chat store of this browser profile. */
export function chatUploads(): ChatUploads {
  chat ??= new ChatUploads(defaultLedger<ChatUploadRow>('chat'), (r) => void abortChatSlot(r));
  return chat;
}

/** Tests: swap the store (`null` goes back to the browser's). */
export function setChatUploads(c: ChatUploads | null): void {
  chat = c;
}

/**
 * Sign-out (D-271): aborts the open chat slots, then clears both stores. Call it BEFORE the session ends,
 * while the requests are still authorised. Never throws.
 */
export async function clearUploadsOnSignOut(): Promise<void> {
  try {
    const rows = await chatUploads().all();
    await Promise.all(rows.map((r) => abortChatSlot(r, true)));
    await chatUploads().clear();
    const lib = defaultLedger<{ id: string }>('uploads');
    for (const r of await lib.all()) await lib.remove(r.id);
  } catch {
    /* best effort */
  }
}
