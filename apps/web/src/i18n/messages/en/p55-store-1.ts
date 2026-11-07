// SEC-AUD-049 (wave 55, D-572): a deleted comment that has replies stays as a tombstone so the replies keep
// their place.
export default {
  'library.comments.deleted': 'Comment deleted',
  'cinema.social.commentDeleted': 'Comment deleted',
  'cinema.social.showReplies': '{n, plural, one {Show # reply} other {Show # replies}}',
  'cinema.social.hideReplies': 'Hide replies',
} satisfies Record<string, string>;
