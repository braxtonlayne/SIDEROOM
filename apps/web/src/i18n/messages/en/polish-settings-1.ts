export default {
  // ---- Settings polish (XC-VIS-002) ----
  'settings.polishSettings.roleMembers': '{count, plural, one {# member} other {# members}}',
  // Audit log: a role or channel that no longer exists (Discord's wording for a mention of one).
  'settings.polishSettings.auditRoleGone': '@deleted-role',
  'settings.polishSettings.auditChannelGone': '#deleted-channel',
} satisfies Record<string, string>;
