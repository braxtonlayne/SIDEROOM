// ARC-ROOM-001 (wave 42): The Arcade room.
export default {
  'arcade.admin.section': 'Arcade',
  'arcade.admin.heading': 'Arcade',
  'arcade.admin.intro':
    'Choose what the Front Page features. Only released Activities whose build declares a content tier can be featured. Every change is written to the audit log.',
  'arcade.admin.loading': 'Loading…',
  'arcade.admin.failed': 'That did not work: {reason}',
  'arcade.admin.candidates': 'Games that can be featured',
  'arcade.admin.noCandidates': 'No released Activity declares a tier yet.',
  'arcade.admin.feature': 'Feature {name}',
  'arcade.admin.unfeature': 'Remove {name} from the Front Page',
  'arcade.admin.featured': 'Featured',
  'arcade.admin.soonHeading': 'Coming soon showcase',
  'arcade.admin.soonTitle': 'Title',
  'arcade.admin.soonSummary': 'Summary',
  'arcade.admin.soonTier': 'Content tier',
  'arcade.admin.soonRepo': 'Repository link (https)',
  'arcade.admin.soonAdd': 'Add to the showcase',
  'arcade.admin.soonRemove': 'Remove {title}',
} satisfies Record<string, string>;
