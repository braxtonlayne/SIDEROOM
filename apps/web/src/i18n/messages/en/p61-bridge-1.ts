// SEC-AUD-129 (D-623) and SEC-AUD-130 (D-626): the bridge screen says when another server bridges the same Discord
// channel, and lists the Discord messages of deleted accounts the bridge could not remove.
export default {
  'settings.bridge.shared.warning':
    'Another server on this instance also bridges this Discord channel. Messages from both appear there. Remove this map to stop.',
  'settings.bridge.erasure.title': 'Messages to delete on Discord',
  'settings.bridge.erasure.notice':
    'Messages of deleted accounts that could not be removed from Discord. Delete them on Discord by hand; this list empties by itself within a day.',
  'settings.bridge.erasure.entry': '#{channel}, message {id}',
} satisfies Record<string, string>;
