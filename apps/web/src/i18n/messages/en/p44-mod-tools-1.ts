// MOD-GRD-001 (wave 44-A): graduated moderation. Staff: Admin > Moderation > Subtree cases. Members: the
// restrictions on their own account with an appeal, and the monthly transparency counts (Settings > Privacy).
export default {
  'settings.privacy.review.title': 'Your account review',
  'settings.privacy.review.description':
    'Restrictions and warnings that are on your account right now. You can appeal each one once.',
  'settings.privacy.review.listLabel': 'Restrictions on your account',
  'settings.privacy.review.until': 'Until {when}',
  'settings.privacy.review.appeal': 'Appeal',
  'settings.privacy.review.appealLabel': 'Why this should be lifted',
  'settings.privacy.review.send': 'Send appeal',
  'settings.privacy.review.sent': 'Your appeal was sent.',
  'settings.privacy.review.appealOpen': 'Appeal sent, waiting for a decision',
  'settings.privacy.review.appealUpheld': 'Appeal decided: the decision stands',
  'settings.privacy.review.appealOverturned': 'Appeal decided: lifted',
  'settings.privacy.transparency.title': 'Moderation transparency',
  'settings.privacy.transparency.description':
    'Month by month, how many accounts moderators acted on after serious bans. Counts only; nothing here names a person or shows how accounts were connected.',
  'settings.privacy.transparency.empty': 'Nothing to report yet.',
  'settings.privacy.transparency.listLabel': 'Moderation counts by month',
  'settings.privacy.transparency.month':
    '{serious, plural, one {# serious ban} other {# serious bans}}, {branch, plural, one {# branch action} other {# branch actions}}, {actioned} accounts actioned, {frozen} frozen pending review, {sterilizations} invite stops, {appeals} appeals ({overturned} lifted)',
} satisfies Record<string, string>;
