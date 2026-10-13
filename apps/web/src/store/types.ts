// FND-API-008: Discord-shaped entities held by the normalized store.
// Field names are snake_case to match the wire format (docs/05). IDs are snowflake strings.

export interface User {
  id: string;
  username: string;
  global_name?: string | null;
  avatar?: string | null;
  bot?: boolean;
}

export interface Guild {
  id: string;
  name: string;
  icon?: string | null;
  owner_id?: string;
  member_count?: number;
  /** True for READY stubs until GUILD_CREATE arrives. */
  unavailable?: boolean;
}

export interface Channel {
  id: string;
  /** Discord channel type (0 text, 1 DM, 2 voice, 4 category, 5 announcement, ...). */
  type: number;
  guild_id?: string;
  name?: string | null;
  topic?: string | null;
  parent_id?: string | null;
  position?: number;
  last_message_id?: string | null;
  /** Slowmode seconds. */
  rate_limit_per_user?: number;
  /** DM / group DM participants. */
  recipients?: User[];
}

export interface Role {
  id: string;
  guild_id: string;
  name: string;
  color: number;
  position: number;
  /** Permission bitfield as a decimal string. */
  permissions: string;
  hoist?: boolean;
}

export interface Member {
  guild_id: string;
  user_id: string;
  nick?: string | null;
  roles: string[];
  joined_at?: string;
}

export interface Message {
  id: string;
  channel_id: string;
  guild_id?: string;
  author: User;
  content: string;
  timestamp: string;
  edited_timestamp?: string | null;
  pinned?: boolean;
  nonce?: string | number | null;
  mentions?: User[];
  mention_everyone?: boolean;
  mention_roles?: string[];
  message_reference?: {
    message_id: string;
    channel_id?: string;
    guild_id?: string;
    /** 0 reply, 1 forward. */
    type?: number;
  } | null;
  reactions?: MessageReaction[];
  /** 0 default, 6 pin notice, 19 reply (Discord message types). */
  type?: number;
  /** 4 suppress embeds, 8192 voice message. */
  flags?: number;
  attachments?: RawAttachment[];
  embeds?: RawEmbed[];
  sticker_items?: { id: string; name: string; format_type: number }[];
  poll?: RawPoll | null;
  message_snapshots?: { message: Partial<Message> }[];
  /** Client-only: set while the message is optimistic. Absent on server messages. */
  local?: LocalState;
}

export interface RawAttachment {
  id: string;
  filename: string;
  size: number;
  url: string;
  content_type?: string | null;
  description?: string | null;
  width?: number | null;
  height?: number | null;
  duration_secs?: number | null;
  /** Base64 amplitude bytes (voice messages). */
  waveform?: string | null;
  expires_at?: string | null;
}

export interface RawEmbed {
  type?: string;
  title?: string;
  description?: string;
  url?: string;
  color?: number;
  timestamp?: string;
  footer?: { text: string; icon_url?: string };
  image?: { url?: string; width?: number; height?: number };
  thumbnail?: { url?: string; width?: number; height?: number };
  video?: { url?: string; width?: number; height?: number };
  provider?: { name?: string; url?: string };
  author?: { name: string; url?: string; icon_url?: string };
  fields?: { name: string; value: string; inline?: boolean }[];
}

export interface RawPoll {
  question: { text?: string };
  answers: {
    answer_id: number;
    poll_media: { text?: string; emoji?: { id?: string | null; name?: string | null } };
  }[];
  expiry?: string | null;
  allow_multiselect?: boolean;
  results?: {
    is_finalized: boolean;
    answer_counts: { id: number; count: number; me_voted: boolean }[];
  };
}

export interface MessageReaction {
  emoji: { id?: string | null; name?: string | null };
  count: number;
  /** The signed-in user reacted with this emoji. */
  me: boolean;
}

/** Optimistic-send state. `sending` -> replaced by the server message; `failed` stays until retry/discard. */
export interface LocalState {
  status: 'sending' | 'failed';
  nonce: string;
  error?: string;
}

export type PresenceStatus = 'online' | 'idle' | 'dnd' | 'offline';

export interface Presence {
  user_id: string;
  status: PresenceStatus;
  activities: unknown[];
}

export interface ReadState {
  channel_id: string;
  last_message_id: string | null;
  mention_count: number;
}

export interface TypingEntry {
  user_id: string;
  /** Epoch ms after which the entry is stale (Discord shows ~10 s). */
  expires_at: number;
}

/** Ordered window of loaded messages for one channel (ascending snowflake order). */
export interface MessageWindow {
  ids: string[];
  /** Optimistic messages (ids are `pending:<nonce>`), oldest first, rendered after `ids`. */
  pending: string[];
  /** Older history exists on the server that is not loaded. */
  hasMoreBefore: boolean;
  /** `ids` ends at the newest message, so live MESSAGE_CREATE can be appended. */
  atLatest: boolean;
  loading: boolean;
  error: string | null;
}

export type ConnectionStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'closed';
