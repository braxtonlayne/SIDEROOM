export default {
  // Security Actions: timed pauses and the raid alert (J-02, D-243)
  'lounge.securityActions.pauseInvitesDay': 'Pause invites for 24 hours',
  'lounge.securityActions.pauseInvitesDayHint':
    'Nobody can join with an invite link or from the server directory for the next 24 hours.',
  'lounge.securityActions.pauseDmsDay': 'Pause direct messages for 24 hours',
  'lounge.securityActions.pauseDmsDayHint':
    'Members of this server cannot message each other privately unless they are friends. Nothing goes to Message Requests while this runs.',
  'lounge.securityActions.pausedUntil': 'Paused until {time}.',
  'lounge.securityActions.pauseInvitesIndefinitely': 'Pause invites until I turn it back on',
  'lounge.securityActions.pauseInvitesIndefinitelyHint':
    'Nobody can join with an invite link until you turn this off.',
  'lounge.securityActions.autoPause': 'Pause invites automatically during a raid',
  'lounge.securityActions.autoPauseHint':
    'When more members join in a minute than the raid alert allows, invites pause for 24 hours and an alert with a Resume invites button is posted.',

  // Channel list: the paused-invites line and the onboarding entry (J-02, J-04, D-243, D-245)
  'lounge.channelSidebar.invitesPausedUntil': 'Invites paused until {time}',
  'lounge.channelSidebar.channelsAndRoles': 'Channels & Roles',

  // Per-server DM switch (J-01, LNG-GLD-025)
  'lounge.serverDm.title': 'Privacy Settings',
  'lounge.serverDm.subtitle': 'Direct messages and {name}',
  'lounge.serverDm.label': 'Allow direct messages from server members',
  'lounge.serverDm.hint':
    'People in this server can message you directly. Turn it off to stop that for this server only.',
  'lounge.serverDm.loading': 'Loading your privacy settings',
  'lounge.serverDm.loadError': 'Could not load your privacy settings.',
  'lounge.serverDm.saveError': 'Could not change that setting.',
  'lounge.serverDm.close': 'Close',
  'lounge.serverDm.moreSettings': 'Open all privacy settings',

  // Applications installed in the server and their command permissions (J-05, J-10)
  'settings.integrationsOverview.noAppsHint': 'Add one from the Apps tab.',
  'settings.integrationsOverview.addedBy': 'Added by {name}',
  'settings.integrationsOverview.commands': 'Commands',
  'settings.integrationsOverview.commandsNamed': 'Command permissions of {name}',
  'settings.commandPermissions.title': 'Command permissions for {name}',
  'settings.commandPermissions.subtitle':
    'Choose who can use its commands. Rules for a single command win over the rules for all commands.',
  'settings.commandPermissions.close': 'Close',
  'settings.commandPermissions.save': 'Save',
  'settings.commandPermissions.saved': 'Command permissions saved.',
  'settings.commandPermissions.loading': 'Loading the commands',
  'settings.commandPermissions.command': 'Command',
  'settings.commandPermissions.allCommands': 'All commands (application default)',
  'settings.commandPermissions.noCommands': 'No commands yet',
  'settings.commandPermissions.noCommandsHint':
    'You can still set rules that apply to every command it adds later.',
  'settings.commandPermissions.rules': 'Rules',
  'settings.commandPermissions.everyone': 'Everyone',
  'settings.commandPermissions.allChannels': 'All channels',
  'settings.commandPermissions.default': 'Default',
  'settings.commandPermissions.allow': 'Allow',
  'settings.commandPermissions.deny': 'Deny',
  'settings.commandPermissions.ruleFor': 'Rule for {name}',
  'settings.commandPermissions.kindRole': 'Role',
  'settings.commandPermissions.kindMember': 'Member',
  'settings.commandPermissions.kindChannel': 'Channel',
  'settings.commandPermissions.remove': 'Remove',
  'settings.commandPermissions.removeNamed': 'Remove the rule for {name}',
  'settings.commandPermissions.add': 'Add a role, member or channel',
  'settings.commandPermissions.addPlaceholder': 'Choose one',

  // Voice and stage channels: video quality and region (J-11, LNG-CHN-017)
  'settings.channels.videoQuality': 'Full video quality',
  'settings.channels.videoQualityHint':
    'Cameras in this channel can send up to 720p whatever the room load. Off means automatic.',
  'settings.channels.region': 'Voice region: automatic (this installation has one region).',
} satisfies Record<string, string>;
