export default {
  // Server settings: join-date filter on the members table (H-49, LNG-SSET-027)
  'settings.members.joinedAfter': 'Joined after',
  'settings.members.joinedBefore': 'Joined before',
  // Lounge: the refusal of an oversize file also points free members to Patron (H-22, D-233)
  'lounge.composer.fileTooLargePatron':
    '{count, plural, one {{name} is larger than the {max} upload limit here. Patrons can upload larger files.} other {# files are larger than the {max} upload limit here. Patrons can upload larger files.}}',
};
