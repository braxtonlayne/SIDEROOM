export default {
  'settings.account.removalMessages':
    'Messages you leave in place stay. Delete them first if you want them gone.',
  'settings.privacy.deleteMessages.title': 'Delete all my messages',
  'settings.privacy.deleteMessages.description':
    'Hide every message and upload you wrote, in servers and in direct messages, in one go.',
  'settings.privacy.deleteMessages.servers': 'Messages in servers (including servers you left)',
  'settings.privacy.deleteMessages.dms': 'Messages in direct messages and group DMs',
  'settings.privacy.deleteMessages.before': 'Only messages before',
  'settings.privacy.deleteMessages.beforeHint': 'Leave empty to delete everything.',
  'settings.privacy.deleteMessages.count':
    '{messages} messages, {reactions} reactions, {votes} poll votes would be removed.',
  'settings.privacy.deleteMessages.nothing': 'Nothing to delete here.',
  'settings.privacy.deleteMessages.note':
    'They are hidden from everyone at once and purged after 30 days; messages under a moderation hold are kept for staff. Your reactions and poll votes go too; a closed poll keeps its totals without your name. You can run this once every 24 hours.',
  'settings.privacy.deleteMessages.button': 'Delete my messages',
  'settings.privacy.deleteMessages.queued':
    'Started. It runs in the background and you will get a message when it is done.',
  'settings.privacy.deleteMessages.running': 'A bulk delete is running.',
  'settings.privacy.deleteMessages.lastRun': 'Last run {when}: {count} messages hidden.',
  'settings.privacy.deleteMessages.confirmTitle': 'Delete all my messages?',
  'settings.privacy.deleteMessages.confirmDescription':
    '{messages} messages, {reactions} reactions and {votes} poll votes will be removed for everyone. This cannot be undone.',
  'settings.privacy.deleteMessages.confirm': 'Delete messages',
  'settings.privacy.deleteMessages.cancel': 'Cancel',
  'settings.account.removalMessagesLink': 'Delete all my messages',
} satisfies Record<string, string>;
