export default {
  // G-18 Share Target files
  'pwa.share.files': '{n, plural, one {# file attached} other {# files attached}}',
  // G-35 Community wizard safety switches
  'settings.communityWizard.alreadyOn': 'Already on for this server.',
  'settings.communityWizard.safetyNeeded': 'Turn both switches on to continue.',
  // G-30 Follow an announcement channel
  'lounge.followButton.label': 'Follow',
  'lounge.followButton.tooltip': "Get this channel's announcements in one of your channels",
  'lounge.followButton.title': 'Follow this channel',
  'lounge.followButton.description':
    'Announcements published here are copied into the channel you choose.',
  'lounge.followButton.channel': 'Post announcements in',
  'lounge.followButton.choose': 'Choose a channel',
  'lounge.followButton.loading': 'Loading your channels…',
  'lounge.followButton.loadError': 'Could not load your channels. Try again in a moment.',
  'lounge.followButton.forbidden': 'You need the Manage Webhooks permission in that channel.',
  'lounge.followButton.failed': 'Could not follow this channel. Try again in a moment.',
  'lounge.followButton.confirm': 'Follow',
  'lounge.followButton.cancel': 'Cancel',
  'lounge.followButton.close': 'Close',
  'lounge.followButton.done': 'Done. Announcements will appear in #{channel}.',
  'lounge.channelView.badgeUpdates': 'Updates',
  // G-59 PDF reading in the browser
  'library.pdf.backToSeries': 'Back to the series',
  'library.pdf.missingTitle': 'PDF reading is not installed',
  'library.pdf.missingDescription':
    'This server was built without the PDF viewer (pdf.js). Ask the operator to install it, or download the chapter if you may.',
  'library.pdf.failedTitle': 'This PDF could not be opened',
  'library.pdf.failedDescription': 'The file may be damaged. Try again in a moment.',
  'library.pdf.previous': 'Previous page',
  'library.pdf.next': 'Next page',
  'library.pdf.pageOf': 'Page {page} of {count}',
  'library.pdf.pageLabel': 'Page {page}',
} satisfies Record<string, string>;
