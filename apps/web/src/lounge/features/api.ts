// Typed fetchers for the feature-module routes (crates/sr-api/src/routes/userfeat). Components never build URLs.
import { apiFetch } from '../../api/client';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

// ---- scheduled messages (FTR-SCH-001)

export interface ScheduledMessage {
  id: string;
  channel_id: string;
  guild_id: string | null;
  content: string;
  /** ISO 8601 time the message goes out. */
  send_at: string;
  status: 'pending' | 'sent' | 'failed' | 'cancelled';
  failure: string | null;
  message_id: string | null;
  created_at: string;
}

export const scheduleMessage = (channelId: string, content: string, sendAt: Date) =>
  apiFetch<ScheduledMessage>(`/channels/${enc(channelId)}/scheduled-messages`, {
    method: 'POST',
    json: { content, send_at: sendAt.toISOString() },
    ...NO_RETRY,
  });

export const listScheduled = (all = false) =>
  apiFetch<ScheduledMessage[]>(`/users/@me/scheduled-messages${all ? '?all=true' : ''}`, NO_RETRY);

export const editScheduled = (id: string, patch: { content?: string; sendAt?: Date }) =>
  apiFetch<ScheduledMessage>(`/users/@me/scheduled-messages/${enc(id)}`, {
    method: 'PATCH',
    json: {
      ...(patch.content !== undefined ? { content: patch.content } : {}),
      ...(patch.sendAt ? { send_at: patch.sendAt.toISOString() } : {}),
    },
    ...NO_RETRY,
  });

export const cancelScheduled = (id: string) =>
  apiFetch<void>(`/users/@me/scheduled-messages/${enc(id)}`, { method: 'DELETE', ...NO_RETRY });

// ---- reminders (FTR-REM-001)

export interface ReminderRow {
  id: string;
  channel_id: string | null;
  guild_id: string | null;
  message_id: string | null;
  note: string;
  remind_at: string;
  status: 'pending' | 'sent' | 'failed' | 'cancelled';
  created_at: string;
}

export interface NewReminder {
  note?: string;
  remindAt: Date;
  channelId?: string;
  messageId?: string;
}

export const createReminder = (r: NewReminder) =>
  apiFetch<ReminderRow>(`/users/@me/reminders`, {
    method: 'POST',
    json: {
      note: r.note ?? '',
      remind_at: r.remindAt.toISOString(),
      ...(r.channelId && r.messageId ? { channel_id: r.channelId, message_id: r.messageId } : {}),
    },
    ...NO_RETRY,
  });

export const listReminders = (all = false) =>
  apiFetch<ReminderRow[]>(`/users/@me/reminders${all ? '?all=true' : ''}`, NO_RETRY);

export const cancelReminder = (id: string) =>
  apiFetch<void>(`/users/@me/reminders/${enc(id)}`, { method: 'DELETE', ...NO_RETRY });

// ---- bookmarks (FTR-BKM-001)

export interface BookmarkRow {
  message_id: string;
  channel_id: string;
  guild_id: string | null;
  note: string;
  created_at: string;
}

/** The slice of a rendered message the panel shows. */
export interface BookmarkedMessage {
  id: string;
  channel_id: string;
  content: string;
  timestamp: string;
  author: { id: string; username: string; global_name?: string | null };
  attachments?: unknown[];
}

export interface BookmarkEntry {
  bookmark: BookmarkRow;
  message: BookmarkedMessage | null;
}

export const saveBookmark = (channelId: string, messageId: string, note = '') =>
  apiFetch<BookmarkRow>(`/channels/${enc(channelId)}/messages/${enc(messageId)}/bookmark`, {
    method: 'PUT',
    json: { note },
    ...NO_RETRY,
  });

export const listBookmarks = (opts: { before?: string; limit?: number } = {}) => {
  const q = new URLSearchParams();
  if (opts.before) q.set('before', opts.before);
  if (opts.limit) q.set('limit', String(opts.limit));
  const s = q.toString();
  return apiFetch<BookmarkEntry[]>(`/users/@me/bookmarks${s ? `?${s}` : ''}`, NO_RETRY);
};

/** Bookmarks without their messages: what the message menus need to show a filled star. */
export const listBookmarkIds = () =>
  apiFetch<BookmarkEntry[]>(`/users/@me/bookmarks?limit=100&messages=false`, NO_RETRY);

export const setBookmarkNote = (messageId: string, note: string) =>
  apiFetch<BookmarkRow>(`/users/@me/bookmarks/${enc(messageId)}`, {
    method: 'PATCH',
    json: { note },
    ...NO_RETRY,
  });

export const removeBookmark = (messageId: string) =>
  apiFetch<void>(`/users/@me/bookmarks/${enc(messageId)}`, { method: 'DELETE', ...NO_RETRY });

// ---- birthdays (FTR-BDAY-001)

export interface Birthday {
  month: number;
  day: number;
  year: number | null;
  visibility: 'servers' | 'hidden';
}

export const getBirthday = () => apiFetch<Birthday>(`/users/@me/birthday`, NO_RETRY);
export const setBirthday = (b: Birthday) =>
  apiFetch<Birthday>(`/users/@me/birthday`, { method: 'PUT', json: b, ...NO_RETRY });
export const clearBirthday = () =>
  apiFetch<void>(`/users/@me/birthday`, { method: 'DELETE', ...NO_RETRY });

// ---- modules switched on in a server

export interface ModuleFlags {
  modules: Record<string, boolean>;
}

export const getModules = (guildId: string) =>
  apiFetch<ModuleFlags>(`/guilds/${enc(guildId)}/modules`, NO_RETRY);

// ---- leveling (FTR-LVL-001)

export interface RankCard {
  user_id: string;
  username: string;
  display_name: string | null;
  xp: number;
  level: number;
  level_xp: number;
  level_span: number;
  rank: number | null;
  messages: number;
  opted_out: boolean;
}

export interface LeaderboardEntry {
  rank: number;
  user_id: string;
  username: string;
  display_name: string | null;
  avatar_key: string | null;
  xp: number;
  level: number;
}

export const getRank = (guildId: string, userId = '@me') =>
  apiFetch<RankCard>(`/guilds/${enc(guildId)}/leveling/rank/${enc(userId)}`, NO_RETRY);

export const getLeaderboard = (guildId: string, offset = 0, limit = 25) =>
  apiFetch<LeaderboardEntry[]>(
    `/guilds/${enc(guildId)}/leveling/leaderboard?limit=${limit}&offset=${offset}`,
    NO_RETRY,
  );

export const setLevelingEnabled = (guildId: string, enabled: boolean) =>
  apiFetch<void>(`/users/@me/guilds/${enc(guildId)}/leveling`, {
    method: 'PUT',
    json: { enabled },
    ...NO_RETRY,
  });

// ---- poll templates (FTR-POLLT-001)

export interface PollTemplate {
  id: string;
  name: string;
  question: string;
  answers: string[];
  allow_multiselect: boolean;
  duration_hours: number;
  created_by: string | null;
  created_at: string;
}

export interface PollTemplateInput {
  name: string;
  question: string;
  answers: string[];
  allow_multiselect: boolean;
  duration_hours: number;
}

export const listPollTemplates = (guildId: string) =>
  apiFetch<PollTemplate[]>(`/guilds/${enc(guildId)}/poll-templates`, NO_RETRY);
export const createPollTemplate = (guildId: string, t: PollTemplateInput) =>
  apiFetch<PollTemplate>(`/guilds/${enc(guildId)}/poll-templates`, {
    method: 'POST',
    json: t,
    ...NO_RETRY,
  });
export const deletePollTemplate = (guildId: string, id: string) =>
  apiFetch<void>(`/guilds/${enc(guildId)}/poll-templates/${enc(id)}`, {
    method: 'DELETE',
    ...NO_RETRY,
  });

// ---- RSS feeds (FTR-RSS-001)

export interface RssFeed {
  id: string;
  channel_id: string;
  url: string;
  title: string;
  include_keywords: string[];
  exclude_keywords: string[];
  poll_interval_secs: number;
  enabled: boolean;
  last_checked_at: string | null;
  last_status: string;
  failures: number;
  created_at: string;
}

export interface NewRssFeed {
  url: string;
  channel_id: string;
  include_keywords: string[];
  exclude_keywords: string[];
  poll_interval_secs?: number;
}

export const listFeeds = (guildId: string) =>
  apiFetch<RssFeed[]>(`/guilds/${enc(guildId)}/rss-feeds`, NO_RETRY);
export const addFeed = (guildId: string, f: NewRssFeed) =>
  apiFetch<RssFeed>(`/guilds/${enc(guildId)}/rss-feeds`, { method: 'POST', json: f, ...NO_RETRY });
export const patchFeed = (guildId: string, id: string, patch: { enabled?: boolean }) =>
  apiFetch<RssFeed>(`/guilds/${enc(guildId)}/rss-feeds/${enc(id)}`, {
    method: 'PATCH',
    json: patch,
    ...NO_RETRY,
  });
export const deleteFeed = (guildId: string, id: string) =>
  apiFetch<void>(`/guilds/${enc(guildId)}/rss-feeds/${enc(id)}`, {
    method: 'DELETE',
    ...NO_RETRY,
  });

// ---- suggestions (FTR-SUGG-001)

export type SuggestionStatus = 'open' | 'considering' | 'approved' | 'denied' | 'implemented';

export interface Suggestion {
  id: string;
  channel_id: string;
  message_id: string;
  author_id: string;
  content: string;
  status: SuggestionStatus;
  response: string;
  responder_id: string | null;
  created_at: string;
  updated_at: string;
}

export const listSuggestions = (guildId: string, status?: SuggestionStatus) =>
  apiFetch<Suggestion[]>(
    `/guilds/${enc(guildId)}/suggestions${status ? `?status=${status}` : ''}`,
    NO_RETRY,
  );
export const submitSuggestion = (guildId: string, content: string) =>
  apiFetch<Suggestion>(`/guilds/${enc(guildId)}/suggestions`, {
    method: 'POST',
    json: { content },
    ...NO_RETRY,
  });
export const respondToSuggestion = (
  guildId: string,
  id: string,
  status: SuggestionStatus,
  response: string,
) =>
  apiFetch<Suggestion>(`/guilds/${enc(guildId)}/suggestions/${enc(id)}`, {
    method: 'PATCH',
    json: { status, response },
    ...NO_RETRY,
  });

// ---- tickets (FTR-TICK-001)

export interface Ticket {
  id: string;
  opener_id: string;
  thread_id: string;
  subject: string;
  status: 'open' | 'closed';
  closed_by: string | null;
  closed_at: string | null;
  created_at: string;
}

export const listTickets = (guildId: string, status?: 'open' | 'closed') =>
  apiFetch<Ticket[]>(
    `/guilds/${enc(guildId)}/tickets${status ? `?status=${status}` : ''}`,
    NO_RETRY,
  );
export const openTicket = (guildId: string, subject: string) =>
  apiFetch<Ticket>(`/guilds/${enc(guildId)}/tickets`, {
    method: 'POST',
    json: { subject },
    ...NO_RETRY,
  });
export const closeTicket = (guildId: string, id: string) =>
  apiFetch<Ticket>(`/guilds/${enc(guildId)}/tickets/${enc(id)}/close`, {
    method: 'POST',
    ...NO_RETRY,
  });
