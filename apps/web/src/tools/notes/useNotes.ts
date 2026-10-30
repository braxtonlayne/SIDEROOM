/**
 * UTL-DEV-001: `useNotes()`, the reusable face of the encrypted scratchpad. A tool, a pinned sticky note or a
 * music player skin calls this and gets the signed-in account's notes store, initialised once.
 *
 *   const { state, store } = useNotes();
 *   state.status === 'ready' && state.notes.map((n) => n.text)
 */
import { useEffect, useSyncExternalStore } from 'react';
import { useCurrentUser } from '../../store/hooks';
import { type NotesState, type NotesStore, notesStoreFor } from './notesStore';

export interface UseNotes {
  state: NotesState;
  store: NotesStore | null;
}

const IDLE: NotesState = {
  status: 'loading',
  notes: [],
  remoteCount: 0,
  bundle: null,
  problem: null,
};
const started = new WeakSet<NotesStore>();
const noop = () => () => undefined;

export function useNotes(): UseNotes {
  const user = useCurrentUser();
  const store = user ? notesStoreFor(user.id) : null;
  const state = useSyncExternalStore(
    store ? store.subscribe : noop,
    store ? store.getState : () => IDLE,
    () => IDLE,
  );
  useEffect(() => {
    // Initialise each store once (React strict mode runs effects twice).
    if (store && !started.has(store)) {
      started.add(store);
      void store.init();
    }
  }, [store]);
  return { state, store };
}
