// ACT-SDK-001 (wave 49, D-466): the shelf picker an Activity opens with `pickShelfObject`.
export default {
  'activities.pick.title': 'Choose a file for {name}',
  'activities.pick.description':
    'Only the file you choose is shared with this Activity. It never sees the rest of your shelves.',
  'activities.pick.shelf': 'Shelf',
  'activities.pick.shelfPlaceholder': 'Choose a shelf',
  'activities.pick.search': 'Search files',
  'activities.pick.searchPlaceholder': 'Type to narrow the list',
  'activities.pick.loading': 'Loading files…',
  'activities.pick.noShelves': 'You have no shelves to choose from yet.',
  'activities.pick.empty': 'No files here that this Activity can use.',
  'activities.pick.noMatch': 'No file matches that search.',
  'activities.pick.error': 'That shelf could not be read. Try another one.',
  'activities.pick.hidden':
    '{count, plural, one {# file is hidden because it is not the right kind or is too big.} other {# files are hidden because they are not the right kind or are too big.}}',
  'activities.pick.more': 'Showing the first {count}. Type to narrow the list.',
  'activities.pick.truncated':
    'This shelf has more than {count} files; only the first {count} are listed.',
  'activities.pick.files': 'Files',
  'activities.pick.cancel': 'Cancel',
  'activities.pick.mine': 'My files',
  'activities.pick.friend': 'Shared by {name}',
  'activities.pick.guild': 'Server: {name}',
} satisfies Record<string, string>;
