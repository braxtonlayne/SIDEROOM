// VOX-E2E-007: privacy codes (E2EE shield popover, D-250); VOX-E2E-003: join errors.
export default {
  'lounge.privacyCodes.title': 'End-to-end encryption',
  'lounge.privacyCodes.allVerified': 'End-to-end encrypted, everyone verified',
  'lounge.privacyCodes.intro':
    'This call is end-to-end encrypted. Compare the codes with the other people, in person or over another app. If they match, nobody else can listen in.',
  'lounge.privacyCodes.securing': 'Securing the call…',
  'lounge.privacyCodes.callCode': 'Call code',
  'lounge.privacyCodes.codeOf': 'Privacy code of {name}',
  'lounge.privacyCodes.verified': 'Verified',
  'lounge.privacyCodes.markVerified': 'Mark as verified',
  'voice.e2ee.unsupported':
    'Your browser cannot join end-to-end encrypted calls. Try a current version of Chrome, Edge, Firefox or Safari.',
  'voice.e2ee.failed':
    'End-to-end encryption could not start, so the call was not joined. Try again.',
} satisfies Record<string, string>;
