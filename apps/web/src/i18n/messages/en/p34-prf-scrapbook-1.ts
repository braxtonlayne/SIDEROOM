// PRF-SCR-001 (wave 34): the scrapbook vault (gallery, message menu entry, channel banner, shelf panel).
export default {
  'lounge.scrapbook.title': 'Scrapbook',
  'lounge.scrapbook.nav': 'Scrapbook',
  'lounge.scrapbook.off': 'The scrapbook is not turned on for this server.',
  'lounge.scrapbook.loading': 'Loading...',
  'lounge.scrapbook.loadFailed': 'The scrapbook could not be loaded.',
  'lounge.scrapbook.empty': 'Nothing has been kept yet.',
  'lounge.scrapbook.more': 'Show more',
  'lounge.scrapbook.howItWorks':
    "Messages that collect {n, plural, one {# reaction} other {# reactions}} of {emoji} are kept here, with their files, in this server's own storage.",
  'lounge.scrapbook.banner':
    "A message with {n, plural, one {# reaction} other {# reactions}} of {emoji} is kept in this server's Scrapbook, with its files.",
  'lounge.scrapbook.send': 'Send to Scrapbook',
  'lounge.scrapbook.notShared': '"{name}" is not shared with you.',
  'lounge.scrapbook.skipped':
    '{n, plural, one {# file was too large or over the limit and was not kept.} other {# files were too large or over the limit and were not kept.}}',
  'lounge.scrapbook.tags': 'Tags',
  'lounge.scrapbook.tagsInput': 'Tags, separated by commas',
  'lounge.scrapbook.editTags': 'Edit tags',
  'lounge.scrapbook.saveTags': 'Save tags',
  'lounge.scrapbook.remove': 'Remove',
  'lounge.scrapbook.removeConfirm':
    "Remove this from the Scrapbook? Its copied files are deleted from the server's storage. The original message is not touched.",
  'lounge.scrapbook.actionFailed': 'That did not work. Try again.',
  'lounge.scrapbook.copyPending':
    "Removed. A copied file could not be deleted from the storage yet; the server's managers can see it and it will be tried again.",
  'lounge.scrapbook.from': 'From',
  'lounge.scrapbook.to': 'To',
  'lounge.scrapbook.channel': 'Channel',
  'lounge.scrapbook.tag': 'Tag',
  'lounge.scrapbook.member': 'Member',
  'lounge.scrapbook.any': 'Any',
  'lounge.scrapbook.shelf': 'Storage shelf',
  'lounge.scrapbook.shelfOk': 'Copies go to the shelf "{label}".',
  'lounge.scrapbook.shelfProblem': 'The shelf "{label}" cannot be written to: {problem}',
  'lounge.scrapbook.shelfNone':
    "No shelf is chosen, so nothing can be kept yet. Choose one of this server's storage shelves.",
  'lounge.scrapbook.shelfPick': 'Choose a shelf',
  'lounge.scrapbook.shelfSet': 'Use this shelf',
  'lounge.scrapbook.shelfClear': 'Stop using it',
  'lounge.scrapbook.shelfHelp':
    'Choosing a shelf lets the Scrapbook upload to it as you: you need the upload right on the shelf yourself, and the Scrapbook stops if you lose it.',
  'lounge.scrapbook.shelfFailed':
    'That shelf cannot be used. It must belong to this server, be switched on, and you need the upload right on it.',
  'lounge.scrapbook.removalsTitle': 'Copies that could not be removed',
  'lounge.scrapbook.removalsNone': "Every removed entry's files are gone from the storage.",
  'lounge.scrapbook.removalFailed':
    '{n, plural, one {Failed once} other {Failed # times}}: {error}. It is tried again automatically.',
  'lounge.scrapbook.retry': 'Try again now',
  'lounge.scrapbook.dismiss': 'I deleted it myself',
} satisfies Record<string, string>;
