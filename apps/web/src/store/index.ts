// FND-API-008: public surface of the normalized client store. The Lounge UI imports ONLY from here.
//
// Hook contract (stable):
//   useConnectionStatus(): ConnectionStatus
//   useCurrentUser(): User | null
//   useGuilds(): Guild[]                          // READY order
//   useGuild(guildId): { guild: Guild | undefined; roles: Role[] }
//   useChannels(guildId): Channel[]               // position then id; guildId '@me' = DMs, newest first
//   useChannel(channelId): Channel | undefined
//   useMessages(channelId): MessagesApi           // auto-loads the newest page; send/retry/discard are optimistic
//   useMembers(guildId): { members: MemberView[]; load(): Promise<void> }
//   usePresence(userId): Presence                 // status 'offline' when unknown
//   useTyping(channelId): User[]                  // users typing now, excluding self
//   useReadState(channelId): ReadState | undefined
//
// Non-hook API: connectGateway()/disconnectGateway() (call once after login), useStore.getState()
// actions (loadGuilds, loadGuild, loadMembers, ...), rest.postTyping(channelId), resetStore().
export * from './types';
export * from './hooks';
export { useStore, resetStore } from './store';
export type { StoreState, StoreActions } from './store';
export { connectGateway, disconnectGateway, getGateway, holdGateway } from './connect';
export { DM_GUILD } from './reducer';
export * as rest from './rest';
export { cdnUrl, cdnSize, isAnimatedHash } from './cdn';
export type { CdnKind, CdnFormat, CdnUrlOptions } from './cdn';
