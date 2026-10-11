/**
 * Lounge data seam.
 *
 * Every Lounge component reads chat state ONLY through the hooks exported here. Today they are backed by a
 * React context filled by `MockDataProvider` (./mockData.tsx). When the gateway client and normalized store
 * land (apps/web/src/store/index.ts: useGuilds, useGuild, useChannels, useMessages, useMembers, usePresence,
 * useTyping), swap by re-implementing the hook bodies in this file (or re-exporting them from the store) and
 * replacing `<MockDataProvider>` in routes/lounge.tsx. Nothing else imports the mock.
 *
 * Ids are decimal-string snowflakes, like everywhere else on the wire.
 */
import { createContext, useContext } from 'react';

export type Snowflake = string;
export type PresenceStatus = 'online' | 'idle' | 'dnd' | 'offline';

export interface LoungeUser {
  id: Snowflake;
  username: string;
  displayName: string;
  avatarUrl?: string | null;
  bot?: boolean;
  /** Server-tag chip (short text), shown next to the name. */
  tag?: string;
}

export interface Activity {
  kind: 'playing' | 'watching' | 'reading' | 'listening' | 'custom';
  text: string;
}

export interface Presence {
  status: PresenceStatus;
  activity?: Activity;
}

export interface GuildPermissions {
  manageGuild: boolean;
  manageChannels: boolean;
  createInvite: boolean;
  moderate: boolean;
}

export interface Guild {
  id: Snowflake;
  name: string;
  iconUrl?: string | null;
  ownerId: Snowflake;
  /** Any non-muted channel unread. */
  unread: boolean;
  mentionCount: number;
  /** Staff Server gets a special frame (LNG-RAIL-009). */
  isStaff?: boolean;
  /** User is connected to a voice channel here (LNG-RAIL-013). */
  voiceConnected?: boolean;
  /** Joined in the last 24h (LNG-RAIL-014). */
  isNew?: boolean;
  /** Server accent hex for theming, if the owner set one. */
  accent?: string | null;
  permissions: GuildPermissions;
}

/** Ordered rail layout: either a bare guild or a folder of guilds. */
export type RailEntry =
  | { kind: 'guild'; guildId: Snowflake }
  | { kind: 'folder'; id: string; name: string | null; color: string; guildIds: Snowflake[] };

export type ChannelType =
  'text' | 'announcement' | 'voice' | 'stage' | 'forum' | 'media' | 'category' | 'rules';

export interface Channel {
  id: Snowflake;
  guildId: Snowflake;
  type: ChannelType;
  name: string;
  topic?: string | null;
  parentId?: Snowflake | null;
  position: number;
  unread: boolean;
  mentionCount: number;
  muted?: boolean;
  nsfw?: boolean;
  locked?: boolean;
  /** Last message the viewer has read; drives the "NEW" divider. */
  lastReadMessageId?: Snowflake | null;
  /** Viewer may post here. */
  canSend: boolean;
  userLimit?: number;
  slowmodeSeconds?: number;
}

export interface DmChannel {
  id: Snowflake;
  /** Recipient ids (excluding the viewer); more than one means a group DM. */
  recipientIds: Snowflake[];
  name?: string | null;
  lastMessageAt: string;
  unread: boolean;
  mentionCount: number;
}

export interface VoiceMember {
  userId: Snowflake;
  speaking?: boolean;
  muted?: boolean;
  deafened?: boolean;
  video?: boolean;
  streaming?: boolean;
}

export interface Role {
  id: Snowflake;
  name: string;
  color?: string | null;
  /** Hoisted roles get their own member-list group. */
  hoist: boolean;
  position: number;
}

export interface Member {
  user: LoungeUser;
  guildId: Snowflake;
  nick?: string | null;
  roleIds: Snowflake[];
}

export interface Attachment {
  id: Snowflake;
  filename: string;
  size: number;
  contentType: string;
  url: string;
  width?: number;
  height?: number;
  spoiler?: boolean;
}

export interface Reaction {
  emoji: string;
  count: number;
  me: boolean;
}

export interface Message {
  id: Snowflake;
  channelId: Snowflake;
  author: LoungeUser;
  content: string;
  /** ISO timestamp. */
  createdAt: string;
  editedAt?: string | null;
  replyToId?: Snowflake | null;
  attachments?: Attachment[];
  reactions?: Reaction[];
  /** Local echo state for optimistic sends. */
  pending?: boolean;
  failed?: boolean;
}

export interface MessagesState {
  /** Ascending by time. */
  messages: Message[];
  hasMore: boolean;
  loadingOlder: boolean;
  loading: boolean;
}

export interface LoungeActions {
  sendMessage(channelId: Snowflake, content: string, replyToId?: Snowflake | null): void;
  editMessage(channelId: Snowflake, messageId: Snowflake, content: string): void;
  loadOlder(channelId: Snowflake): void;
  markRead(channelId: Snowflake): void;
  markGuildRead(guildId: Snowflake): void;
  toggleReaction(channelId: Snowflake, messageId: Snowflake, emoji: string): void;
  /** Throttled "I am typing" signal. */
  startTyping(channelId: Snowflake): void;
}

/** The whole readable surface. Real store: build the same shape from normalized selectors. */
export interface LoungeData {
  me: LoungeUser;
  guilds: Guild[];
  rail: RailEntry[];
  channelsByGuild: Record<Snowflake, Channel[]>;
  dms: DmChannel[];
  users: Record<Snowflake, LoungeUser>;
  rolesByGuild: Record<Snowflake, Role[]>;
  membersByGuild: Record<Snowflake, Member[]>;
  presence: Record<Snowflake, Presence>;
  typing: Record<Snowflake, Snowflake[]>;
  voice: Record<Snowflake, VoiceMember[]>;
  messages: Record<Snowflake, MessagesState>;
  friendRequestCount: number;
  messageRequestCount: number;
  actions: LoungeActions;
}

export const LoungeDataContext = createContext<LoungeData | null>(null);

function useData(): LoungeData {
  const v = useContext(LoungeDataContext);
  if (!v) throw new Error('Lounge hooks must be used inside a data provider');
  return v;
}

const EMPTY_CHANNELS: Channel[] = [];
const EMPTY_MEMBERS: Member[] = [];
const EMPTY_ROLES: Role[] = [];
const EMPTY_IDS: Snowflake[] = [];
const EMPTY_VOICE: VoiceMember[] = [];
const OFFLINE: Presence = { status: 'offline' };
const EMPTY_MESSAGES: MessagesState = {
  messages: [],
  hasMore: false,
  loadingOlder: false,
  loading: false,
};

export const useCurrentUser = () => useData().me;
export const useGuilds = () => useData().guilds;
export const useRail = () => useData().rail;
export function useGuild(guildId: Snowflake | undefined) {
  const d = useData();
  return d.guilds.find((g) => g.id === guildId);
}
export function useChannels(guildId: Snowflake | undefined) {
  const d = useData();
  return (guildId ? d.channelsByGuild[guildId] : undefined) ?? EMPTY_CHANNELS;
}
export const useAllChannels = () => useData().channelsByGuild;
export const useDms = () => useData().dms;
export const useUsers = () => useData().users;
export function useRoles(guildId: Snowflake | undefined) {
  const d = useData();
  return (guildId ? d.rolesByGuild[guildId] : undefined) ?? EMPTY_ROLES;
}
export function useMembers(guildId: Snowflake | undefined) {
  const d = useData();
  return (guildId ? d.membersByGuild[guildId] : undefined) ?? EMPTY_MEMBERS;
}
export function usePresence(userId: Snowflake): Presence {
  const d = useData();
  return d.presence[userId] ?? OFFLINE;
}
export const usePresenceMap = () => useData().presence;
export function useTyping(channelId: Snowflake | undefined): Snowflake[] {
  const d = useData();
  return (channelId ? d.typing[channelId] : undefined) ?? EMPTY_IDS;
}
export function useVoiceMembers(channelId: Snowflake) {
  const d = useData();
  return d.voice[channelId] ?? EMPTY_VOICE;
}
export function useMessages(channelId: Snowflake | undefined): MessagesState {
  const d = useData();
  return (channelId ? d.messages[channelId] : undefined) ?? EMPTY_MESSAGES;
}
export const useLoungeActions = () => useData().actions;
export function useRequestCounts() {
  const d = useData();
  return { friends: d.friendRequestCount, messages: d.messageRequestCount };
}
