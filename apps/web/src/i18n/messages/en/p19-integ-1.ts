export default {
  // ---- Case timeline: a Person link folded two automatic suspicion cases (phase 19 integration, migration 1152) ----
  'admin.moderation.event.suspicionFolded':
    'The accounts were linked to another Person; its automatic case {into} continues the review.',

  // ---- Dashboard > Retention check: holds after an erasure and the sign-in purge (MOD-DASH-009, D-347) ----
  'admin.moderation.dash.retention.holdsErasure':
    'Holds a deletion request met, waiting for a decision',
  'admin.moderation.dash.retention.signInsLabel': 'Sign-in history purge',
  'admin.moderation.dash.retention.signInsPast':
    'Sign-in history older than {days, number} days still stored (deleted daily, no hold exception)',
  'admin.moderation.dash.retention.signInsLast':
    'Last run deleted {n, plural, one {# row} other {# rows}} ({when}).',
  'admin.moderation.dash.retention.signInsNever': 'The purge job has not run yet.',
  'admin.moderation.dash.retention.signInsBacklog':
    'An old backlog remains; the job runs again within minutes.',
} satisfies Record<string, string>;
