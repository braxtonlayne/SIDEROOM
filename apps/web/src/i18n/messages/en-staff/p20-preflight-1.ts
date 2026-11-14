export default {
  'admin.preflight.title': 'Preflight',
  'admin.preflight.setupTitle': 'Final step: preflight',
  'admin.preflight.description':
    'Runs the same checks as the sr-cli preflight command on this server: database, storage, mail, public URL, TURN, disk, clock and secret files.',
  'admin.preflight.run': 'Run preflight',
  'admin.preflight.running': 'Running checks…',
  'admin.preflight.never': 'Not run yet.',
  'admin.preflight.ranAt': 'Last run {at}.',
  'admin.preflight.summary':
    '{passed, plural, one {# check passed} other {# checks passed}}, {warned, plural, one {# warning} other {# warnings}}, {failed, plural, one {# failed} other {# failed}}.',
  'admin.preflight.ready': 'Ready: nothing failed.',
  'admin.preflight.notReady': 'Not ready: fix the failing checks before the first invite.',
  'admin.preflight.tableLabel': 'Preflight results',
  'admin.preflight.statusPass': 'Pass',
  'admin.preflight.statusWarn': 'Warning',
  'admin.preflight.statusFail': 'Fail',
} satisfies Record<string, string>;
