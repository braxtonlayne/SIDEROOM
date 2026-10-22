// MOD-AGE-001 to -003 and MOD-DSR-005 to -007: age tools on the case page, the hold decision after a deletion
// request, and the restriction and rectification actions of the data request queue.
export default {
  'admin.moderation.age.button': 'Age restriction',
  'admin.moderation.age.title': 'Age restriction and underage handling',
  'admin.moderation.age.description':
    'Mark or unmark what this case is about as age-restricted. The change applies on the next request and is logged.',
  'admin.moderation.age.descriptionUnder18':
    "Reset this account's age confirmation and lock it, so the age gate asks again and the account cannot answer 18+ itself until you lift the lock. The account stays active and is told.",
  'admin.moderation.age.descriptionUnder13':
    "Reset and lock this account's age confirmation and suspend the account (needs the account permission). Its inviter is told only that the invitee did not meet the entry rules.",
  'admin.moderation.age.confirm': 'Apply',
  'admin.moderation.age.reason': 'Reason (kept in the staff log)',
  'admin.moderation.age.kind': 'What to do',
  'admin.moderation.age.done': 'Done.',
  'admin.moderation.age.restrictChannel': 'Mark the channel age-restricted',
  'admin.moderation.age.unrestrictChannel': 'Remove the channel age restriction',
  'admin.moderation.age.restrictSeries': 'Mark the series age-restricted',
  'admin.moderation.age.unrestrictSeries': 'Remove the series age restriction',
  'admin.moderation.age.restrictTitle': 'Mark the title age-restricted',
  'admin.moderation.age.unrestrictTitle': 'Remove the title age restriction',
  'admin.moderation.age.under18': 'Account is under 18: reset and lock the age confirmation',
  'admin.moderation.age.under13': 'Account is below the minimum age: reset, lock and suspend',
  'admin.moderation.event.ageRestrictionSet':
    '{restricted, select, true {Marked {label} age-restricted: {reason}} other {Removed the age restriction from {label}: {reason}}}',
  'admin.moderation.event.underageHandled':
    'Underage handling ({band, select, under_13 {below the minimum age, suspended} other {under 18, age confirmation reset and locked}}): {note}',
  'admin.moderation.event.erasureHoldFlagged':
    'A deletion request met a hold on this case; the held rows stay. Decide: release the hold, or retain with a legal basis.',
  'admin.moderation.event.erasureHoldRetained':
    'Held rows retained after a deletion request. Basis: {basis}',
  'admin.moderation.hold.erasureFlagged': 'Deletion request',
  'admin.moderation.hold.erasureNote':
    'A deletion request (data request {id}) met this hold. The held rows stay until staff decide: release, or retain with a stated legal basis.',
  'admin.moderation.hold.erasureRetained': 'Retained after a deletion request. Basis: {basis}',
  'admin.moderation.hold.retain': 'Retain',
  'admin.moderation.hold.retainAria': 'Retain hold {id} with a legal basis',
  'admin.moderation.dialog.retainTitle': 'Retain the held rows',
  'admin.moderation.dialog.retainDescription':
    'A deletion request met this hold. The rows stay held and are not erased. State the legal basis for keeping them; it is kept on the hold and in the staff log.',
  'admin.moderation.dialog.retainConfirm': 'Retain',
  'admin.moderation.dialog.retainBasis': 'Legal basis',
  'admin.moderation.dialog.retainedNotice': 'The hold was marked as retained.',
  'admin.dataRequests.hiddenBadge': 'Activity hidden',
  'admin.dataRequests.hiddenNote':
    'Shared activity hidden from discovery since {when}. Reason: {reason}',
  'admin.dataRequests.hideActivity': 'Hide activity',
  'admin.dataRequests.showActivity': 'Show activity',
  'admin.dataRequests.hideActivityAria': 'Hide the shared activity of {name} from discovery',
  'admin.dataRequests.showActivityAria': 'Show the shared activity of {name} again',
  'admin.dataRequests.hideActivityTitle': 'Hide activity from discovery',
  'admin.dataRequests.showActivityTitle': 'Show activity again',
  'admin.dataRequests.hideActivityDescription':
    "While the request is reviewed, the account's shared cinema and library activity, watching presence and recommendations are hidden from other members. Nothing is deleted. It ends when the request closes.",
  'admin.dataRequests.showActivityDescription':
    "The account's shared activity becomes visible again, as its own privacy settings allow.",
  'admin.dataRequests.restrictReason': 'Reason (kept in the staff log)',
  'admin.dataRequests.activityHidden': 'The activity is hidden from discovery.',
  'admin.dataRequests.activityShown': 'The activity is visible again.',
  'admin.dataRequests.rectify': 'Correct Person link',
  'admin.dataRequests.rectifyAria': 'Correct the Person link of {name}',
  'admin.dataRequests.rectifyTitle': 'Correct a Person link',
  'admin.dataRequests.rectifyDescription':
    "Put the requester's account under the Person it belongs to, or split it off into a Person of its own. Needs the account permission as well. The reason is kept in the staff log.",
  'admin.dataRequests.rectifyConfirm': 'Correct',
  'admin.dataRequests.rectifyReason': 'Why the link was wrong',
  'admin.dataRequests.rectifyPerson': 'Person id',
  'admin.dataRequests.rectifyPersonHint':
    'The Person to link the account to. Leave empty to split the account off into a new Person.',
  'admin.dataRequests.rectified': 'The Person link was corrected.',
} satisfies Record<string, string>;
