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
} satisfies Record<string, string>;
