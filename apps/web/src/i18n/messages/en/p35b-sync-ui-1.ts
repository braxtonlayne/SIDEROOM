// BRG-SYN-004 (wave 35b, the whole-server Discord mirror): mode card, preflight, plan, sync status, conflict log,
// role editor and channel badges, member grouping, teardown options and the whole-server privacy texts.
export default {
  // ---- mode card
  'settings.bridgeSync.mode.title': 'How much to bridge',
  'settings.bridgeSync.mode.current': 'Mode: {mode}',
  'settings.bridgeSync.mode.selective': 'Only bridge these',
  'settings.bridgeSync.mode.wholeGuild': 'Bridge the whole server',
  'settings.bridgeSync.mode.selectiveBody':
    'You pick the channels. Roles and permissions are not synced: Discord keeps its own.',
  'settings.bridgeSync.mode.wholeBody':
    'Bridge the whole server makes this server and the Discord server one: roles, categories, channels and threads are created and kept in step on both, and every channel’s messages flow both ways. Permissions are set here; changes made to them on Discord are undone. Who holds which role on Discord is decided on Discord.',
  'settings.bridgeSync.mode.adminWarningTitle': 'The bot needs Administrator on Discord',
  'settings.bridgeSync.mode.adminWarning':
    'To keep permissions in step the bot must be an Administrator with the top role on Discord. Anyone who gets the bot token then controls the Discord server. Selective mode does not need this.',
  'settings.bridgeSync.mode.prepare': 'Prepare the whole-server plan',
  'settings.bridgeSync.mode.prepareHint':
    'Nothing changes until the owner confirms the plan. Direct messages and voice are never mirrored.',
  'settings.bridgeSync.mode.needBot': 'Connect the bot first.',
  'settings.bridgeSync.mode.ownerOnly': 'Only the server owner can change this.',
  'settings.bridgeSync.mode.back': 'Go back to Only bridge these',
  'settings.bridgeSync.mode.backTitle': 'Go back to Only bridge these?',
  'settings.bridgeSync.mode.backBody':
    'Nothing is deleted on either side. Channels stay mirrored as separate mappings. Role syncing and permission reverts stop, and Discord keeps the permissions that were last pushed to it.',
  'settings.bridgeSync.mode.backConfirm': 'Go back',
  'settings.bridgeSync.mode.backDone': 'The bridge now mirrors only the channels you pick.',
  'settings.bridgeSync.mode.cancel': 'Cancel',

  // ---- whole-server privacy
  'settings.bridgeSync.privacy.title': 'The whole server is mirrored',
  'settings.bridgeSync.privacy.body':
    'Everything in this server except direct messages and voice is mirrored to Discord server {name}. Who can read it there is decided by Discord’s role assignments.',
  'settings.bridgeSync.privacy.noName': 'the Discord server you chose',
  'settings.bridgeSync.card.wholeNote':
    'You can later choose to bridge the whole server instead. That has its own review and notice.',

  // ---- preflight
  'settings.bridgeSync.preflight.title': 'Checks before the plan',
  'settings.bridgeSync.preflight.ok': 'Ready',
  'settings.bridgeSync.preflight.fail': 'Fix this first',
  'settings.bridgeSync.preflight.warn': 'Warning',
  'settings.bridgeSync.preflight.recheck': 'Check again',
  'settings.bridgeSync.preflight.checking': 'Checking',
  'settings.bridgeSync.preflight.invite': 'Invite the bot with Administrator',
  'settings.bridgeSync.preflight.botAdmin': 'The bot is an Administrator on Discord',
  'settings.bridgeSync.preflight.botTopRole': 'The bot’s role is the highest role on Discord',
  'settings.bridgeSync.preflight.intents': 'Server Members and Message Content intents are on',
  'settings.bridgeSync.preflight.caps': 'Roles and channels fit Discord’s limits (250 and 500)',
  'settings.bridgeSync.preflight.uniqueGuild': 'No other server here already mirrors this Discord server',
  'settings.bridgeSync.preflight.otherIntegration': 'No other bot changed roles there in the last hour',

  // ---- plan
  'settings.bridgeSync.plan.title': 'Whole-server plan',
  'settings.bridgeSync.plan.intro':
    'This is a dry run. Nothing is created, changed or sent until the owner confirms.',
  'settings.bridgeSync.plan.seedLegend': 'When names and order differ, use',
  'settings.bridgeSync.plan.seedSr': 'This server’s',
  'settings.bridgeSync.plan.seedDiscord': 'Discord’s',
  'settings.bridgeSync.plan.seedHint':
    'Permissions are always this server’s. Role order too. This choice is for names, topics and channel order.',
  'settings.bridgeSync.plan.build': 'Build the plan',
  'settings.bridgeSync.plan.building': 'Building the plan. Large servers take a few seconds.',
  'settings.bridgeSync.plan.failed': 'The plan could not be built. Check again, then rebuild.',
  'settings.bridgeSync.plan.rebuild': 'Rebuild',
  'settings.bridgeSync.plan.close': 'Close',
  'settings.bridgeSync.plan.estimate':
    'About {calls, number} Discord calls, roughly {minutes, plural, one {# minute} other {# minutes}}.',
  'settings.bridgeSync.plan.expiresIn':
    '{minutes, plural, one {This plan expires in # minute.} other {This plan expires in # minutes.}}',
  'settings.bridgeSync.plan.expired': 'This plan expired. Rebuild it.',
  'settings.bridgeSync.plan.stale':
    'This server or the Discord server changed after the plan was built. Rebuild it and look again.',
  'settings.bridgeSync.plan.blockersTitle': 'Fix these before confirming',
  'settings.bridgeSync.plan.confirm': 'Confirm the plan',
  'settings.bridgeSync.plan.ownerOnly': 'Only the server owner can confirm. You can prepare and review it.',
  'settings.bridgeSync.plan.confirmTitle': 'Bridge the whole server?',
  'settings.bridgeSync.plan.confirmStart': 'Confirm and start',
  'settings.bridgeSync.plan.confirmBody':
    'Roles, categories, channels and threads will be created and matched on both sides, and permissions on Discord will be set from here. Mirroring starts now; there is no history backfill.',
  'settings.bridgeSync.plan.started': 'The whole server is now being mirrored. Progress is on this page.',
  'settings.bridgeSync.plan.tabsLabel': 'Plan sections',
  'settings.bridgeSync.plan.tabRoles': 'Roles ({n})',
  'settings.bridgeSync.plan.tabChannels': 'Channels ({n})',
  'settings.bridgeSync.plan.tabThreads': 'Threads ({n})',
  'settings.bridgeSync.plan.tabChanges': 'Changes on Discord ({n})',
  'settings.bridgeSync.plan.tabImports': 'Imported from Discord ({n})',
  'settings.bridgeSync.plan.tabSkipped': 'Not mirrored ({n})',
  'settings.bridgeSync.plan.tabBlockers': 'Blockers ({n})',
  'settings.bridgeSync.plan.colName': 'Here',
  'settings.bridgeSync.plan.colDiscord': 'On Discord',
  'settings.bridgeSync.plan.colWhat': 'What happens',
  'settings.bridgeSync.plan.empty': 'Nothing here.',
  'settings.bridgeSync.plan.actionMatch': 'Matched',
  'settings.bridgeSync.plan.actionPinned': 'Kept from your selection',
  'settings.bridgeSync.plan.actionCreateHere': 'Created here',
  'settings.bridgeSync.plan.actionCreateThere': 'Created on Discord',
  'settings.bridgeSync.plan.unlink': 'Unlink {name}',
  'settings.bridgeSync.plan.unlinkShort': 'Unlink',
  'settings.bridgeSync.plan.linkTitle': 'Link two by hand',
  'settings.bridgeSync.plan.linkHere': 'Only here',
  'settings.bridgeSync.plan.linkThere': 'Only on Discord',
  'settings.bridgeSync.plan.linkChoose': 'Choose',
  'settings.bridgeSync.plan.link': 'Link',
  'settings.bridgeSync.plan.importsIntro':
    'These exist only on Discord. Their Discord permissions can come across. You confirm them, not Discord. Dangerous ones are marked.',
  'settings.bridgeSync.plan.importToggle': 'Bring over the permissions of {name}',
  'settings.bridgeSync.plan.dangerous': 'Dangerous: {list}',
  'settings.bridgeSync.plan.noPermissions': 'No permissions',
  'settings.bridgeSync.plan.skipE2ee': 'End-to-end encrypted',
  'settings.bridgeSync.plan.skipDm': 'Direct message',
  'settings.bridgeSync.plan.skipSrOnlyBit': 'Permission only this server has',
  'settings.bridgeSync.plan.skipMemberOverwrite': 'Per-person permission, stays here only',
  'settings.bridgeSync.plan.skipManagedRole': 'Managed by an app or Discord',
  'settings.bridgeSync.plan.skipDegradedType': 'Mirrored as a simpler type',
  'settings.bridgeSync.plan.skipCap': 'Over Discord’s limit',
  'settings.bridgeSync.plan.skipVoiceMedia': 'Voice and video are never mirrored',
  'settings.bridgeSync.plan.blockBotAdmin': 'The bot is not an Administrator on Discord.',
  'settings.bridgeSync.plan.blockBotTopRole': 'The bot’s role is not the highest on Discord.',
  'settings.bridgeSync.plan.blockIntents': 'A privileged intent is off in the Discord developer portal.',
  'settings.bridgeSync.plan.blockCaps': 'Too many roles or channels for Discord’s limits.',
  'settings.bridgeSync.plan.blockUniqueGuild': 'Another server here already mirrors this Discord server.',
  'settings.bridgeSync.plan.blockStaffGuild': 'A staff server cannot be bridged.',

  // ---- sync status
  'settings.bridgeSync.status.title': 'Sync status',
  'settings.bridgeSync.status.loading': 'Loading sync status',
  'settings.bridgeSync.status.stateSeeding': 'Setting up',
  'settings.bridgeSync.status.stateLive': 'In sync',
  'settings.bridgeSync.status.statePaused': 'Paused',
  'settings.bridgeSync.status.statePausedAccess': 'Paused: the bot lost access on Discord',
  'settings.bridgeSync.status.statePausedInvalid': 'Paused: Discord refused too many requests',
  'settings.bridgeSync.status.seedProgress': 'Mirroring {done, number} of {total, number}',
  'settings.bridgeSync.status.seedEta':
    '{minutes, plural, =0 {Less than a minute left.} one {About # minute left.} other {About # minutes left.}}',
  'settings.bridgeSync.status.botTitle': 'The bot on Discord',
  'settings.bridgeSync.status.botAdmin': 'Administrator',
  'settings.bridgeSync.status.botTop': 'Highest role',
  'settings.bridgeSync.status.botIntents': 'Intents',
  'settings.bridgeSync.status.yes': 'Yes',
  'settings.bridgeSync.status.no': 'No',
  'settings.bridgeSync.status.lastReconcile': 'Last full check',
  'settings.bridgeSync.status.never': 'Not yet',
  'settings.bridgeSync.status.queue': 'Waiting to send',
  'settings.bridgeSync.status.dead': 'Gave up on',
  'settings.bridgeSync.status.rateWait': 'Waiting on a Discord limit',
  'settings.bridgeSync.status.rateSeconds': '{n, plural, one {# second} other {# seconds}}',
  'settings.bridgeSync.status.none': 'None',
  'settings.bridgeSync.status.mapped': 'Mirrored',
  'settings.bridgeSync.status.oneSided': 'On one side only',
  'settings.bridgeSync.status.degraded': 'Simpler type',
  'settings.bridgeSync.status.frozen': 'Frozen',
  'settings.bridgeSync.status.reconcile': 'Reconcile now',
  'settings.bridgeSync.status.reconcileSoon': 'Next possible {when}',
  'settings.bridgeSync.status.reconcileStarted': 'Checking both sides now.',
  'settings.bridgeSync.status.pause': 'Pause sync',
  'settings.bridgeSync.status.resume': 'Resume sync',
  'settings.bridgeSync.status.paused': 'Sync is paused.',
  'settings.bridgeSync.status.resumed': 'Sync is running.',
  'settings.bridgeSync.status.window':
    'A permission changed on Discord is undone within a couple of seconds. If the bridge is offline it stays changed on Discord until it is back. Nothing on this server is ever affected.',
  'settings.bridgeSync.status.pausedNote': 'While paused, nothing is pushed or undone. This server is unaffected.',

  // ---- conflict log
  'settings.bridgeSync.conflicts.title': 'Conflict log',
  'settings.bridgeSync.conflicts.loading': 'Loading the conflict log',
  'settings.bridgeSync.conflicts.empty': 'No conflicts.',
  'settings.bridgeSync.conflicts.filterKind': 'Show',
  'settings.bridgeSync.conflicts.allKinds': 'Everything',
  'settings.bridgeSync.conflicts.showResolved': 'Include resolved',
  'settings.bridgeSync.conflicts.list': 'Conflicts',
  'settings.bridgeSync.conflicts.kindPermissionReverted': 'Permission change undone',
  'settings.bridgeSync.conflicts.kindOverwriteTrimmed': 'Discord permission trimmed',
  'settings.bridgeSync.conflicts.kindStructureConflict': 'Both sides changed it',
  'settings.bridgeSync.conflicts.kindFrozen': 'Frozen after a tug of war',
  'settings.bridgeSync.conflicts.kindHeldDeletion': 'Deleted on Discord, held here',
  'settings.bridgeSync.conflicts.kindRoleDeletionRefused': 'Role deletion refused',
  'settings.bridgeSync.conflicts.kindDeadOp': 'Could not be sent',
  'settings.bridgeSync.conflicts.kindCap': 'Over Discord’s limit',
  'settings.bridgeSync.conflicts.discordDid': 'Discord: {value}',
  'settings.bridgeSync.conflicts.srHas': 'This server: {value}',
  'settings.bridgeSync.conflicts.who': 'By {name}',
  'settings.bridgeSync.conflicts.whoUnknown': 'By someone unknown',
  'settings.bridgeSync.conflicts.bridgeDid': 'The bridge: {value}',
  'settings.bridgeSync.conflicts.when': 'Last {when}',
  'settings.bridgeSync.conflicts.times': '{n, plural, one {# time} other {# times}}',
  'settings.bridgeSync.conflicts.resolved': 'Resolved',
  'settings.bridgeSync.conflicts.applyOnSr': 'Apply on SIDE ROOM',
  'settings.bridgeSync.conflicts.takeDiscord': 'Take Discord’s',
  'settings.bridgeSync.conflicts.restore': 'Restore',
  'settings.bridgeSync.conflicts.deleteNow': 'Delete now',
  'settings.bridgeSync.conflicts.unfreeze': 'Unfreeze',
  'settings.bridgeSync.conflicts.retry': 'Retry',
  'settings.bridgeSync.conflicts.dismiss': 'Dismiss',
  'settings.bridgeSync.conflicts.actionLabel': '{action}: {name}',
  'settings.bridgeSync.conflicts.done': 'Done.',
  'settings.bridgeSync.conflicts.prefillTitle': 'Prefilled from Discord’s change',
  'settings.bridgeSync.conflicts.prefillBody':
    'The permissions below are what someone tried to set on Discord. Review them, then save. Saving is your edit, under your permissions.',

  // ---- role editor
  'settings.bridgeSync.role.mirrored': 'Mirrored to Discord',
  'settings.bridgeSync.role.frozen': 'Sync frozen',
  'settings.bridgeSync.role.srOnlyGroup': 'SIDE ROOM only, not sent to Discord',
  'settings.bridgeSync.role.discordOnlyTitle': 'Set on Discord',
  'settings.bridgeSync.role.discordOnlyBody':
    'Discord has these on the role. This server cannot change them; they are kept when permissions are pushed.',
  'settings.bridgeSync.role.orderNote':
    'Role order is set here. A reorder on Discord is undone.',

  // ---- channel settings
  'settings.bridgeSync.channel.mirrored': 'Mirrored to Discord',
  'settings.bridgeSync.channel.held': 'Deleted on Discord. Held here until it is restored or removed.',
  'settings.bridgeSync.channel.frozen': 'Sync frozen',
  'settings.bridgeSync.channel.degraded': 'Mirrored as a simpler type: {reason}',
  'settings.bridgeSync.channel.srOnlyRow': 'SIDE ROOM only',
  'settings.bridgeSync.channel.srOnlyNote':
    'Permissions for a single person apply here only. Nothing is sent to Discord.',

  // ---- member list
  'lounge.bridgeSync.memberTag': 'On Discord',

  // ---- teardown (whole server)
  'settings.bridgeSync.teardown.title': 'Switch off the whole-server bridge?',
  'settings.bridgeSync.teardown.leaveTitle': 'Discord after switching off',
  'settings.bridgeSync.teardown.leave':
    'Leave Discord as it is. Roles, categories, channels and threads stay there with the permissions last pushed.',
  'settings.bridgeSync.teardown.remove':
    'Also remove what the bridge created on Discord ({roles, plural, one {# role} other {# roles}}, {categories, plural, one {# category} other {# categories}}, {channels, plural, one {# channel} other {# channels}}, {threads, plural, one {# thread} other {# threads}}).',
  'settings.bridgeSync.teardown.removeWarn':
    '{n, plural, one {# of those channels has messages from Discord members; they will be lost on Discord.} other {# of those channels have messages from Discord members; they will be lost on Discord.}}',
  'settings.bridgeSync.teardown.restore':
    'Also put Discord’s permissions back as they were before the bridge.',
  'settings.bridgeSync.teardown.noSnapshot': 'No earlier copy of Discord’s permissions exists.',
  'settings.bridgeSync.teardown.neverDeletes':
    'The bot never deletes anything on Discord that it did not create. Delete those by hand on Discord.',
  'settings.bridgeSync.teardown.herestays':
    'Nothing is deleted here. Roles, channels and threads that came from Discord are now ordinary ones.',
  'settings.bridgeSync.teardown.loadingPreview': 'Counting what the bridge created on Discord',
  'settings.bridgeSync.teardown.residueTitle': 'Some things are still on Discord',
  'settings.bridgeSync.teardown.residueBody': 'Discord refused to remove these. Remove them by hand.',
  'settings.bridgeSync.teardown.residueList': 'Ids to remove by hand',

  // ---- privacy surfaces
  'lounge.bridgeSync.bannerWhole':
    'This whole server is mirrored with a Discord server. Everything here except direct messages and voice is shared with it, and messages from Discord carry a DISCORD tag.',
  'help.bridgeSync.wholeNotice':
    'Everything in {server} except direct messages and voice is mirrored to Discord. Who can read it there is decided by Discord’s role assignments.',
} satisfies Record<string, string>;
