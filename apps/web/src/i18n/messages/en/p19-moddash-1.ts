export default {
  // ---- Admin > Moderation > Dashboard (MOD-DASH-002, -005 to -010; spec 7 and 9) ----
  'admin.moderation.tabDashboard': 'Dashboard',
  'admin.moderation.dash.toolbar': 'Dashboard options',
  'admin.moderation.dash.window': 'Time window',
  'admin.moderation.dash.windowDays': 'Last {n, plural, one {# day} other {# days}}',
  'admin.moderation.dash.refresh': 'Refresh',
  'admin.moderation.dash.generated': 'Figures as of {when}. Totals only: nobody is named.',

  'admin.moderation.dash.inbox.title': 'Reports inbox',
  'admin.moderation.dash.inbox.description':
    'Reports nobody has picked up yet. Several reports about the same thing sit in one group. Who sent them is not shown here.',
  'admin.moderation.dash.inbox.groups': 'Waiting groups',
  'admin.moderation.dash.inbox.reports': 'Reports waiting',
  'admin.moderation.dash.inbox.oldest': 'Oldest report',
  'admin.moderation.dash.inbox.emptyTitle': 'Inbox empty',
  'admin.moderation.dash.inbox.emptyDescription': 'Every report has been picked up by someone.',
  'admin.moderation.dash.inbox.selectAll': 'Select all',
  'admin.moderation.dash.inbox.dismissSelected':
    'Dismiss {n, plural, =0 {selected} other {# selected}}',
  'admin.moderation.dash.inbox.listLabel': 'Reports waiting in the inbox',
  'admin.moderation.dash.inbox.pick': 'Select case {id}',
  'admin.moderation.dash.inbox.duplicates': '{n} duplicate reports',
  'admin.moderation.dash.inbox.reportCount': '{n, plural, one {# report} other {# reports}}',
  'admin.moderation.dash.inbox.first': 'first {when}',
  'admin.moderation.dash.inbox.openAria': 'Open case {id}',
  'admin.moderation.dash.inbox.open': 'Open case',
  'admin.moderation.dash.inbox.dismissTitle': 'Dismiss {n, plural, one {# case} other {# cases}}',
  'admin.moderation.dash.inbox.dismissDescription':
    'Every chosen case is dismissed with this reason. Each one gets its own entry in its timeline and in the access log.',
  'admin.moderation.dash.inbox.dismissConfirm': 'Dismiss',
  'admin.moderation.dash.inbox.dismissed':
    '{done, plural, one {# case dismissed} other {# cases dismissed}}; {skipped, plural, =0 {none skipped} one {# skipped because it left the inbox} other {# skipped because they left the inbox}}.',

  'admin.moderation.dash.health.title': 'Health',
  'admin.moderation.dash.health.description':
    'The last {n, plural, one {day} other {# days}}. Totals only; there is no figure per staff member.',
  'admin.moderation.dash.health.reports': 'Reports filed',
  'admin.moderation.dash.health.casesOpened': 'Cases opened',
  'admin.moderation.dash.health.median': 'Median time to first action',
  'admin.moderation.dash.health.waiting': '{n, plural, one {# case} other {# cases}} still waiting',
  'admin.moderation.dash.health.reopenRate': 'Reopen rate',
  'admin.moderation.dash.health.reopenHint': '{reopened} of {closed} closed cases reopened',
  'admin.moderation.dash.health.appeals': 'Appeals decided',
  'admin.moderation.dash.health.appealsValue': '{upheld} upheld, {overturned} overturned',
  'admin.moderation.dash.health.appealsOpen': '{n} waiting for a decision',
  'admin.moderation.dash.health.perDay': 'Reports per day',

  'admin.moderation.dash.takedowns.title': 'Takedown register',
  'admin.moderation.dash.takedowns.description': 'Notices, their states and due dates.',
  'admin.moderation.dash.takedowns.undecided': 'Notices to decide',
  'admin.moderation.dash.takedowns.overdue': 'Past the triage target',
  'admin.moderation.dash.takedowns.overdueHint': 'These need a decision first.',
  'admin.moderation.dash.takedowns.counterNoticed': 'Counter-notices pending',
  'admin.moderation.dash.takedowns.restoreDue': '{n} ready to restore',
  'admin.moderation.dash.takedowns.received':
    'Received in the last {n, plural, one {day} other {# days}}',
  'admin.moderation.dash.takedowns.removed': '{n} removed in that time',
  'admin.moderation.dash.takedowns.open': 'Open the register',

  'admin.moderation.dash.requests.title': 'Data requests',
  'admin.moderation.dash.requests.description': 'Exports and erasures with their deadlines.',
  'admin.moderation.dash.requests.exports': 'Open exports',
  'admin.moderation.dash.requests.erasures': 'Open erasures',
  'admin.moderation.dash.requests.overdue': 'Overdue',
  'admin.moderation.dash.requests.dueSoon': 'Due within 7 days',
  'admin.moderation.dash.requests.completed':
    'Completed in the last {n, plural, one {day} other {# days}}',
  'admin.moderation.dash.requests.open': 'Open the data requests',

  'admin.moderation.dash.bans.title': 'Platform bans',
  'admin.moderation.dash.bans.description': 'Bans in force, by reason, severity and age.',
  'admin.moderation.dash.bans.active': 'Active bans',
  'admin.moderation.dash.bans.lifted': 'Lifted in the last {n, plural, one {day} other {# days}}',
  'admin.moderation.dash.bans.byReason': 'Active bans by reason',
  'admin.moderation.dash.bans.bySeverity': 'Active bans by severity',
  'admin.moderation.dash.bans.byAge': 'Active bans by age',
  'admin.moderation.dash.bans.open': 'Open the bans list',

  'admin.moderation.dash.retention.title': 'Retention check',
  'admin.moderation.dash.retention.description':
    'Rows the purge jobs are leaving in place because a hold covers them, so you can see what is piling up.',
  'admin.moderation.dash.retention.holdsActive': 'Holds in force',
  'admin.moderation.dash.retention.holdsReview': 'Holds due for review',
  'admin.moderation.dash.retention.listLabel': 'Rows skipped by purge jobs',
  'admin.moderation.dash.retention.atLeast': 'at least {n, number}',
  'admin.moderation.dash.retention.messagesHeld': 'Deleted messages due for purge, held',
  'admin.moderation.dash.retention.messagesHeldAttachment':
    'Deleted messages due for purge, kept by a held attachment',
  'admin.moderation.dash.retention.attachmentsRemovedHeld':
    'Attachments taken off a message, kept by a hold',
  'admin.moderation.dash.retention.attachmentsExpiryHeld':
    'Large attachments past their expiry, kept by a hold',
  'admin.moderation.dash.retention.submissionsHeld': 'Draft or rejected submissions under a hold',

  'admin.moderation.dash.age.title': 'Age and child safety',
  'admin.moderation.dash.age.description':
    'Age-restricted items, resets of the age confirmation and child-safety work. Counts only.',
  'admin.moderation.dash.age.channels': 'Age-restricted channels',
  'admin.moderation.dash.age.series': 'Age-restricted series',
  'admin.moderation.dash.age.titles': 'Age-restricted titles',
  'admin.moderation.dash.age.resets':
    'Age confirmations reset in the last {n, plural, one {day} other {# days}}',
  'admin.moderation.dash.age.openCases': 'Open child-safety cases',
  'admin.moderation.dash.age.reports':
    'Child-safety reports in the last {n, plural, one {day} other {# days}}',
  'admin.moderation.dash.age.activeBans': 'Active child-safety bans',

  // ---- Admin > Bans filters (MOD-BAN-003) ----
  'admin.bans.filters': 'Filter bans',
  // ---- ban figures above the list (MOD-DASH-007, D-340) ----
  'admin.bans.figures.label': 'Ban figures',
  'admin.bans.figures.summary': '{active} bans in force; {lifted} lifted in this window.',
  'admin.bans.figures.byReason': 'In force by reason',
  'admin.bans.figures.bySeverity': 'In force by severity',
  'admin.bans.figures.byAge': 'In force by age',
  'admin.bans.figures.chipAria': 'Show {label} bans ({count})',
  'admin.bans.severityFilter': 'Severity',
  'admin.bans.severityAny': 'Any severity',
  'admin.bans.severitySerious': 'Serious',
  'admin.bans.severityStandard': 'Standard',
  'admin.bans.reasonFilter': 'Reason',
  'admin.bans.reasonAny': 'Any reason',
  'admin.bans.reasonNone': 'No reason recorded',
  'admin.bans.ageFilter': 'Age of the ban',
  'admin.bans.ageAny': 'Any age',
  'admin.bans.ageUnder7': 'Under 7 days',
  'admin.bans.age7to30': '7 to 30 days',
  'admin.bans.age30to90': '30 to 90 days',
  'admin.bans.ageOver90': 'Over 90 days',
} satisfies Record<string, string>;
