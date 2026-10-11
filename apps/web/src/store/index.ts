// FND-API-008: public surface of the normalized client store. The Lounge UI imports ONLY from here.
//
// Hook contract (stable):
//   useConnectionStatus(): ConnectionStatus
//   useCurrentUser(): User | null
//   useGuilds(): Guild[]                          // READY order
//   useGuild(guildId): { guild: Guild | undefined; roles: Role[] }
//   useChannels(guildId): Channel[]               // sorted by position then id
//   useChannel(channelId): Channel | undefined
//   useMessages(channelId): MessagesApi           // see below
//   useMembers(guildId): { members: MemberView[]; load(): Promise<void> }
//   usePresence(userId): Presence                 // 'offline' when unknown
//   useTyping(channelId): User[]                  // users typing now, excluding self
//   useReadState(channelId): ReadState | undefined
export * from './types';
export * from './hooks';
