export default {
  // RDY-PAR-004: the DM spam filter looks at what a stranger sends, not only who they are.
  'settings.safety.filterHeuristics':
    'With Non-friends or Everyone on, a first message that looks like spam (a flood of repeated characters or words, or several different links) goes to Likely spam in Message Requests.',
  'lounge.requests.reasonSpamFilter': 'Filed as likely spam.',
  'lounge.messageRequests.spamHeading': 'Likely spam ({count})',
  'lounge.messageRequests.spamHelp':
    'These messages were flagged by your spam filter or filed by you. They stay here until you accept or ignore them.',
  'lounge.messageRequests.spamList': 'Likely spam',
} satisfies Record<string, string>;
