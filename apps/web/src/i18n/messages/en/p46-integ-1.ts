// D-389 (wave 46): an Activity that declares byos_read asks before it opens one file of a shelf.
export default {
  'activities.confirm.shelfTitle': 'Let {name} open a file from your shelves?',
  'activities.confirm.shelfBody':
    'The game asks for one file of a shelf you can open. It gets a link to that one file that works for up to ten minutes, and nothing else from your shelves.',
  'activities.confirm.shelfAllow': 'Allow',
} satisfies Record<string, string>;
