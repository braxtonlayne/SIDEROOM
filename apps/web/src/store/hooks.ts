/* eslint-disable @typescript-eslint/no-unused-vars -- signature stubs, replaced in the next commit */
import type {
  Channel,
  ConnectionStatus,
  Guild,
  Member,
  Message,
  Presence,
  ReadState,
  Role,
  User,
} from './types';

export interface MessagesApi {
  /** Oldest to newest; optimistic messages (message.local set) are last. */
  messages: Message[];
  hasMoreBefore: boolean;
  loading: boolean;
  error: string | null;
  /** Load the newest page if the channel has no window yet. Safe to call repeatedly. */
  loadInitial: () => Promise<void>;
  /** Load the page of history before the oldest loaded message. */
  loadOlder: () => Promise<void>;
  /** Optimistically send; resolves with the server message, rejects if the REST call fails. */
  send: (content: string) => Promise<Message>;
  /** Re-send a failed optimistic message by nonce. */
  retry: (nonce: string) => Promise<Message>;
  /** Drop a failed optimistic message by nonce. */
  discard: (nonce: string) => void;
}

export interface MemberView {
  member: Member;
  user: User | undefined;
  roles: Role[];
  presence: Presence;
}

export interface GuildView {
  guild: Guild | undefined;
  roles: Role[];
}

export interface MembersApi {
  members: MemberView[];
  load: () => Promise<void>;
}

// SIGNATURES ONLY in this commit; implementations land in the next one.
const todo = (): never => {
  throw new Error('store not implemented yet');
};
export const useConnectionStatus = (): ConnectionStatus => todo();
export const useCurrentUser = (): User | null => todo();
export const useGuilds = (): Guild[] => todo();
export const useGuild = (_guildId: string | undefined): GuildView => todo();
export const useChannels = (_guildId: string | undefined): Channel[] => todo();
export const useChannel = (_channelId: string | undefined): Channel | undefined => todo();
export const useMessages = (_channelId: string | undefined): MessagesApi => todo();
export const useMembers = (_guildId: string | undefined): MembersApi => todo();
export const usePresence = (_userId: string | undefined): Presence => todo();
export const useTyping = (_channelId: string | undefined): User[] => todo();
export const useReadState = (_channelId: string | undefined): ReadState | undefined => todo();
