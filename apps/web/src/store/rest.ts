// REST calls feeding the store (docs/05 section 4). Thin wrappers over apiFetch.
import { apiFetch } from '../api/client';
import { cmpId } from './reducer';
import type { Channel, Guild, Message, Role, User } from './types';

export const PAGE_SIZE = 50;

export interface RawMember {
  user: User;
  nick?: string | null;
  roles: string[];
  joined_at?: string;
}

export const fetchGuilds = () => apiFetch<Guild[]>('/users/@me/guilds');
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
  opts: { before?: string; limit?: number } = {},
): Promise<Message[]> {
  const q = new URLSearchParams({ limit: String(opts.limit ?? PAGE_SIZE) });
  if (opts.before) q.set('before', opts.before);
  const list = await apiFetch<Message[]>(`/channels/${channelId}/messages?${q}`);
  return [...list].reverse();
}

export const postMessage = (
  channelId: string,
  content: string,
  nonce: string,
  replyToId?: string | null,
) =>
  apiFetch<Message>(`/channels/${channelId}/messages`, {
    method: 'POST',
    json: {
      content,
      nonce,
      enforce_nonce: true,
      ...(replyToId
        ? { message_reference: { message_id: replyToId, fail_if_not_exists: false } }
        : {}),
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
}

export interface CreateChannelBody {
  name: string;
  /** 0 text, 2 voice, 4 category. */
  type?: number;
  parent_id?: string | null;
  topic?: string | null;
  permission_overwrites?: { id: string; type: number; allow?: string; deny?: string }[];
}

export type GuildWithChannels = Guild & { channels?: Channel[]; roles?: Role[] };

export const createGuild = (name: string) =>
  apiFetch<GuildWithChannels>('/guilds', { method: 'POST', json: { name }, retries: 0 });

export const getInvite = (code: string) =>
  apiFetch<Invite>(`/invites/${encodeURIComponent(code)}?with_counts=true`, { retries: 0 });
export const acceptInvite = (code: string) =>
  apiFetch<Invite>(`/invites/${encodeURIComponent(code)}`, { method: 'POST', json: {}, retries: 0 });
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
export const searchGuildMessages = (guildId: string, q: string, offset = 0) => {
  const params = new URLSearchParams({ q, limit: '25' });
  if (offset) params.set('offset', String(offset));
  return apiFetch<SearchResults>(`/guilds/${guildId}/messages/search?${params}`, { retries: 0 });
};

/** Messages around one id (jump to message). Oldest first. */
export async function fetchMessagesAround(channelId: string, messageId: string, limit = PAGE_SIZE) {
  const list = await apiFetch<Message[]>(
    `/channels/${channelId}/messages?around=${messageId}&limit=${limit}`,
  );
  return [...list].sort((a, b) => cmpId(a.id, b.id));
}

export const searchChannelMessages = (channelId: string, q: string, offset = 0) => {
  const params = new URLSearchParams({ q, limit: '25' });
  if (offset) params.set('offset', String(offset));
  return apiFetch<SearchResults>(`/channels/${channelId}/messages/search?${params}`, {
    retries: 0,
  });
};
