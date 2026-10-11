// REST calls feeding the store (docs/05 section 4). Thin wrappers over apiFetch.
import { apiFetch } from '../api/client';
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

export const postMessage = (channelId: string, content: string, nonce: string) =>
  apiFetch<Message>(`/channels/${channelId}/messages`, {
    method: 'POST',
    json: { content, nonce, enforce_nonce: true },
    retries: 0,
  });

export const postTyping = (channelId: string) =>
  apiFetch<void>(`/channels/${channelId}/typing`, { method: 'POST', json: {}, retries: 0 });
