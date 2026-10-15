export default {
  'settings.members.sortBy': 'Sort by',
  'settings.members.sortName': 'Name (A to Z)',
  'settings.members.sortNewest': 'Joined (newest first)',
  'settings.members.sortOldest': 'Joined (oldest first)',
  'settings.members.selectAll': 'Select all members on this page',
  'settings.members.selectMember': 'Select {name}',
  'settings.members.bulkBar': 'Bulk actions',
  'settings.members.selected':
    '{count, plural, one {# member selected} other {# members selected}}',
  'settings.members.clearSelection': 'Clear selection',
  'settings.members.bulkAddRole': 'Add role',
  'settings.members.bulkRemoveRole': 'Remove role',
  'settings.members.bulkRole': 'Role',
  'settings.members.bulkTitle': '{action}: {count, plural, one {# member} other {# members}}',
  'settings.members.bulkRoleDescription':
    'The role is changed on {count, plural, one {# member} other {# members}}. Members you cannot manage are reported when it finishes.',
  'settings.members.bulkKickDescription':
    '{count, plural, one {# member} other {# members}} will be removed from the server. Members you cannot moderate are skipped.',
  'settings.members.bulkBanDescription':
    '{count, plural, one {# member} other {# members}} will be banned. Members you cannot moderate are skipped.',
  'settings.members.bulkDone':
    '{count, plural, one {Done for # member.} other {Done for # members.}}',
  'settings.members.bulkPartial': 'Done for {ok}, failed for {failed}. The failures stay selected.',
} satisfies Record<string, string>;
