/**
 * Wave 71, ops: the Admin > Bug reports page. A report whose parts are missing reads as "Unknown" (SEC-AUD-266) and the
 * list pages with "Load more" (SEC-AUD-267). Admin strings go in `en/`: `scripts/staff-keys.mjs --write` moves what only
 * staff code names.
 */
export default {
  'admin.bugs.unknown': 'Unknown',
  'admin.bugs.loadMore': 'Load more',
  'admin.bugs.loadMoreFailed': 'Could not load more reports. Try again.',
} satisfies Record<string, string>;
