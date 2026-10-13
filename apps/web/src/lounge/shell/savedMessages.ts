/**
 * Bookmarks and Remind Me (LNG-MSGUI-026). There is no server endpoint for either, so both live in the
 * account's settings blob (`GET/PATCH /users/@me/settings`) under the keys `bookmarks` and `reminders`, each a
 * map keyed by message id so a merge patch can add or drop one entry. Reminders fire from the open app: a
 * timer checks them while the Lounge is mounted, and ones that came due while it was closed show on the next
 * visit. Nothing is delivered to a closed browser (that needs Web Push, LNG-NTF-003).
 */
import { useEffect } from 'react';
import { create } from 'zustand';
import { rest, useStore } from '../../store';
import type { Message } from '../data';

export interface SavedMessage {
  message_id: string;
  channel_id: string;
  guild_id: string | null;
  /** Start of the message text, for the list and the reminder toast. */
  preview: string;
  author: string;
  saved_at: string;
}

export interface Reminder extends SavedMessage {
  /** ISO time the reminder is due. */
  due_at: string;
}

export const REMIND_CHOICES = [
  { label: 'in 5 minutes', ms: 5 * 60_000 },
  { label: 'in 1 hour', ms: 60 * 60_000 },
  { label: 'in 3 hours', ms: 3 * 60 * 60_000 },
  { label: 'in 8 hours', ms: 8 * 60 * 60_000 },
  { label: 'in 24 hours', ms: 24 * 60 * 60_000 },
] as const;

const PREVIEW_CHARS = 140;

export function toSaved(m: Message, guildId: string | undefined, now = new Date()): SavedMessage {
  const text = m.content.trim() || (m.attachments?.length ? 'Attachment' : 'Message');
  return {
    message_id: m.id,
    channel_id: m.channelId,
    guild_id: guildId ?? null,
    preview: text.length > PREVIEW_CHARS ? `${text.slice(0, PREVIEW_CHARS)}…` : text,
    author: m.author.displayName,
    saved_at: now.toISOString(),
  };
}

type Wire = Record<string, unknown>;
const isRecord = (v: unknown): v is Wire => !!v && typeof v === 'object' && !Array.isArray(v);

/** Pure: keeps only well-formed entries of a settings sub-object. */
export function parseSaved<T extends SavedMessage>(
  raw: unknown,
  extra?: keyof T,
): Record<string, T> {
  const out: Record<string, T> = {};
  if (!isRecord(raw)) return out;
  for (const [id, v] of Object.entries(raw)) {
    if (!isRecord(v)) continue;
    if (typeof v.message_id !== 'string' || typeof v.channel_id !== 'string') continue;
    if (extra && typeof v[extra as string] !== 'string') continue;
    out[id] = v as unknown as T;
  }
  return out;
}

export interface SavedState {
  bookmarks: Record<string, SavedMessage>;
  reminders: Record<string, Reminder>;
  /** Replace both maps from a settings blob. */
  adopt(settings: Record<string, unknown>): void;
  toggleBookmark(entry: SavedMessage): void;
  addReminder(entry: SavedMessage, dueAt: Date): void;
  dismissReminder(id: string): void;
  reset(): void;
}

const push = (key: 'bookmarks' | 'reminders', id: string, value: unknown) => {
  rest.patchUserSettings({ [key]: { [id]: value } }).catch(() => {
    /* offline: the local copy stays until the next visit; a failed save is not surfaced */
  });
};

export const useSaved = create<SavedState>()((set, get) => ({
  bookmarks: {},
  reminders: {},
  adopt: (settings) =>
    set({
      bookmarks: parseSaved<SavedMessage>(settings.bookmarks),
      reminders: parseSaved<Reminder>(settings.reminders, 'due_at'),
    }),
  toggleBookmark: (entry) => {
    const id = entry.message_id;
    const had = id in get().bookmarks;
    set((s) => {
      const next = { ...s.bookmarks };
      if (had) delete next[id];
      else next[id] = entry;
      return { bookmarks: next };
    });
    push('bookmarks', id, had ? null : entry);
  },
  addReminder: (entry, dueAt) => {
    const reminder: Reminder = { ...entry, due_at: dueAt.toISOString() };
    const id = entry.message_id;
    set((s) => ({ reminders: { ...s.reminders, [id]: reminder } }));
    push('reminders', id, reminder);
  },
  dismissReminder: (id) => {
    set((s) => {
      const next = { ...s.reminders };
      delete next[id];
      return { reminders: next };
    });
    push('reminders', id, null);
  },
  reset: () => set({ bookmarks: {}, reminders: {} }),
}));

/** Reminders whose time has come, oldest first. */
export function dueReminders(reminders: Record<string, Reminder>, now = Date.now()): Reminder[] {
  return Object.values(reminders)
    .filter((r) => Date.parse(r.due_at) <= now)
    .sort((a, b) => Date.parse(a.due_at) - Date.parse(b.due_at));
}

/** Keeps the saved maps in step with the settings blob the sync loads and other sessions update. */
export function useSavedSync(enabled: boolean): void {
  useEffect(() => {
    if (!enabled) return;
    const adopt = () => {
      const blob = useStore.getState().userSettings;
      if (blob) useSaved.getState().adopt(blob.settings);
    };
    adopt();
    let last = useStore.getState().userSettings;
    return useStore.subscribe((s) => {
      if (s.userSettings === last) return;
      last = s.userSettings;
      adopt();
    });
  }, [enabled]);
}
