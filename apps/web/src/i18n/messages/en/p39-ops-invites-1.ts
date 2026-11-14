// OPS-INV-001: Lock the Doors, the invite quotas and the invites.prune job on Admin > Invites. The entrance
// notice ("entrance.doors.*") is in entrance.ts: the entrance paints before the rest of the catalog loads.
export default {
  'admin.invites.lastPrune':
    'Last cleanup {when}: {revoked} unused invites revoked, {marked} inactive subtrees newly marked. The invite graph still has {graph} entries; the cleanup never deletes any.',
  'settings.userInvites.blockedAtMax':
    'You already hold the most unused invites allowed. Use or revoke one first.',
};
