// REST calls feeding the store (docs/05 section 4). Thin wrappers over apiFetch.
import { apiFetch } from '../api/client';
import { cmpId } from './reducer';
import type { Channel, Guild, Message, Role, User, UserSettingsBlob } from './types';
export type { UserSettingsBlob };

export const PAGE_SIZE = 50;

export interface RawMember {
  user: User;
  nick?: string | null;
  roles: string[];
  joined_at?: string;
}

export const fetchGuilds = () => apiFetch<Guild[]>('/users/@me/guilds');
export const fetchDmChannels = () => apiFetch<Channel[]>('/users/@me/channels');
export const fetchGuild = (id: string) => apiFetch<Guild>(`/guilds/${id}`);
export const fetchChannels = (guildId: string) =>
  apiFetch<Channel[]>(`/guilds/${guildId}/channels`);
export const fetchRoles = (guildId: string) =>
  apiFetch<Omit<Role, 'guild_id'>[]>(`/guilds/${guildId}/roles`);
export const fetchMembers = (guildId: string, limit = 1000) =>
  apiFetch<RawMember[]>(`/guilds/${guildId}/members?limit=${limit}`);

/** The server returns newest first; this returns oldest first. */
export async function fetchMessages(
  channelId: string,
  opts: { before?: string; after?: string; limit?: number } = {},
): Promise<Message[]> {
  const q = new URLSearchParams({ limit: String(opts.limit ?? PAGE_SIZE) });
  if (opts.before) q.set('before', opts.before);
  if (opts.after) q.set('after', opts.after);
  const list = await apiFetch<Message[]>(`/channels/${channelId}/messages?${q}`);
  return opts.after ? [...list].sort((a, b) => cmpId(a.id, b.id)) : [...list].reverse();
}

/** Optional parts of a new message (uploaded files, a poll, reply ping control). */
export interface PostExtras {
  attachments?: {
    id: string;
    uploaded_filename: string;
    description?: string | null;
    duration_secs?: number | null;
    waveform?: string | null;
  }[];
  poll?: {
    question: { text: string };
    answers: { poll_media: { text: string; emoji?: { name: string } } }[];
    duration: number;
    allow_multiselect: boolean;
  };
  allow_reply_ping?: boolean;
  flags?: number;
  sticker_ids?: string[];
  tts?: boolean;
}

export const postMessage = (
  channelId: string,
  content: string,
  nonce: string,
  replyToId?: string | null,
  extras: PostExtras = {},
) =>
  apiFetch<Message>(`/channels/${channelId}/messages`, {
    method: 'POST',
    json: {
      ...(content ? { content } : {}),
      nonce,
      enforce_nonce: true,
      ...(replyToId
        ? { message_reference: { message_id: replyToId, fail_if_not_exists: false } }
        : {}),
      ...(extras.attachments?.length ? { attachments: extras.attachments } : {}),
      ...(extras.poll ? { poll: extras.poll } : {}),
      ...(extras.flags ? { flags: extras.flags } : {}),
      ...(extras.sticker_ids?.length ? { sticker_ids: extras.sticker_ids } : {}),
      ...(extras.tts ? { tts: true } : {}),
      ...(replyToId && extras.allow_reply_ping === false
        ? { allowed_mentions: { replied_user: false, parse: ['users', 'roles', 'everyone'] } }
        : {}),
    },
    retries: 0,
  });

export const deleteMessage = (channelId: string, messageId: string) =>
  apiFetch<void>(`/channels/${channelId}/messages/${messageId}`, {
    method: 'DELETE',
    retries: 0,
  });

export const pinMessage = (channelId: string, messageId: string) =>
  apiFetch<void>(`/channels/${channelId}/pins/${messageId}`, { method: 'PUT', retries: 0 });
export const unpinMessage = (channelId: string, messageId: string) =>
  apiFetch<void>(`/channels/${channelId}/pins/${messageId}`, { method: 'DELETE', retries: 0 });

const pollPath = (channelId: string, messageId: string) =>
  `/channels/${channelId}/polls/${messageId}`;
export const votePoll = (channelId: string, messageId: string, answerId: number) =>
  apiFetch<void>(`${pollPath(channelId, messageId)}/answers/${answerId}/voters/@me`, {
    method: 'PUT',
    retries: 0,
  });
export const unvotePoll = (channelId: string, messageId: string, answerId: number) =>
  apiFetch<void>(`${pollPath(channelId, messageId)}/answers/${answerId}/voters/@me`, {
    method: 'DELETE',
    retries: 0,
  });
export const endPoll = (channelId: string, messageId: string) =>
  apiFetch<Message>(`${pollPath(channelId, messageId)}/expire`, {
    method: 'POST',
    json: {},
    retries: 0,
  });

export const forwardMessage = (
  targetChannelId: string,
  source: { channelId: string; messageId: string; guildId?: string },
  nonce: string,
) =>
  apiFetch<Message>(`/channels/${targetChannelId}/messages`, {
    method: 'POST',
    json: {
      nonce,
      enforce_nonce: true,
      message_reference: {
        type: 1,
        message_id: source.messageId,
        channel_id: source.channelId,
        ...(source.guildId ? { guild_id: source.guildId } : {}),
      },
    },
    retries: 0,
  });

export const editMessage = (channelId: string, messageId: string, content: string) =>
  apiFetch<Message>(`/channels/${channelId}/messages/${messageId}`, {
    method: 'PATCH',
    json: { content },
    retries: 0,
  });

/** Read-state ack (Q128). The route is not in the API router yet; the call fails until it lands. */
export const ackMessage = (channelId: string, messageId: string) =>
  apiFetch<void>(`/channels/${channelId}/messages/${messageId}/ack`, {
    method: 'POST',
    json: {},
    retries: 0,
  });

/** "Mark as unread": the read marker is set to exactly `messageId` (the message before the unread one). */
export const markMessageUnread = (channelId: string, messageId: string) =>
  apiFetch<void>(`/channels/${channelId}/messages/${messageId}/ack`, {
    method: 'POST',
    json: { manual: true, mention_count: 0 },
    retries: 0,
  });

const reactionPath = (channelId: string, messageId: string, emoji: string) =>
  `/channels/${channelId}/messages/${messageId}/reactions/${encodeURIComponent(emoji)}/@me`;

export const addReaction = (channelId: string, messageId: string, emoji: string) =>
  apiFetch<void>(reactionPath(channelId, messageId, emoji), { method: 'PUT', retries: 0 });

export const removeReaction = (channelId: string, messageId: string, emoji: string) =>
  apiFetch<void>(reactionPath(channelId, messageId, emoji), { method: 'DELETE', retries: 0 });

export const postTyping = (channelId: string) =>
  apiFetch<void>(`/channels/${channelId}/typing`, { method: 'POST', json: {}, retries: 0 });

// ---- Server flows (invites, create guild/channel, search) ----

export interface Invite {
  code: string;
  guild: { id: string; name: string; icon?: string | null };
  channel: { id: string; name?: string | null; type: number };
  inviter?: { id: string; username?: string; global_name?: string | null };
  uses: number;
  max_uses: number;
  /** Seconds; 0 = never expires. */
  max_age: number;
  temporary?: boolean;
  created_at: string;
  expires_at?: string | null;
  approximate_member_count?: number;
  approximate_presence_count?: number;
}

export interface CreateInviteOptions {
  /** Seconds until expiry, 0 to 604800. */
  max_age?: number;
  /** 0 (unlimited) to 100. */
  max_uses?: number;
  /** "Copy link": the invite leaves the platform, so the server defaults to a single use (D-022). */
  external?: boolean;
}

export interface CreateChannelBody {
  name: string;
  /** 0 text, 2 voice, 4 category. */
  type?: number;
  parent_id?: string | null;
  topic?: string | null;
  permission_overwrites?: { id: string; type: number; allow?: string; deny?: string }[];
  /** The "Watch party room" preset (D-208): a voice channel plus a persistent party; needs a title. */
  sr_watch_party_room?: boolean;
  sr_watch_party_title_id?: string;
  sr_watch_party_episode_id?: string;
}

export type GuildWithChannels = Guild & { channels?: Channel[]; roles?: Role[] };

/** `template` is a built-in template id or a template code; the server builds the whole layout in one go. */
export const createGuild = (name: string, template?: string) =>
  apiFetch<GuildWithChannels>('/guilds', {
    method: 'POST',
    json: template ? { name, template } : { name },
    retries: 0,
  });

/** A starter layout shipped with the server (Create a Server wizard, LNG-GLD-015). */
export interface BuiltinTemplate {
  id: string;
  name: string;
  /** One line shown under the name. */
  description: string;
}
export const listBuiltinTemplates = () =>
  apiFetch<BuiltinTemplate[]>('/guilds/templates/builtin', { retries: 0 });

export const getInvite = (code: string) =>
  apiFetch<Invite>(`/invites/${encodeURIComponent(code)}?with_counts=true`, { retries: 0 });
export const acceptInvite = (code: string) =>
  apiFetch<Invite>(`/invites/${encodeURIComponent(code)}`, {
    method: 'POST',
    json: {},
    retries: 0,
  });
export const createInvite = (channelId: string, opts: CreateInviteOptions = {}) =>
  apiFetch<Invite>(`/channels/${channelId}/invites`, { method: 'POST', json: opts, retries: 0 });
export const listGuildInvites = (guildId: string) =>
  apiFetch<Invite[]>(`/guilds/${guildId}/invites`, { retries: 0 });
export const listChannelInvites = (channelId: string) =>
  apiFetch<Invite[]>(`/channels/${channelId}/invites`, { retries: 0 });
export const revokeInvite = (code: string) =>
  apiFetch<Invite>(`/invites/${encodeURIComponent(code)}`, { method: 'DELETE', retries: 0 });

export const createChannel = (guildId: string, body: CreateChannelBody) =>
  apiFetch<Channel>(`/guilds/${guildId}/channels`, { method: 'POST', json: body, retries: 0 });

/** One search hit: the message plus the server's highlighted snippet (byte ranges into `text`). */
export type SearchHit = Message & {
  hit?: boolean;
  sr_snippet?: { text: string; highlights: [number, number][] };
};
export interface SearchResults {
  total_results: number;
  /** Each result is a one-message group. */
  messages: SearchHit[][];
}
export interface SearchOptions {
  offset?: number;
  /** Extra query params, e.g. sort_by / sort_order. */
  extra?: Record<string, string>;
}
const searchParams = (q: string, o: SearchOptions) => {
  const params = new URLSearchParams({ q, limit: '25', ...o.extra });
  if (o.offset) params.set('offset', String(o.offset));
  return params;
};
export const searchGuildMessages = (guildId: string, q: string, o: SearchOptions | number = {}) =>
  apiFetch<SearchResults>(
    `/guilds/${guildId}/messages/search?${searchParams(q, typeof o === 'number' ? { offset: o } : o)}`,
    { retries: 0 },
  );

/** Messages around one id (jump to message). Oldest first. */
export async function fetchMessagesAround(channelId: string, messageId: string, limit = PAGE_SIZE) {
  const list = await apiFetch<Message[]>(
    `/channels/${channelId}/messages?around=${messageId}&limit=${limit}`,
  );
  return [...list].sort((a, b) => cmpId(a.id, b.id));
}

export const searchChannelMessages = (
  channelId: string,
  q: string,
  o: SearchOptions | number = {},
) =>
  apiFetch<SearchResults>(
    `/channels/${channelId}/messages/search?${searchParams(q, typeof o === 'number' ? { offset: o } : o)}`,
    { retries: 0 },
  );

/** The synced per-user settings blob (`GET/PATCH /users/@me/settings`, RFC 7386 merge patch). */
export const fetchUserSettings = () =>
  apiFetch<UserSettingsBlob>('/users/@me/settings', { retries: 0 });
export const patchUserSettings = (settings: Record<string, unknown>) =>
  apiFetch<UserSettingsBlob>('/users/@me/settings', {
    method: 'PATCH',
    json: { settings },
    retries: 0,
  });
