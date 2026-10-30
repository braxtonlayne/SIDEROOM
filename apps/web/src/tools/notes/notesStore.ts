/**
 * UTL-DEV-001: the notes store, written to be embedded. The scratchpad tool, a pinned sticky note and (later) a
 * music player skin all read the same store: `useNotes()` for React, or `store.getState()` / `subscribe` without it.
 *
 * Plaintext exists only here, in memory. Every write is encrypted first (`./crypto`), and what reaches the server
 * is the ciphertext and a revision. A save that finds the note changed elsewhere (409) never loses an edit: the
 * other browser's version is kept as the note and the local text becomes a "conflict copy".
 */
import { ApiError, apiFetch } from '../../api/client';
import {
  type KeyStorage,
  NotesCryptoError,
  type RecoveryBundle,
  createNotesKey,
  decryptNote,
  encryptNote,
  fromBase64,
  indexedDbKeys,
  openRecoveryBundle,
  toBase64,
} from './crypto';

export interface RemoteNote {
  note_id: string;
  ciphertext: string;
  revision: number;
  updated_at: string;
}

export interface NotesApi {
  list(): Promise<RemoteNote[]>;
  put(id: string, ciphertext: string, baseRevision: number | null): Promise<RemoteNote>;
  remove(id: string): Promise<void>;
}

export const notesApi: NotesApi = {
  list: () => apiFetch<RemoteNote[]>('/users/@me/tools/notes'),
  put: (id, ciphertext, baseRevision) =>
    apiFetch<RemoteNote>(`/users/@me/tools/notes/${id}`, {
      method: 'PUT',
      json: baseRevision === null ? { ciphertext } : { ciphertext, base_revision: baseRevision },
      retries: 0,
    }),
  remove: (id) => apiFetch<void>(`/users/@me/tools/notes/${id}`, { method: 'DELETE', retries: 0 }),
};

export interface Note {
  id: string;
  text: string;
  revision: number;
  updatedAt: string;
  /** This note could not be opened with the key (damaged, or made with another key). Its text is empty. */
  unreadable: boolean;
}

export type NotesStatus = 'loading' | 'no-key' | 'ready' | 'error';

export interface NotesState {
  status: NotesStatus;
  notes: Note[];
  /** Notes on the server that this browser cannot open yet (shown when there is no key). */
  remoteCount: number;
  /** The recovery bundle for this browser's key, for "download recovery file". */
  bundle: RecoveryBundle | null;
  /** The last problem, as a code the UI turns into words. */
  problem: 'offline' | 'full' | 'too-big' | 'bad-passphrase' | 'bad-file' | 'unsupported' | null;
}

/** The longest plaintext that surely fits the 64 KiB ciphertext cap (UTF-8, plus 29 bytes of overhead). */
export const MAX_NOTE_TEXT_BYTES = 64 * 1024 - 64;

export interface NotesStore {
  getState(): NotesState;
  subscribe(listener: () => void): () => void;
  init(): Promise<void>;
  /** First use: make the key, wrap it with `passphrase`. Resolves with the recovery bundle to offer as a file. */
  setup(passphrase: string, rounds?: number): Promise<RecoveryBundle | null>;
  /** New browser: open a recovery file and load the notes. */
  restore(bundleJson: string, passphrase: string): Promise<boolean>;
  create(text?: string): Promise<string | null>;
  save(id: string, text: string): Promise<void>;
  remove(id: string): Promise<void>;
  /** Give up on notes that cannot be opened: delete them all on the server and start again with a new key. */
  startOver(): Promise<void>;
  /** The recovery file the member keeps: the notes key wrapped with their passphrase. Never contains plaintext. */
  exportFile(): string | null;
  /** Forget the key in this browser (sign-out on a shared computer). The notes stay on the server. */
  lock(): Promise<void>;
}

export interface NotesDeps {
  keys: KeyStorage;
  api?: NotesApi;
  uuid?: () => string;
}

const EMPTY: NotesState = { status: 'loading', notes: [], remoteCount: 0, bundle: null, problem: null };

const newest = (a: Note, b: Note) => b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id);

export function createNotesStore(deps: NotesDeps): NotesStore {
  const api = deps.api ?? notesApi;
  const uuid = deps.uuid ?? (() => crypto.randomUUID());
  let state: NotesState = EMPTY;
  let key: CryptoKey | null = null;
  const listeners = new Set<() => void>();
  // Saves of one note run one after another, so each starts from the revision the last one returned.
  const chains = new Map<string, Promise<void>>();

  const set = (patch: Partial<NotesState>) => {
    state = { ...state, ...patch };
    for (const l of [...listeners]) l();
  };
  const upsert = (n: Note) =>
    set({ notes: [...state.notes.filter((x) => x.id !== n.id), n].sort(newest) });

  const open = async (r: RemoteNote): Promise<Note> => {
    try {
      const text = await decryptNote(key!, fromBase64(r.ciphertext), r.note_id);
      return { id: r.note_id, text, revision: r.revision, updatedAt: r.updated_at, unreadable: false };
    } catch {
      return { id: r.note_id, text: '', revision: r.revision, updatedAt: r.updated_at, unreadable: true };
    }
  };

  const fail = (e: unknown): NotesState['problem'] => {
    if (e instanceof NotesCryptoError) {
      return e.problem === 'bad-passphrase'
        ? 'bad-passphrase'
        : e.problem === 'unsupported'
          ? 'unsupported'
          : 'bad-file';
    }
    if (e instanceof ApiError && e.status === 409) return 'full';
    if (e instanceof ApiError && e.status === 400) return 'too-big';
    return 'offline';
  };

  const loadAll = async () => {
    const remote = await api.list();
    const notes = await Promise.all(remote.map(open));
    set({ status: 'ready', notes: notes.sort(newest), remoteCount: remote.length, problem: null });
  };

  const store: NotesStore = {
    getState: () => state,
    subscribe(l) {
      listeners.add(l);
      return () => listeners.delete(l);
    },

    async init() {
      set({ status: 'loading', problem: null });
      try {
        const held = await deps.keys.load();
        if (!held) {
          const remote = await api.list().catch(() => []);
          set({ status: 'no-key', remoteCount: remote.length, notes: [], bundle: null });
          return;
        }
        key = held.key;
        set({ bundle: held.bundle });
        await loadAll();
      } catch (e) {
        set({ status: 'error', problem: fail(e) });
      }
    },

    async setup(passphrase, rounds) {
      try {
        const made = await createNotesKey(passphrase, rounds);
        await deps.keys.save(made.key, made.bundle);
        key = made.key;
        set({ status: 'ready', bundle: made.bundle, notes: [], problem: null });
        return made.bundle;
      } catch (e) {
        set({ problem: fail(e) });
        return null;
      }
    },

    async restore(bundleJson, passphrase) {
      try {
        let parsed: unknown;
        try {
          parsed = JSON.parse(bundleJson);
        } catch {
          throw new NotesCryptoError('bad-bundle');
        }
        // A whole export file holds the bundle under `key`; a bare recovery file is the bundle itself.
        const raw = parsed as { key?: unknown; kind?: unknown } | null;
        const bundle = (raw && raw.kind === 'sideroom-notes' ? raw.key : parsed) as RecoveryBundle;
        const opened = await openRecoveryBundle(bundle, passphrase);
        await deps.keys.save(opened, bundle);
        key = opened;
        set({ bundle, problem: null });
        await loadAll();
        return true;
      } catch (e) {
        set({ problem: fail(e) });
        return false;
      }
    },

    async create(text = '') {
      if (!key) return null;
      const id = uuid();
      const note: Note = { id, text, revision: 0, updatedAt: new Date().toISOString(), unreadable: false };
      upsert(note);
      await store.save(id, text);
      return id;
    },

    save(id, text) {
      const run = async () => {
        if (!key) return;
        if (new TextEncoder().encode(text).length > MAX_NOTE_TEXT_BYTES) {
          set({ problem: 'too-big' });
          return;
        }
        const cur = state.notes.find((n) => n.id === id);
        if (!cur) return;
        const wire = toBase64(await encryptNote(key, text, id));
        try {
          const saved = await api.put(id, wire, cur.revision === 0 ? null : cur.revision);
          upsert({
            id,
            text,
            revision: saved.revision,
            updatedAt: saved.updated_at,
            unreadable: false,
          });
          set({ problem: null });
        } catch (e) {
          if (e instanceof ApiError && e.status === 409 && cur.revision !== 0) {
            // Changed elsewhere: take the other version as the note and keep ours as a copy.
            const remote = (await api.list()).find((r) => r.note_id === id);
            if (remote) {
              upsert(await open(remote));
              if (text.trim() !== '') await store.create(text);
              set({ problem: null });
              return;
            }
          }
          set({ problem: fail(e) });
        }
      };
      const prev = chains.get(id) ?? Promise.resolve();
      const next = prev.then(run, run);
      chains.set(id, next);
      return next;
    },

    async remove(id) {
      const had = state.notes.find((n) => n.id === id);
      set({ notes: state.notes.filter((n) => n.id !== id) });
      if (!had || had.revision === 0) return;
      try {
        await api.remove(id);
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 404)) {
          upsert(had);
          set({ problem: fail(e) });
        }
      }
    },

    async startOver() {
      try {
        const remote = await api.list();
        await Promise.all(remote.map((r) => api.remove(r.note_id)));
        await deps.keys.clear();
        key = null;
        set({ status: 'no-key', notes: [], remoteCount: 0, bundle: null, problem: null });
      } catch (e) {
        set({ problem: fail(e) });
      }
    },

    exportFile() {
      if (!state.bundle) return null;
      // The key wrapped with the passphrase, nothing else: the notes themselves are on the server as ciphertext.
      return JSON.stringify({ kind: 'sideroom-notes', v: 1, key: state.bundle });
    },

    async lock() {
      await deps.keys.clear();
      key = null;
      set({ status: 'no-key', notes: [], bundle: null });
    },
  };
  return store;
}

const shared = new Map<string, NotesStore>();

/** The page-wide store of an account (one per signed-in user id). */
export function notesStoreFor(userId: string): NotesStore {
  let s = shared.get(userId);
  if (!s) {
    s = createNotesStore({ keys: indexedDbKeys(userId) });
    shared.set(userId, s);
  }
  return s;
}

/** Tests: forget the shared stores. */
export function resetSharedNotes(): void {
  shared.clear();
}
