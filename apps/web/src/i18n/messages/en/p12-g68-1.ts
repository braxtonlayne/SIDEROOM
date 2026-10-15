export default {
  // Suggestion votes: the viewer's own vote (lounge/community/Suggestions.tsx, G-72)
  'lounge.community.suggestions.youVotedUp': '(you voted up)',
  'lounge.community.suggestions.youVotedDown': '(you voted down)',
  // Share target opened without anything shared (pwa/SharePage.tsx, G-77)
  'pwa.share.openOnce': 'Sharing from another app? Open SIDE ROOM once, then share again.',
  // Edit a showing announcement (admin/sections/Announcements.tsx, G-69)
  'admin.announcements.editAria': 'Edit {title}',
  'admin.announcements.edit': 'Edit',
  'admin.announcements.editTitle': 'Edit announcement',
  'admin.announcements.editSave': 'Save changes',
  'admin.announcements.edited': 'Announcement updated.',
  'admin.announcements.editFixed':
    'Severity and the message to everyone cannot change, and nobody is messaged again.',
  // Delete account (settings/user/sections/DeleteAccount.tsx, G-82)
  'settings.account.deleteButton': 'Delete my account',
  'settings.account.deleteTitle': 'Delete your account',
  'settings.account.deleteDescription':
    'This cannot be undone. You are signed out everywhere, your username is held for 30 days, and you need a new invite to return.',
  'settings.account.deleteTypeUsername': 'Type your username, {username}, to confirm',
  'settings.account.deleteUsernameMismatch': 'Type your username exactly to confirm.',
  'settings.account.deleteCodeLabel': 'Authenticator or recovery code',
  'settings.account.deleteEnterCode': 'Enter a code from your authenticator app or a recovery code.',
  'settings.account.deleteConfirm': 'Delete my account for good',
  'settings.account.deleteStaff':
    'You are on the staff. Leave the Staff Server first, then delete your account.',
  'settings.account.deleteOwner':
    'You are the Owner. Transfer ownership first, then delete your account.',
  // Slowmode presets on demand (lounge/features/SlowmodeForm.tsx, lounge/shell/SlowmodeDialog.tsx, G-74)
  'lounge.slowmode.title': 'Slowmode preset for #{name}',
  'lounge.slowmode.description':
    'Set the slowmode of this channel from one of the server presets. It can go back by itself.',
  'lounge.slowmode.close': 'Close',
  'lounge.slowmode.loading': 'Loading the presets',
  'lounge.slowmode.loadFailed': 'Could not load the slowmode presets.',
  'lounge.slowmode.moduleOff':
    'Slowmode schedules are not turned on for this server. A manager can turn them on in Server Settings, Features.',
  'lounge.slowmode.preset': 'Preset',
  'lounge.slowmode.duration': 'How long',
  'lounge.slowmode.keep': 'Until somebody changes it',
  'lounge.slowmode.min15': 'For 15 minutes',
  'lounge.slowmode.hour1': 'For 1 hour',
  'lounge.slowmode.hour4': 'For 4 hours',
  'lounge.slowmode.day1': 'For 24 hours',
  'lounge.slowmode.channels': 'Channels',
  'lounge.slowmode.apply': 'Apply preset',
  'lounge.slowmode.failed': 'Could not apply the preset. Check that you can manage these channels.',
  'lounge.slowmode.off': 'off',
  'lounge.slowmode.secs': '{n, plural, one {# second} other {# seconds}}',
  'lounge.slowmode.result': 'Applied',
  'lounge.slowmode.applied': '#{channel} now has slowmode {now}.',
  'lounge.slowmode.appliedUntil':
    '#{channel} now has slowmode {now}. It goes back to {before} at {when}.',
  'lounge.slowmode.nothing': 'None of those channels could be changed.',
  'lounge.slowmode.panelTitle': 'Apply a preset now',
  'lounge.slowmode.panelHint':
    'Set the slowmode of channels right away, for a while or until you change it. Members with Manage Channels can also do this from the channel menu.',
} satisfies Record<string, string>;
