// BRG-UI-001 (wave 35, D-381): the Discord bridge. Server Settings > Modules card, the "Discord bridge" screen, the
// teardown dialog, the channel banner, the per-member opt-out, the DISCORD tag and the privacy page list.
export default {
  // ---- the card under Modules > "Integrations, more"
  'settings.bridge.card.noticeTitle': 'Before you switch this on',
  'settings.bridge.card.notice':
    "Switching this on mirrors the channels you map to a Discord server you choose, with members' names, avatars and messages, and shows Discord users' messages here. Members can opt out per channel. Switching it off removes the webhooks from Discord and the mappings here.",
  'settings.bridge.card.ownerOnlyOn': 'Only the server owner can switch this on.',
  'settings.bridge.card.ownerOnlyOff': 'Only the server owner can switch this off.',
  'settings.bridge.card.open': 'Open the Discord bridge settings',

  // ---- the screen
  'settings.bridge.nav': 'Discord bridge',
  'settings.bridge.title': 'Discord bridge',
  'settings.bridge.description':
    'Mirror chosen channels with a Discord server, both ways, through a bot you create on Discord.',
  'settings.bridge.loading': 'Loading the Discord bridge',
  'settings.bridge.privacyTitle': 'What the bridge shares',
  'settings.bridge.privacy':
    "Every channel you map is mirrored to the Discord server you choose, with members' names, avatars and messages, and Discord users' messages are shown here with a DISCORD tag. Attachments go as links, never uploaded. Encrypted channels and direct messages are never mirrored. Members can opt out per channel.",

  'settings.bridge.token.title': 'Bot connection',
  'settings.bridge.token.step1': 'Create an application with a bot on Discord:',
  'settings.bridge.token.portal': 'Discord developer portal',
  'settings.bridge.token.step2':
    'On the bot page, turn on the Message Content intent. Without it Discord sends no message text.',
  'settings.bridge.token.step3': 'Copy the bot token and paste it below.',
  'settings.bridge.token.field': 'Bot token',
  'settings.bridge.token.fieldHint':
    'Shown only now. It is sealed on the server and never displayed again. Only a bot token is accepted.',
  'settings.bridge.token.save': 'Connect the bot',
  'settings.bridge.token.saved': 'The bot is connected.',
  'settings.bridge.token.revoke': 'Disconnect the bot',
  'settings.bridge.token.revoked': 'The bot token was removed.',
  'settings.bridge.token.connected': 'Connected to the Discord server {name}.',
  'settings.bridge.token.connecting': 'Connecting to Discord.',
  'settings.bridge.token.problem': 'The bot could not connect. {error}',
  'settings.bridge.token.intentTitle': 'The Message Content intent is off',
  'settings.bridge.token.intentBody':
    'Turn on "Message Content Intent" for the bot in the Discord developer portal, or messages from Discord arrive empty.',
  'settings.bridge.token.invite': 'Invite the bot to your Discord server',
  'settings.bridge.token.inviteHint': 'It asks only for the permissions the bridge needs.',
  'settings.bridge.token.ownerOnlyPaste': 'Only the server owner can paste the bot token.',
  'settings.bridge.token.ownerOnlyRevoke': 'Only the server owner can disconnect the bot.',

  'settings.bridge.map.title': 'Mirrored channels',
  'settings.bridge.map.encrypted':
    'Encrypted channels cannot be mirrored: the server cannot read them, so they are refused.',
  'settings.bridge.map.none': 'No channel is mirrored yet.',
  'settings.bridge.map.colOurs': 'Channel here',
  'settings.bridge.map.colTheirs': 'Channel on Discord',
  'settings.bridge.map.colDirection': 'Direction',
  'settings.bridge.map.colOptOut': 'Opted out',
  'settings.bridge.map.colActions': 'Actions',
  'settings.bridge.map.both': 'Both ways',
  'settings.bridge.map.toDiscord': 'From here to Discord',
  'settings.bridge.map.fromDiscord': 'From Discord to here',
  'settings.bridge.map.optOuts': '{n, plural, one {# member} other {# members}}',
  'settings.bridge.map.testShort': 'Test',
  'settings.bridge.map.test': 'Post a hello in #{name} on Discord',
  'settings.bridge.map.tested': 'Posted a hello for #{name} on Discord.',
  'settings.bridge.map.removeShort': 'Remove',
  'settings.bridge.map.remove': 'Stop mirroring #{name}',
  'settings.bridge.map.removed': 'The channel is no longer mirrored.',
  'settings.bridge.map.addTitle': 'Mirror another channel',
  'settings.bridge.map.fieldOurs': 'Channel here',
  'settings.bridge.map.fieldTheirs': 'Channel on Discord',
  'settings.bridge.map.fieldDirection': 'Direction',
  'settings.bridge.map.choose': 'Choose a channel',
  'settings.bridge.map.add': 'Mirror this channel',
  'settings.bridge.map.added': 'The channel is now mirrored.',

  'settings.bridge.off.title': 'Switch off',
  'settings.bridge.off.body':
    'Switching the bridge off removes its webhooks from Discord and the mappings here. Messages already mirrored stay where they are.',
  'settings.bridge.off.button': 'Switch off the Discord bridge',
  'settings.bridge.off.ownerOnly': 'Only the server owner can switch the bridge off.',

  // ---- the teardown dialog
  'settings.bridge.teardown.title': 'Switch off the Discord bridge?',
  'settings.bridge.teardown.removedTitle': 'What is removed',
  'settings.bridge.teardown.removedWebhooks':
    'The webhooks the bridge created on Discord are deleted.',
  'settings.bridge.teardown.removedMaps':
    'The channel mappings, the message links and every opt-out are deleted here.',
  'settings.bridge.teardown.removedToken': 'The bot token is wiped.',
  'settings.bridge.teardown.staysTitle': 'What stays',
  'settings.bridge.teardown.staysMessages':
    'Messages already mirrored stay where they are: the DISCORD-tagged messages here, and the messages the webhooks posted on Discord.',
  'settings.bridge.teardown.discordStep':
    'If Discord refuses to delete a webhook, this screen names it and you remove it by hand under {where} on Discord.',
  'settings.bridge.teardown.again':
    'Switching it on again starts empty: paste the token and map the channels afresh.',
  'settings.bridge.teardown.cancel': 'Keep it on',
  'settings.bridge.teardown.confirm': 'Switch off and remove',
  'settings.bridge.teardown.doneTitle': 'The Discord bridge is off',
  'settings.bridge.teardown.doneBody':
    '{n, plural, =0 {No channel mapping} one {# channel mapping} other {# channel mappings}} removed.',
  'settings.bridge.teardown.nothingLeft': 'Every webhook was deleted on Discord.',
  'settings.bridge.teardown.failedTitle': 'Some webhooks are still on Discord',
  'settings.bridge.teardown.failedBody':
    'Discord would not delete these webhooks. Remove them by hand under {where} on Discord.',
  'settings.bridge.teardown.failedList': 'Webhooks to remove by hand',
  'settings.bridge.teardown.close': 'Done',

  // ---- the channel banner, the menu entry and the tag
  'lounge.bridge.bannerBoth':
    'Messages here are mirrored to Discord server {name} and back; messages from Discord are shown with a DISCORD tag.',
  'lounge.bridge.bannerOut': 'Messages here are mirrored to Discord server {name}.',
  'lounge.bridge.bannerIn':
    'Messages from Discord server {name} are shown here with a DISCORD tag.',
  'lounge.bridge.bannerOptedOut': 'Your messages are not mirrored.',
  'lounge.bridge.optOut': "Don't mirror my messages",
  'lounge.bridge.optIn': 'Mirror my messages again',
  'lounge.bridge.optError': 'Could not save that change. Try again.',
  'lounge.messageItem.bridgeTag': 'DISCORD',

  // ---- the privacy page (the handbook's privacy section)
  'help.handbook.privacy.bridge':
    "If your server's owner switches the Discord bridge on, the channels they map are mirrored to a Discord server they choose, with names, avatars and messages. A banner in each such channel says so, and you can opt out per channel; your messages then stay here only. Messages already sent stay on Discord.",
  'help.bridge.listTitle': 'Channels mirrored to Discord',
  'help.bridge.listEmpty': 'None of your servers mirrors a channel to Discord.',
  'help.bridge.listItem': '#{channel} in {server} is mirrored with the Discord server {discord}.',
  'help.bridge.optOut': "Don't mirror my messages in #{channel}",
  'help.bridge.optIn': 'Mirror my messages again in #{channel}',
} satisfies Record<string, string>;
