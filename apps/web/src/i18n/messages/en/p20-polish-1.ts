export default {
  // ---- Staff age buttons on the Library series page and the Cinema title page (MOD-AGE-001, phase 20) ----
  'moderation.ageButton.mark': 'Mark 18+',
  'moderation.ageButton.unmark': 'Remove 18+ mark',
  'moderation.ageButton.markTitle': 'Mark this item 18+',
  'moderation.ageButton.unmarkTitle': 'Remove the 18+ mark',
  'moderation.ageButton.markHelp':
    'Members must confirm they are 18 or older to open it, and members who hide 18+ items stop seeing it in search.',
  'moderation.ageButton.unmarkHelp':
    'Anyone who can reach the item may open it without the age check. Search shows it to everyone again.',
  'moderation.ageButton.reasonLabel': 'Reason (kept in the staff access log)',
  'moderation.ageButton.cancel': 'Cancel',
  'moderation.ageButton.markConfirm': 'Mark 18+',
  'moderation.ageButton.unmarkConfirm': 'Remove mark',
  'moderation.ageButton.errorDenied': 'You do not have the permission to change this mark.',
  'moderation.ageButton.errorGone': 'This item no longer exists.',
  'moderation.ageButton.errorGeneric': 'The change could not be saved. Try again.',

  // ---- Review hub: Reports tile (MOD-DASH-002, D-339) ----
  'admin.reviewHub.reportsTile':
    '{n, plural, one {# report is waiting for someone to pick it up} other {# reports are waiting for someone to pick them up}}.',
} satisfies Record<string, string>;
