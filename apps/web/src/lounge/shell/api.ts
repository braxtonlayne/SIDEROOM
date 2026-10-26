// REST wrappers used by the shell and sidebar modules (leave, DMs, friends, presence). Mutations use
// retries: 0 so failures surface to the person instead of being replayed.
import { apiFetch } from '../../api/client';
import { useIgnores } from './ignores';

const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });

/** Leave a guild (the owner must transfer ownership first; the server answers 400/403). */
export const leaveGuild = (guildId: string) =>
  send<void>(`/users/@me/guilds/${encodeURIComponent(guildId)}`, 'DELETE');

export interface UserRef {
  id: string;
  username: string;
  global_name?: string | null;
  avatar?: string | null;
  bot?: boolean;
}

export interface RelationshipRow {
  id: string;
  /** 1 friend, 2 blocked, 3 incoming request, 4 outgoing request. */
  type: number;
  nickname?: string | null;
  since: string;
  user: UserRef;
}
export const REL = { FRIEND: 1, BLOCKED: 2, INCOMING: 3, OUTGOING: 4 } as const;

export const listRelationships = () =>
  apiFetch<RelationshipRow[]>('/users/@me/relationships', { retries: 0 });
/** Friend request by username (case-insensitive). */
export const addFriendByUsername = (username: string) =>
  send<void>('/users/@me/relationships', 'POST', { username });
/** Accept an incoming request (or send one by id). */
export const acceptFriend = (userId: string) =>
  send<void>(`/users/@me/relationships/${encodeURIComponent(userId)}`, 'PUT', { type: 1 });
export const blockUser = (userId: string) =>
  send<void>(`/users/@me/relationships/${encodeURIComponent(userId)}`, 'PUT', { type: 2 });
/** Remove a friend, cancel or decline a request, or unblock. */
export const removeRelationship = (userId: string) =>
  send<void>(`/users/@me/relationships/${encodeURIComponent(userId)}`, 'DELETE');
export const ignoreUser = (userId: string) =>
  send<void>(`/users/@me/ignores/${encodeURIComponent(userId)}`, 'PUT', {}).then(() =>
    useIgnores.getState().add(userId),
  );

export interface DmObject {
  id: string;
  type: number;
  name?: string | null;
  last_message_id?: string | null;
  recipients: UserRef[];
  is_message_request?: boolean | null;
  /** With `is_message_request`: `server_filter` or `new_account` when a shared server's Protection settings sent it here (D-224). */
  sr_request_reason?: 'server_filter' | 'new_account' | 'spam' | null;
}
/** Open (or find) a 1:1 DM, or create a group DM when more than one id is given. */
export const createDm = (recipientIds: string[]) =>
  send<DmObject>(
    '/users/@me/channels',
    'POST',
    recipientIds.length === 1 ? { recipient_id: recipientIds[0] } : { recipients: recipientIds },
  );
/**
 * Add a friend to a group DM (204), or to a 1:1 DM, which spawns a new group and answers with it (200).
 * `PUT /channels/{id}/recipients/{user_id}`.
 */
export const addRecipient = (channelId: string, userId: string) =>
  send<DmObject | undefined>(
    `/channels/${encodeURIComponent(channelId)}/recipients/${encodeURIComponent(userId)}`,
    'PUT',
    {},
  );
/** Close a DM (hides it from the list). */
export const closeDm = (channelId: string) =>
  send<void>(`/users/@me/channels/${encodeURIComponent(channelId)}`, 'DELETE');
export const respondToMessageRequest = (
  channelId: string,
  action: 'accepted' | 'ignored' | 'spam',
) =>
  send<void>(`/users/@me/channels/${encodeURIComponent(channelId)}/request`, 'PUT', {
    state: action,
  });

export interface PresenceBody {
  status?: 'online' | 'idle' | 'dnd' | 'invisible';
  duration_seconds?: number | null;
  custom_status?: {
    text?: string | null;
    emoji_name?: string | null;
    expires_at?: string | null;
  } | null;
}
export interface PresenceObject {
  status: string;
  custom_status?: { text?: string | null; emoji_name?: string | null } | null;
  status_expires_at?: string | null;
}
export const getPresence = () => apiFetch<PresenceObject>('/users/@me/presence', { retries: 0 });
export const patchPresence = (body: PresenceBody) =>
  send<PresenceObject>('/users/@me/presence', 'PATCH', body);

// ---- channels ----

export const reorderChannelsRequest = (
  guildId: string,
  moves: { id: string; position: number; parent_id: string | null }[],
) => send<void>(`/guilds/${encodeURIComponent(guildId)}/channels`, 'PATCH', moves);
export const deleteChannelRequest = (channelId: string) =>
  send<unknown>(`/channels/${encodeURIComponent(channelId)}`, 'DELETE');
