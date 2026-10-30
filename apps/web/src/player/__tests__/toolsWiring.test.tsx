/**
 * MUS-SKN-001 x UTL-DEV-001 (integration): the Pastel Study Deck's timer and notes pad are wave 32's shared
 * pomodoro and encrypted scratchpad once `wireSkinTools()` binds them. The Study redesign reads the extras in two
 * places (TimerExtra and NotesExtra, each calling `useSkinExtras()`); both must pick up the binding.
 */
import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { NotesState, NotesStore } from '../../tools/notes/notesStore';
import { resetSharedTimers, timerStore } from '../../tools/timers/timerStore';
import { __resetSkinExtras } from '../skins/backdrop/extras';
import { StudySkin } from '../skins/backdrop/StudySkin';
import { __resetPlayerStore, engineReport, play } from '../store';
import { SKIN_NOTES_SAVE_MS, wireSkinTools } from '../toolsWiring';

/** A notes store the test controls; `useNotes()` is mocked to return it. */
function fakeNotes(initial: Partial<NotesState>) {
  let state: NotesState = {
    status: 'ready',
    notes: [],
    remoteCount: 0,
    bundle: null,
    problem: null,
    ...initial,
  };
  const subs = new Set<() => void>();
  const set = (patch: Partial<NotesState>) => {
    state = { ...state, ...patch };
    subs.forEach((f) => f());
  };
  const save = vi.fn(async (id: string, text: string) => {
    set({
      notes: state.notes.map((n) => (n.id === id ? { ...n, text, revision: n.revision + 1 } : n)),
    });
  });
  const create = vi.fn(async (text = '') => {
    const id = `new-${state.notes.length + 1}`;
    set({
      notes: [{ id, text, revision: 1, updatedAt: '', unreadable: false }, ...state.notes],
    });
    return id;
  });
  const store = {
    getState: () => state,
    subscribe: (f: () => void) => {
      subs.add(f);
      return () => subs.delete(f);
    },
    save,
    create,
  } as unknown as NotesStore;
  return {
    store,
    set,
    save,
    create,
    get state() {
      return state;
    },
  };
}

let current: ReturnType<typeof fakeNotes> | null = null;
vi.mock('../../tools/notes/useNotes', async () => {
  const { useSyncExternalStore } = await import('react');
  return {
    useNotes: () => {
      const f = current!;
      const state = useSyncExternalStore(f.store.subscribe, f.store.getState, f.store.getState);
      return { state, store: f.store };
    },
  };
});

let unbind: (() => void) | null = null;
beforeEach(() => {
  __resetPlayerStore();
  __resetSkinExtras();
  resetSharedTimers();
  play([{ id: 'a', title: 'Alpha', src: '/a.mp3', durationSec: 200 }]);
  engineReport.status('playing');
});
afterEach(() => {
  unbind?.();
  unbind = null;
  vi.useRealTimers();
});

describe('wireSkinTools', () => {
  it('drives the shared pomodoro from both the timer and the notes columns', async () => {
    current = fakeNotes({
      notes: [{ id: 'n1', text: 'Newest note', revision: 3, updatedAt: '', unreadable: false }],
    });
    const user = userEvent.setup();
    render(<StudySkin />);
    act(() => {
      unbind = wireSkinTools();
    });
    // The timer column reads the shared store: starting it here starts the drawer's pomodoro.
    expect(screen.getByRole('timer').textContent).toBe('25:00');
    await user.click(screen.getByRole('button', { name: 'Start' }));
    expect(timerStore('pomodoro').getState().phase).toBe('running');
    await user.click(screen.getByRole('button', { name: 'Pause timer' }));
    expect(timerStore('pomodoro').getState().phase).not.toBe('running');
    // The notes column shows the newest scratchpad note and says it is saved, encrypted.
    expect((screen.getByRole('textbox', { name: 'Notes' }) as HTMLTextAreaElement).value).toBe(
      'Newest note',
    );
    expect(screen.getByText('Saved to your encrypted notes.')).toBeTruthy();
  });

  it('saves typing into the newest note after a pause, and on unmount', () => {
    vi.useFakeTimers();
    current = fakeNotes({
      notes: [{ id: 'n1', text: 'Old', revision: 1, updatedAt: '', unreadable: false }],
    });
    unbind = wireSkinTools();
    const { unmount } = render(<StudySkin />);
    const pad = screen.getByRole('textbox', { name: 'Notes' }) as HTMLTextAreaElement;
    fireEvent.change(pad, { target: { value: 'Old, then new' } });
    expect(pad.value).toBe('Old, then new');
    expect(current.save).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(SKIN_NOTES_SAVE_MS + 10);
    });
    expect(current.save).toHaveBeenCalledWith('n1', 'Old, then new');
    fireEvent.change(pad, { target: { value: 'Unsaved tail' } });
    unmount();
    expect(current.save).toHaveBeenLastCalledWith('n1', 'Unsaved tail');
  });

  it('creates a note when the scratchpad is empty', async () => {
    vi.useFakeTimers();
    current = fakeNotes({ notes: [] });
    unbind = wireSkinTools();
    render(<StudySkin />);
    fireEvent.change(screen.getByRole('textbox', { name: 'Notes' }), {
      target: { value: 'First thought' },
    });
    await act(async () => {
      vi.advanceTimersByTime(SKIN_NOTES_SAVE_MS + 10);
    });
    expect(current.create).toHaveBeenCalledWith('First thought');
    expect(current.state.notes[0]?.text).toBe('First thought');
  });

  it('falls back to the tab-only pad until the scratchpad has a key', () => {
    current = fakeNotes({ status: 'no-key' });
    unbind = wireSkinTools();
    render(<StudySkin />);
    expect(screen.getByRole('textbox', { name: 'Notes' })).toBeTruthy();
    expect(screen.getByText(/^Kept in this tab only\. Set up Notes/)).toBeTruthy();
  });
});
