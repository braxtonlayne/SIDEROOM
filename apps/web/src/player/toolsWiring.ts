/**
 * MUS-SKN-001 x UTL-DEV-001 (integration, wave 34): bind wave 32's real tools to the skins' extras.
 *
 * The Pastel Study Deck (and any later skin that calls `useSkinExtras()`) shows a focus timer and a notes pad.
 * Here they become the shared ones the utility drawer uses: the same `pomodoro` timer (`useFocusTimer`), and the
 * member's end-to-end encrypted scratchpad (`useNotes`). The pad edits the newest note, the one the drawer's Notes
 * tool opens first, and creates one when the scratchpad is empty. Typing is kept locally and saved after a short
 * pause (and when the pad goes away), the way the drawer's editor saves.
 *
 * Until the member has set up the scratchpad's key (or while it cannot be reached), the pad falls back to the
 * in-memory pad for this tab and says so; nothing is ever written in plain text.
 *
 * `PlayerHost` loads this module lazily once, so the notes store and its crypto are not in the player's first
 * chunk; the extras remount when the binding arrives.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNotes } from '../tools/notes/useNotes';
import type { NotesStore } from '../tools/notes/notesStore';
import { useFocusTimer } from '../tools/timers/useFocusTimer';
import {
  DEFAULT_SKIN_EXTRAS,
  registerSkinExtras,
  type SkinFocusTimer,
  type SkinNotes,
} from './skins/backdrop/extras';

/** Same pause as the drawer's Notes editor. */
export const SKIN_NOTES_SAVE_MS = 800;

export function useSkinFocusTimer(): SkinFocusTimer {
  const f = useFocusTimer('pomodoro');
  return {
    clock: f.clock,
    running: f.running,
    progress: f.progress,
    segment: f.state.segment,
    toggle: f.toggle,
    reset: f.reset,
    skip: f.skip,
  };
}

/** The note the pad edits, per store, so it stays the same note while the tab is open. */
const chosen = new WeakMap<NotesStore, string>();

export function useSkinNotes(): SkinNotes {
  const { state, store } = useNotes();
  const memory = DEFAULT_SKIN_EXTRAS.useNotes();
  const ready = state.status === 'ready' && store !== null;

  let noteId = store ? chosen.get(store) : undefined;
  if (store && ready && (!noteId || !state.notes.some((n) => n.id === noteId))) {
    noteId = state.notes[0]?.id;
    if (noteId) chosen.set(store, noteId);
  }
  const note = state.notes.find((n) => n.id === noteId) ?? null;

  const [draft, setDraft] = useState<string | null>(null);
  const pending = useRef<{ text: string; timer: ReturnType<typeof setTimeout> } | null>(null);
  const creating = useRef<Promise<string | null> | null>(null);

  const flush = useCallback(() => {
    const p = pending.current;
    if (!p || !store) return;
    clearTimeout(p.timer);
    pending.current = null;
    const text = p.text;
    const id = chosen.get(store);
    if (id) {
      void store.save(id, text);
      return;
    }
    // The scratchpad is empty: the first save creates the note the pad keeps editing. A later save made while
    // that is in flight waits for it and then saves its own text into the new note.
    if (!creating.current) {
      creating.current = store.create(text).then((newId) => {
        if (newId) chosen.set(store, newId);
        return newId;
      });
      return;
    }
    void creating.current.then((newId) => {
      if (newId) void store.save(newId, text);
    });
  }, [store]);

  // Save what is still waiting when the pad goes away (a skin change, the player closing).
  useEffect(() => flush, [flush]);

  // Drop the local draft once the store holds the same text (or another note was chosen).
  useEffect(() => {
    if (pending.current === null && draft !== null && note && note.text === draft) setDraft(null);
  }, [note, draft]);

  const setText = useCallback(
    (text: string) => {
      setDraft(text);
      if (pending.current) clearTimeout(pending.current.timer);
      pending.current = { text, timer: setTimeout(flush, SKIN_NOTES_SAVE_MS) };
    },
    [flush],
  );

  if (state.status === 'loading' && store) {
    return { status: 'loading', text: '', setText: () => undefined, persistent: true };
  }
  if (!ready) return memory;
  return { status: 'ready', text: draft ?? note?.text ?? '', setText, persistent: true };
}

let unbind: (() => void) | null = null;

/** Bind once; later calls do nothing. Returns the unbinder (tests). */
export function wireSkinTools(): () => void {
  if (!unbind) {
    const off = registerSkinExtras({ useFocusTimer: useSkinFocusTimer, useNotes: useSkinNotes });
    unbind = () => {
      off();
      unbind = null;
    };
  }
  return unbind;
}
