export default {
  // ---- Admin > Moderation > Takedowns: the fingerprint of a delisted file (D-595) ----
  'admin.takedowns.digest.recorded': 'Fingerprint: recorded',
  'admin.takedowns.digest.pending': 'Fingerprint: being taken',
  'admin.takedowns.digest.waitingMount':
    'Fingerprint: waiting, the owner has switched this bucket off',
  'admin.takedowns.digest.tagOnly': 'Fingerprint: none, matched by tag and size only',
} as const;
