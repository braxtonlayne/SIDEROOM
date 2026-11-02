export default {
  // ---- Settings polish (XC-VIS-002) ----
  'settings.polishSettings.roleMembers': '{count, plural, one {# member} other {# members}}',
  // Audit log: a role or channel that no longer exists (Discord's wording for a mention of one).
  'settings.polishSettings.auditRoleGone': '@deleted-role',
  'settings.polishSettings.auditChannelGone': '#deleted-channel',
  // Admin > Invites: the last cleanup line, pluralized ("1 entries" before).
  'settings.polishSettings.lastPrune':
    'Last cleanup {when}: {revoked, plural, one {# unused invite} other {# unused invites}} revoked, {marked, plural, one {# inactive subtree} other {# inactive subtrees}} newly marked. The invite graph still has {graph, plural, one {# entry} other {# entries}}; the cleanup never deletes any.',
} satisfies Record<string, string>;
