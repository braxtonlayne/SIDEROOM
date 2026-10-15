/**
 * Bookmarks, reminders and scheduled messages (FTR-BKM-001, FTR-REM-001, FTR-SCH-001, LNG-MSGUI-026).
 * They live on the server (`/users/@me/bookmarks`, `/reminders`, `/scheduled-messages`), so a reminder reaches a
 * closed browser as a direct message and everything follows the person across devices. This store keeps the
 * loaded copy and updates it optimistically; a failed save puts the previous state back.
 */
import { create } from 'zustand';
import type { Message } from '../data';
import * as api from '../features/api';
import type { BookmarkRow, ReminderRow, ScheduledMessage } from '../features/api';

/** A message a bookmark or reminder points at. */
export interface SavedRef {
  message_id: string;
  channel_id: string;
  guild_id: string | null;
  /** Start of the message text; becomes the note of a reminder. */
  preview: string;
}

export const REMIND_CHOICES = [
  { label: 'in 5 minutes', ms: 5 * 60_000 },
  { label: 'in 1 hour', ms: 60 * 60_000 },
  { label: 'in 3 hours', ms: 3 * 60 * 60_000 },
  { label: 'in 8 hours', ms: 8 * 60 * 60_000 },
  { label: 'in 24 hours', ms: 24 * 60 * 60_000 },
] as const;

const PREVIEW_CHARS = 140;

export function toSaved(m: Message, guildId: string | undefined): SavedRef {
  const text = m.content.trim() || (m.attachments?.length ? 'Attachment' : 'Message');
  return {
    message_id: m.id,
    channel_id: m.channelId,
    guild_id: guildId ?? null,
    preview: text.length > PREVIEW_CHARS ? `${text.slice(0, PREVIEW_CHARS)}…` : text,
  };
}

/** Ids are numeric snowflakes; anything else (a local placeholder) cannot be sent to the server. */
const isSnowflake = (v: string) => /^\d+$/.test(v);

export interface SavedState {
  /** Bookmarks by message id (the newest hundred; the panel pages for more). */
  bookmarks: Record<string, BookmarkRow>;
  /** Pending reminders, soonest first. */
  reminders: ReminderRow[];
  /** Pending scheduled messages, soonest first. */
  scheduled: ScheduledMessage[];
  loaded: boolean;
  /** Fetch all three lists. A list that fails keeps what it had. */
  load(): Promise<void>;
  toggleBookmark(ref: SavedRef): Promise<void>;
  setBookmarkNote(messageId: string, note: string): Promise<void>;
  /** Remind about a message, or (without `ref`) about a note. */
  addReminder(input: { note: string; remindAt: Date; ref?: SavedRef | undefined }): Promise<void>;
  cancelReminder(id: string): Promise<void>;
  addScheduled(row: ScheduledMessage): void;
  replaceScheduled(row: ScheduledMessage): void;
  cancelScheduled(id: string): Promise<void>;
  reset(): void;
}

const byDue = (a: ReminderRow, b: ReminderRow) => Date.parse(a.remind_at) - Date.parse(b.remind_at);
const bySend = (a: ScheduledMessage, b: ScheduledMessage) =>
  Date.parse(a.send_at) - Date.parse(b.send_at);

const EMPTY = { bookmarks: {}, reminders: [], scheduled: [], loaded: false };

export const useSaved = create<SavedState>()((set, get) => ({
  ...EMPTY,
  load: async () => {
    const [marks, reminders, scheduled] = await Promise.allSettled([
      api.listBookmarkIds(),
      api.listReminders(),
      api.listScheduled(),
    ]);
    set((s) => ({
      bookmarks:
        marks.status === 'fulfilled'
          ? Object.fromEntries(marks.value.map((e) => [e.bookmark.message_id, e.bookmark]))
          : s.bookmarks,
      reminders: reminders.status === 'fulfilled' ? reminders.value.sort(byDue) : s.reminders,
      scheduled: scheduled.status === 'fulfilled' ? scheduled.value.sort(bySend) : s.scheduled,
      loaded: true,
    }));
  },
  toggleBookmark: async (ref) => {
    const id = ref.message_id;
    const before = get().bookmarks;
    const had = id in before;
    set({
      bookmarks: had
        ? Object.fromEntries(Object.entries(before).filter(([k]) => k !== id))
        : {
            ...before,
            [id]: {
              message_id: id,
              channel_id: ref.channel_id,
              guild_id: ref.guild_id,
              note: '',
              created_at: new Date().toISOString(),
            },
          },
    });
    try {
      if (had) await api.removeBookmark(id);
      else {
        const row = await api.saveBookmark(ref.channel_id, id);
        set((s) => ({ bookmarks: { ...s.bookmarks, [id]: row } }));
      }
    } catch {
      set({ bookmarks: before });
    }
  },
  setBookmarkNote: async (messageId, note) => {
    const before = get().bookmarks;
    const current = before[messageId];
    if (!current) return;
    set({ bookmarks: { ...before, [messageId]: { ...current, note } } });
    try {
      const row = await api.setBookmarkNote(messageId, note);
      set((s) => ({ bookmarks: { ...s.bookmarks, [messageId]: row } }));
    } catch {
      set({ bookmarks: before });
    }
  },
  addReminder: async ({ note, remindAt, ref }) => {
    const target =
      ref && isSnowflake(ref.message_id) && isSnowflake(ref.channel_id)
        ? { channelId: ref.channel_id, messageId: ref.message_id }
        : {};
    const row = await api.createReminder({ note: note || ref?.preview || '', remindAt, ...target });
    set((s) => ({ reminders: [...s.reminders, row].sort(byDue) }));
  },
  cancelReminder: async (id) => {
    const before = get().reminders;
    set({ reminders: before.filter((r) => r.id !== id) });
    try {
      await api.cancelReminder(id);
    } catch {
      set({ reminders: before });
    }
  },
  addScheduled: (row) => set((s) => ({ scheduled: [...s.scheduled, row].sort(bySend) })),
  replaceScheduled: (row) =>
    set((s) => ({ scheduled: s.scheduled.map((r) => (r.id === row.id ? row : r)).sort(bySend) })),
  cancelScheduled: async (id) => {
    const before = get().scheduled;
    set({ scheduled: before.filter((r) => r.id !== id) });
    try {
      await api.cancelScheduled(id);
    } catch {
      set({ scheduled: before });
    }
  },
  reset: () => set({ ...EMPTY }),
}));
