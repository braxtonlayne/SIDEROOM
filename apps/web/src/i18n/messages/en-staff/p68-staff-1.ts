// Wave 68 staff tools: the review queue names people (D-692, SEC-AUD-213); transcoding workers show their GPU and throughput (D-705, SEC-AUD-216).
export default {
  'admin.review.submitterTitle': '{name}. {hint}',
  'admin.review.heldByOtherNamed': '{name} is reviewing this until {when}.',
  'admin.review.takeOverDescriptionNamed':
    '{name} is still reviewing this. They will lose the claim and cannot decide any more.',
  'admin.transcoding.workers.noHardware': 'no hardware encoder',
  'admin.transcoding.workers.speedNow': '{speed}x now',
  'admin.transcoding.workers.last24h': '{done} done, {failed} failed in 24 h',
} as const;
