/** Wire types a bot handles. Extra fields the server sends stay on the object. */

export type Snowflake = string;

/** Intent bits (Discord positions). */
export const Intents = {
  Guilds: 1 << 0,
  GuildMembers: 1 << 1,
  GuildModeration: 1 << 2,
  GuildExpressions: 1 << 3,
  GuildVoiceStates: 1 << 7,
  GuildPresences: 1 << 8,
  GuildMessages: 1 << 9,
  GuildMessageReactions: 1 << 10,
  GuildMessageTyping: 1 << 11,
  MessageContent: 1 << 15,
} as const;

/** Guilds and guild messages, no privileged intent. */
export const DEFAULT_INTENTS = Intents.Guilds | Intents.GuildMessages;

export interface User {
  id: Snowflake;
  username?: string;
  global_name?: string | null;
  avatar?: string | null;
  bot?: boolean;
}

export interface Message {
  id: Snowflake;
  channel_id: Snowflake;
  guild_id?: Snowflake;
  author: User;
  /** Empty without the MESSAGE_CONTENT intent (except DMs, mentions and own messages). */
  content: string;
  mentions?: User[];
  [extra: string]: unknown;
}

export interface Role {
  id: Snowflake;
  name?: string;
  permissions?: string;
}

export interface Channel {
  id: Snowflake;
  name?: string | null;
  type?: number;
  guild_id?: Snowflake;
}

export interface Guild {
  id: Snowflake;
  name?: string;
  unavailable?: boolean;
  channels?: Channel[];
  roles?: Role[];
}

export interface Ready {
  session_id: string;
  resume_gateway_url?: string;
  user: User;
  application?: { id: Snowflake; flags?: number };
  guilds?: Guild[];
}

export interface RawInteraction {
  id: Snowflake;
  application_id: Snowflake;
  type: number;
  data?: Record<string, unknown> & { name?: string; custom_id?: string; options?: RawOption[] };
  guild_id?: Snowflake;
  channel_id?: Snowflake;
  member?: { user?: User } & Record<string, unknown>;
  token: string;
  message?: Message;
  app_permissions?: string;
}

export interface RawOption {
  type: number;
  name: string;
  value?: string | number | boolean;
  focused?: boolean;
  options?: RawOption[];
}

export const InteractionType = {
  Ping: 1,
  ApplicationCommand: 2,
  MessageComponent: 3,
  Autocomplete: 4,
  ModalSubmit: 5,
} as const;

export const CallbackType = {
  Pong: 1,
  ChannelMessage: 4,
  DeferredChannelMessage: 5,
  DeferredUpdateMessage: 6,
  UpdateMessage: 7,
  Autocomplete: 8,
  Modal: 9,
} as const;

/** Message flag: only the invoking user sees the response. */
export const EPHEMERAL = 64;

export interface MessageOptions {
  content?: string;
  embeds?: unknown[];
  components?: unknown[];
  /** Interaction responses only. */
  ephemeral?: boolean;
}
