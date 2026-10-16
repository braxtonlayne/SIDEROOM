export default {
  // Activities: portal tab (DEV-PORT-003, DEV-ACT-001)
  'developers.appDetail.tabActivities': 'Activities',
  'developers.activities.what': 'Activity settings',
  'developers.activities.unavailable': 'Activities are not available on this server.',
  'developers.activities.heading': 'Activity',
  'developers.activities.intro':
    'An Activity is a web app that runs inside a voice channel. It runs on its own address, in a sandbox, and staff review it before it can be started.',
  'developers.activities.disabled':
    'Activities are not switched on for this server yet. You can prepare and submit your Activity, but nobody can start it until the operator turns Activities on.',
  'developers.activities.originLabel': 'Your Activity address:',
  'developers.activities.copyRedirect': 'Copy redirect URI',
  'developers.activities.redirectHint':
    'Add {uri} to the redirect URIs on the OAuth2 tab so your Activity can ask people to sign in.',
  'developers.activities.entryUrl': 'Entry URL',
  'developers.activities.entryHint':
    'The page your Activity starts from, on your own server. Must be https.',
  'developers.activities.mappingsHeading': 'URL mappings',
  'developers.activities.mappingsIntro':
    'Your Activity can only talk to its own address. A mapping sends requests under a path, like /api, to a server of yours. Files are served read-only (GET).',
  'developers.activities.mappingsLabel': 'URL mappings',
  'developers.activities.prefix': 'Path',
  'developers.activities.target': 'Target',
  'developers.activities.addMapping': 'Add a mapping',
  'developers.activities.remove': 'Remove',
  'developers.activities.removeMapping': 'Remove the mapping for {prefix}',
  'developers.activities.save': 'Save',
  'developers.activities.saveFailed': 'The Activity settings could not be saved.',
  'developers.activities.reviewHeading': 'Review',
  'developers.activities.stateDraft': 'Not submitted. Changes after an approval need a new review.',
  'developers.activities.statePending': 'Waiting for staff to review it.',
  'developers.activities.stateApproved': 'Approved. People can start it in voice channels.',
  'developers.activities.stateRejected':
    'Not approved. Change what the note asks and submit again.',
  'developers.activities.reviewerNote': 'Reviewer note: {note}',
  'developers.activities.approvedHash': 'Approved entry document (SHA-256):',
  'developers.activities.submitHint':
    'Submitting fetches your entry page now and remembers its fingerprint. Only that exact page is served after approval, so a changed page waits for a new review.',
  'developers.activities.submit': 'Submit for review',
  'developers.activities.resubmit': 'Submit again',
  'developers.activities.submitFailed': 'The Activity could not be submitted.',

  // Activities: launcher, call bar and panel (DEV-ACT-004)
  'activities.button.start': 'Start an Activity',
  'activities.button.join': 'Join {name}',
  'activities.launcher.title': 'Activities',
  'activities.launcher.description':
    'Pick an Activity to start for everyone in this voice channel.',
  'activities.launcher.close': 'Close',
  'activities.launcher.empty': 'No Activities are available yet.',
  'activities.launcher.listLabel': 'Available Activities',
  'activities.launcher.start': 'Start',
  'activities.launcher.startLabel': 'Start {name}',
  'activities.launcher.loadFailed': 'The Activities could not be loaded.',
  'activities.launcher.forbidden': 'You need permission to use Activities in this channel.',
  'activities.launcher.running': 'An Activity is already running in this channel.',
  'activities.launcher.failed': 'The Activity could not be started.',
  'activities.panel.label': '{name} Activity',
  'activities.panel.playing': '{n, plural, one {# person} other {# people}}',
  'activities.panel.join': 'Join',
  'activities.panel.leave': 'Leave',
  'activities.panel.end': 'End for everyone',
  'activities.frame.title': '{name} (Activity)',
  'activities.confirm.authorizeTitle': 'Let {name} identify you?',
  'activities.confirm.authorizeBody':
    'This Activity asks for access to: {scopes}. It can use this until you remove it under Authorized Apps.',
  'activities.confirm.authorize': 'Allow',
  'activities.confirm.deny': 'Not now',
  'activities.confirm.linkTitle': '{name} wants to open a link',
  'activities.confirm.linkWarning': 'Only open links you trust. The link opens in a new tab.',
  'activities.confirm.openLink': 'Open link',
} satisfies Record<string, string>;
