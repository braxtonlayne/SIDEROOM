/**
 * Lounge data seam.
 *
 * Every Lounge component reads chat state ONLY through the hooks exported here. In the app the hooks read the
 * normalized store (apps/web/src/store) through the adapters in ./live.ts, which map the Discord-shaped store
 * entities to the view model below (./liveMap.ts). A `MockDataProvider` (./mockData.tsx) supplies a
 * `LoungeDataContext` for tests and Ladle, and its values win over the store when present.
 *
 * Ids are decimal-string snowflakes, like everywhere else on the wire.
 */
import { createContext, useContext, useMemo } from 'react';
import {
  LIVE_ACTIONS,
  useEnsureGuild,
  useEnsureMembers,
  useLiveMessages,
  useLiveSnapshot,
  useLiveTypingIds,
  type LiveSnapshot,
} from './live';
import { applyRailLayout } from './shell/railLayout';
import { useLoungeUi } from './store';

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
  /** Any permission that opens Server Settings (Manage Server/Roles/Channels, Ban Members, View Audit Log, ...). */
  serverSettings?: boolean;
}

export interface Guild {
  id: Snowflake;
  name: string;
  iconUrl?: string | null;
  ownerId: Snowflake;
  /** Any non-muted channel unread. */
  unread: boolean;
  mentionCount: number;
  /** The viewer muted this server (no unread pill or mention badge). */
  muted?: boolean;
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
  /** The loaded window ends at the newest message (live appends are shown). Mock data: always true. */
  atLatest?: boolean;
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
  /** Make sure a message is loaded (search "jump"); live data fetches the page around it. */
  revealMessage?(channelId: Snowflake, messageId: Snowflake): void;
  /** Leave a jumped-to history window and reload the newest messages. */
  jumpToPresent?(channelId: Snowflake): void;
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

/**
 * Set only by the mock provider (tests, Ladle, `VITE_SR_MOCK=1`). Without it every hook reads the store.
 */
export const LoungeDataContext = createContext<LoungeData | null>(null);

/**
 * Picks the source for one hook call. Both sources are always subscribed, so hook order never changes;
 * the store-side effects (loading) run only when no mock is present (`active`).
 */
function useSource(): { mock: LoungeData | null; live: LiveSnapshot } {
  const mock = useContext(LoungeDataContext);
  const live = useLiveSnapshot();
  return { mock, live };
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
const NO_REQUESTS = { friends: 0, messages: 0 };

export function useCurrentUser(): LoungeUser {
  const { mock, live } = useSource();
  return mock ? mock.me : live.me;
}
export function useGuilds(): Guild[] {
  const { mock, live } = useSource();
  return mock ? mock.guilds : live.guilds;
}
export function useRail(): RailEntry[] {
  const { mock, live } = useSource();
  const layout = useLoungeUi((s) => s.railLayout);
  const base = mock ? mock.rail : live.rail;
  return useMemo(() => applyRailLayout(base, layout), [base, layout]);
}
export function useGuild(guildId: Snowflake | undefined) {
  const { mock, live } = useSource();
  return (mock ?? live).guilds.find((g) => g.id === guildId);
}
export function useChannels(guildId: Snowflake | undefined): Channel[] {
  const { mock, live } = useSource();
  useEnsureGuild(guildId, !mock);
  const map = mock ? mock.channelsByGuild : live.channelsByGuild;
  return (guildId ? map[guildId] : undefined) ?? EMPTY_CHANNELS;
}
export function useAllChannels(): Record<Snowflake, Channel[]> {
  const { mock, live } = useSource();
  return mock ? mock.channelsByGuild : live.channelsByGuild;
}
export function useDms(): DmChannel[] {
  const { mock, live } = useSource();
  return mock ? mock.dms : live.dms;
}
export function useUsers(): Record<Snowflake, LoungeUser> {
  const { mock, live } = useSource();
  return mock ? mock.users : live.users;
}
export function useRoles(guildId: Snowflake | undefined): Role[] {
  const { mock, live } = useSource();
  useEnsureGuild(guildId, !mock);
  const map = mock ? mock.rolesByGuild : live.rolesByGuild;
  return (guildId ? map[guildId] : undefined) ?? EMPTY_ROLES;
}
export function useMembers(guildId: Snowflake | undefined): Member[] {
  const { mock, live } = useSource();
  useEnsureMembers(guildId, !mock);
  const map = mock ? mock.membersByGuild : live.membersByGuild;
  return (guildId ? map[guildId] : undefined) ?? EMPTY_MEMBERS;
}
export function usePresence(userId: Snowflake): Presence {
  const { mock, live } = useSource();
  return (mock ? mock.presence : live.presence)[userId] ?? OFFLINE;
}
export function usePresenceMap(): Record<Snowflake, Presence> {
  const { mock, live } = useSource();
  return mock ? mock.presence : live.presence;
}
export function useTyping(channelId: Snowflake | undefined): Snowflake[] {
  const { mock } = useSource();
  const typingIds = useLiveTypingIds(mock ? undefined : channelId);
  if (mock) return (channelId ? mock.typing[channelId] : undefined) ?? EMPTY_IDS;
  return typingIds;
}
export function useVoiceMembers(channelId: Snowflake): VoiceMember[] {
  const { mock } = useSource();
  // Voice state is not in the store yet (LNG-VOICE); live mode shows nobody connected.
  return mock?.voice[channelId] ?? EMPTY_VOICE;
}
export function useMessages(channelId: Snowflake | undefined): MessagesState {
  const mock = useContext(LoungeDataContext);
  const live = useLiveMessages(channelId, !mock);
  if (mock) return (channelId ? mock.messages[channelId] : undefined) ?? EMPTY_MESSAGES;
  return channelId ? live : EMPTY_MESSAGES;
}
export function useLoungeActions(): LoungeActions {
  const mock = useContext(LoungeDataContext);
  return mock ? mock.actions : LIVE_ACTIONS;
}
export function useRequestCounts() {
  const mock = useContext(LoungeDataContext);
  return mock
    ? { friends: mock.friendRequestCount, messages: mock.messageRequestCount }
    : NO_REQUESTS;
}
