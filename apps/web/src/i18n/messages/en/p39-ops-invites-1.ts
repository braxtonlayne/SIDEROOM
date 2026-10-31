// OPS-INV-001: Lock the Doors, the invite quotas and the invites.prune job on Admin > Invites. The entrance
// notice ("entrance.doors.*") is in entrance.ts: the entrance paints before the rest of the catalog loads.
export default {
  'admin.invites.doorsTitle': 'Lock the Doors',
  'admin.invites.doorsIntro':
    'While the doors are locked, no registration succeeds, even with a valid invite, and the entrance says the doors are closed. People who are already in are not affected. Each change is written to the audit log with your reason.',
  'admin.invites.doorsLoading': 'Loading the invite controls',
  'admin.invites.doorsState': 'The doors are',
  'admin.invites.doorsClosed': 'Closed',
  'admin.invites.doorsOpen': 'Open',
  'admin.invites.doorsClosedHelp': 'Nobody can register until you open the doors again.',
  'admin.invites.doorsOpenHelp': 'Anyone with a valid invite can register.',
  'admin.invites.doorsLock': 'Lock the doors',
  'admin.invites.doorsUnlock': 'Open the doors',
  'admin.invites.doorsNeedFeatures': 'Locking the doors needs the Manage features permission.',
  'admin.invites.lockTitle': 'Lock the doors?',
  'admin.invites.lockDescription':
    'No new account can be created until you open the doors again. Say why; it goes into the audit log.',
  'admin.invites.unlockTitle': 'Open the doors?',
  'admin.invites.unlockDescription':
    'Registration with a valid invite works again. Say why; it goes into the audit log.',
  'admin.invites.locked': 'The doors are locked.',
  'admin.invites.unlocked': 'The doors are open.',
  'admin.invites.quotasLabel': 'Invite quotas',
  'admin.invites.quotaCredits': 'Credits a new account starts with',
  'admin.invites.quotaOutstanding': 'Most unused invites one account may hold',
  'admin.invites.quotaDays': 'Days before an unused invite is revoked',
  'admin.invites.quotasEdit':
    'Change these three numbers under Limits (the invites group). They apply at once; accounts and codes that already exist keep what they have.',
  'admin.invites.lastPrune':
    'Last cleanup {when}: {revoked} unused invites revoked, {marked} inactive subtrees newly marked. The invite graph still has {graph} entries; the cleanup never deletes any.',
  'admin.invites.neverPruned':
    'The cleanup job (invites.prune) has not run yet. It revokes unused invites older than the limit above and marks the subtrees whose first member never activated. It never deletes the invite graph.',
  'admin.invites.inactiveTitle': 'Inactive subtrees',
  'admin.invites.inactiveIntro':
    'Members who joined with an invite and never activated (email not verified, never seen online) after the limit above, with everyone they invited. This is a view only; nothing is removed.',
  'admin.invites.inactiveLoading': 'Loading inactive subtrees',
  'admin.invites.inactiveEmpty': 'No inactive subtrees.',
  'admin.invites.inactiveRow': 'Subtree of {size}, marked {when}',
  'admin.invites.inactiveMore': 'Showing {shown} of {total}.',
  'settings.userInvites.blockedAtMax':
    'You already hold the most unused invites allowed. Use or revoke one first.',
};
