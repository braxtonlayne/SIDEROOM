export default {
  // ---- BRG-COR-001: the Discord bridge tile on Admin > System (phase 35) ----
  'admin.system.bridgeTitle': 'Discord bridge',
  'admin.system.bridgeHint':
    'Servers whose owner connected a Discord bot, the channels they mirror, the posts waiting for their turn under Discord rate limits and the posts that failed for good. Switch the bridge off for every server under Admin > Features (Discord bridge).',
  'admin.system.bridgeOff': 'Switched off by the operator',
  'admin.system.bridgeServers': '{n} servers connected',
  'admin.system.bridgeMaps': '{n} mapped channels',
  'admin.system.bridgeQueued': '{n} posts waiting',
  'admin.system.bridgeDead': '{n} failed for good',
} satisfies Record<string, string>;
