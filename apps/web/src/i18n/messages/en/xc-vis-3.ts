// XC-VIS-003 (owner-delegated polish decisions): strings of the signed-in app. The entrance's live in entrance.ts.
export default {
  // G5: a channel with an Activity running offers to join it, not to start another.
  'lounge.composer.joinActivity': 'Join {name}',
  'activities.launcher.runningNamed': '{name} is running in this channel.',
  'activities.launcher.join': 'Join',
  // T4: the per-row menu of Server Settings > Members (Timeout, Kick, Ban).
  'settings.members.moreActions': 'More actions for {name}',
  // M3: a not-ready or failed video goes back to its title page.
  'cinema.watch.backToTitle': 'Back to the title page',
} satisfies Record<string, string>;
