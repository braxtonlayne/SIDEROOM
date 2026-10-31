// Wave 35b integration (BRG-SYN-001 to 004): the words the reconciled bridge API builds from the server's shapes
// (plan changes, conflict values, what the bridge did) and the audit-log texts of the bridge's actions 220 to 224.
export default {
  'settings.bridgeSync.integ.changeGains': 'Role {name} on Discord gains {list}.',
  'settings.bridgeSync.integ.changeLoses': 'Role {name} on Discord loses {list}.',
  'settings.bridgeSync.integ.none': 'nothing',
  'settings.bridgeSync.integ.allowDeny': 'allows {allow}; denies {deny}',
  'settings.bridgeSync.integ.order': 'the role order',
  'settings.bridgeSync.integ.discordBit': 'Discord permission {n}',
  'settings.bridgeSync.integ.didRevert': 'Put the permissions set here back on Discord.',
  'settings.bridgeSync.integ.didTrim':
    'Removed what the overwrite allowed on Discord; what it denies stays.',
  'settings.bridgeSync.integ.didStructure': 'Kept the value set here and sent it to Discord.',
  'settings.bridgeSync.integ.didFrozen':
    'Stopped syncing this until someone unfreezes it. Nothing here was changed.',
  'settings.bridgeSync.integ.didHeld':
    'Hid it here. It is deleted when the hold ends unless someone restores it.',
  'settings.bridgeSync.integ.didRoleKept':
    'Made the role again on Discord: it denies something in a channel here.',
  'settings.bridgeSync.integ.didDeadOp': 'Gave up after eight tries.',
  'settings.bridgeSync.integ.didCap': 'Left it on one side: Discord’s limit was reached.',
  // ---- the server audit log (actions 220 to 224)
  'settings.auditText.a220': 'changed how much of the server is bridged to Discord',
  'settings.auditText.a221': 'synced the server with Discord',
  'settings.auditText.a222': 'put back permissions that were changed on Discord',
  'settings.auditText.a223': 'logged a Discord bridge conflict',
  'settings.auditText.a224': 'removed what the bridge created on Discord',
};
