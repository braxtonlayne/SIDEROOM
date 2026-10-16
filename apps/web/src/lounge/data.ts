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
import { useVoiceParticipants } from '../voice/hooks';
import { useSocial } from './shell/social';
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
  /** Badge id (0 to 15) and 24-bit colour of the server tag; the chip draws them when present. */
  tagBadge?: number;
  tagColor?: number;
  /** The system user "SIDE ROOM" (message flag SYSTEM_TAG, D-188): shown with a SYSTEM badge. */
  system?: boolean;
  /** Posted through a webhook (no profile to open). */
  webhook?: boolean;
}

export interface Activity {
  kind: 'playing' | 'watching' | 'reading' | 'listening' | 'custom';
  text: string;
  /** Second line of a rich activity ("S1 E3 · Episode title", "Chapter 12"). */
  details?: string;
  /** Epoch ms the activity started, for "for 12 minutes". */
  startedAt?: number;
  /** Platform of a game activity ("steam", "playstation", "xbox", "switch", "pc"); picks the card icon. */
  platform?: string;
  /** Set when the activity is a SIDE ROOM watch party that can be joined (/cinema/party/{id}). */
  partyId?: string;
  /** Epoch ms after which the client drops the activity unless it was refreshed. */
  expiresAt?: number;
}

export interface Presence {
  status: PresenceStatus;
  /** The activity shown on one line (custom status first when there is no other). */
  activity?: Activity;
  /** Every activity, custom status excluded, for cards (Active Now, full profile). */
  activities?: Activity[];
  /** The custom status, when set. */
  custom?: Activity;
}

export interface GuildPermissions {
  manageGuild: boolean;
  manageChannels: boolean;
  createInvite: boolean;
  moderate: boolean;
  /** Delete others' messages, remove reactions, end polls. */
  manageMessages?: boolean;
  /** Pin and unpin messages (Manage Messages also allows it). */
  pinMessages?: boolean;
  /** Use @everyone, @here and every role mention. */
  mentionEveryone?: boolean;
  /** Any permission that opens Server Settings (Manage Server/Roles/Channels, Ban Members, View Audit Log, ...). */
  serverSettings?: boolean;
  /** Create scheduled events (and edit or delete their own). */
  createEvents?: boolean;
  /** Edit, start, end and delete anyone's scheduled events. */
  manageEvents?: boolean;
}

export interface Guild {
  id: Snowflake;
  name: string;
  /** The server's traits (J-07), at most five. */
  traits?: { emoji_id?: string | null; emoji_name?: string | null; label: string }[];
  iconUrl?: string | null;
  /** Server banner shown behind the name in the channel list header (LNG-GLD-019). */
  bannerUrl?: string | null;
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

/** What the viewer may do in one channel, computed with the shared Rust permission engine (D-178). */
export interface ChannelPermissions {
  view: boolean;
  send: boolean;
  manageMessages: boolean;
  /** PIN_MESSAGES only (D-178). */
  pinMessages: boolean;
  mentionEveryone: boolean;
  manageChannels: boolean;
  createInvite: boolean;
  /** SEND_TTS_MESSAGES: the /tts command. */
  sendTts?: boolean;
  /** CREATE_PUBLIC_THREADS. */
  createThreads?: boolean;
  /** Voice: join, talk, use voice activity (without it push-to-talk is required), and moderate others. */
  connect?: boolean;
  speak?: boolean;
  useVad?: boolean;
  muteMembers?: boolean;
  deafenMembers?: boolean;
  moveMembers?: boolean;
  /** USE_SOUNDBOARD: play sounds into the call. */
  useSoundboard?: boolean;
  /** REQUEST_TO_SPEAK: raise a hand in a stage. */
  requestToSpeak?: boolean;
  /** SET_VOICE_CHANNEL_STATUS: the status line of a voice channel. */
  setVoiceStatus?: boolean;
}

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
  /** Viewer's permissions in this channel (overwrites applied); absent until the engine has loaded. */
  permissions?: ChannelPermissions;
  userLimit?: number;
  slowmodeSeconds?: number;
}

export interface DmChannel {
  id: Snowflake;
  /** Recipient ids (excluding the viewer); more than one means a group DM. */
  recipientIds: Snowflake[];
  name?: string | null;
  /** Group DM owner. */
  ownerId?: Snowflake | null;
  /** Group DM icon URL. */
  iconUrl?: string | null;
  lastMessageAt: string;
  unread: boolean;
  mentionCount: number;
}

export interface VoiceMember {
  userId: Snowflake;
  speaking?: boolean;
  muted?: boolean;
  deafened?: boolean;
  /** Muted / deafened by a moderator (the viewer cannot undo it). */
  serverMuted?: boolean;
  serverDeafened?: boolean;
  video?: boolean;
  streaming?: boolean;
  /** Stage channels: in the audience. */
  suppressed?: boolean;
  /** Stage channels: when the hand went up (ms since the epoch), null when it is down. */
  handRaisedAt?: number | null;
}

export interface Role {
  id: Snowflake;
  name: string;
  color?: string | null;
  /** Hoisted roles get their own member-list group. */
  hoist: boolean;
  position: number;
  /** Second colour of a gradient role (CSS hex). */
  colorSecondary?: string | null;
  /** Third colour of a gradient role; only with a second one (the holographic preset). */
  colorTertiary?: string | null;
  /** Emoji shown as the role icon next to the name. */
  emoji?: string | null;
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
  /** Alt text. */
  description?: string | null;
  /** BlurHash painted until the image has loaded. */
  blurhash?: string;
  /** Voice messages: length in seconds. */
  durationSecs?: number | null;
  /** Voice messages: amplitude bytes (0..255). */
  waveform?: number[] | null;
  /** Large file past its retention window ("This file has expired"). */
  expired?: boolean;
  /** Hidden by the viewer's own sensitive-media setting (D-230); there is no URL. */
  filtered?: boolean;
}

export interface Reaction {
  emoji: string;
  count: number;
  me: boolean;
}

/** Discord-shaped embed (snake_case fields are kept: they are rendered, not edited). */
export type Embed = import('../store/types').RawEmbed;

export interface PollAnswer {
  id: number;
  text: string;
  emoji?: string | undefined;
  count: number;
  me: boolean;
}

export interface Poll {
  question: string;
  answers: PollAnswer[];
  multi: boolean;
  /** ISO; null when the poll never closes. */
  expiresAt: string | null;
  finalized: boolean;
}

export interface StickerItem {
  id: Snowflake;
  name: string;
  /** 1 PNG, 2 APNG, 3 Lottie, 4 GIF. */
  formatType: number;
}

/** The copy taken when a message was forwarded. */
export interface ForwardSnapshot {
  /** Where the original lives (`message_reference` of the forward). */
  source?: { channelId: Snowflake; guildId?: Snowflake; messageId: Snowflake };
  content: string;
  createdAt: string;
  attachments: Attachment[];
  embeds: Embed[];
}

/** A poll to create with a message. `durationHours` 1..168. */
export interface NewPoll {
  question: string;
  answers: { text: string; emoji?: string }[];
  durationHours: number;
  multi: boolean;
}

/** An uploaded file to attach to a new message (see ./upload.ts). */
export interface UploadedFile {
  id: string;
  uploadedFilename: string;
  description?: string | null;
  durationSecs?: number | null;
  waveform?: string | null;
}

export interface SendExtras {
  attachments?: UploadedFile[];
  poll?: NewPoll;
  /** false = reply without pinging the author. */
  mentionRepliedUser?: boolean;
  flags?: number;
  /** Guild stickers to send (up to three). */
  stickerIds?: string[];
  /** Text-to-speech message (needs SEND_TTS_MESSAGES). */
  tts?: boolean;
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
  pinned?: boolean;
  /** Discord message type: 0 default, 6 pin notice. */
  type?: number;
  mentionIds?: Snowflake[];
  mentionRoleIds?: Snowflake[];
  mentionEveryone?: boolean;
  embeds?: Embed[];
  stickers?: StickerItem[];
  poll?: Poll | null;
  /** Set when this message is a forward (the original is summarized in the snapshot). */
  forwarded?: ForwardSnapshot | null;
  /** Voice message (flag 1 << 13). */
  voice?: boolean;
  /** Ephemeral app reply (flag 1 << 6): only this viewer sees it and it is never stored. */
  ephemeral?: boolean;
  suppressEmbeds?: boolean;
  /** Ids of application emoji in the text that this message's author may not render (D-237): shown as `:name:`. */
  textEmoji?: string[];
  /** Buttons and selects on app messages. */
  components?: import('../store/types').RawComponent[];
  /** `message_reference.channel_id`: the thread of a thread-created row, the origin of a thread's first row. */
  referenceChannelId?: Snowflake | null;
  /** Text-to-speech message. */
  tts?: boolean;
  /** The thread started from this message. */
  thread?: { id: Snowflake; name: string; messageCount: number } | null;
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
  sendMessage(
    channelId: Snowflake,
    content: string,
    replyToId?: Snowflake | null,
    extras?: SendExtras,
  ): void;
  editMessage(channelId: Snowflake, messageId: Snowflake, content: string): void;
  deleteMessage?(channelId: Snowflake, messageId: Snowflake): void;
  pinMessage?(channelId: Snowflake, messageId: Snowflake, pinned: boolean): void;
  /** Forward a message to up to five channels, each optionally followed by a note. */
  forwardMessage?(
    source: { channelId: Snowflake; messageId: Snowflake; guildId?: Snowflake },
    targets: Snowflake[],
    note?: string,
  ): void;
  /** Cast or withdraw the viewer's vote on a poll answer. */
  votePoll?(channelId: Snowflake, messageId: Snowflake, answerId: number, add: boolean): void;
  /** Close a poll early (author only). */
  endPoll?(channelId: Snowflake, messageId: Snowflake): void;
  /** Re-send a message whose delivery failed / drop it. */
  retryMessage?(channelId: Snowflake, messageId: Snowflake): void;
  discardMessage?(channelId: Snowflake, messageId: Snowflake): void;
  loadOlder(channelId: Snowflake): void;
  /** Jumped-to window: load the page after the newest loaded message. */
  loadNewer?(channelId: Snowflake): void;
  markRead(channelId: Snowflake): void;
  /** Mark Unread from a message; resolves with the id of the last message still counted as read. */
  markUnread?(channelId: Snowflake, messageId: Snowflake): Promise<Snowflake>;
  /** The viewer left the channel: automatic read acks resume. */
  releaseUnread?(channelId: Snowflake): void;
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
  const live = useVoiceParticipants(mock ? undefined : channelId);
  if (mock) return mock.voice[channelId] ?? EMPTY_VOICE;
  return live;
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
  const friends = useSocial((s) => s.relationships.filter((r) => r.type === 3).length);
  const messages = useSocial((s) => s.messageRequests.length);
  return useMemo(
    () =>
      mock
        ? { friends: mock.friendRequestCount, messages: mock.messageRequestCount }
        : friends || messages
          ? { friends, messages }
          : NO_REQUESTS,
    [mock, friends, messages],
  );
}
