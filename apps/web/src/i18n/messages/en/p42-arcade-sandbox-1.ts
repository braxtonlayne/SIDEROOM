export default {
  // Hardened Activity sandbox: the host asks before it hands over a file (ARC-SBX-001)
  'activities.confirm.saveTitle': '{name} wants to save a file',
  'activities.confirm.saveBody': 'Save {file} ({size, number} bytes) to your device?',
  'activities.confirm.save': 'Save file',

  // Build trust badges and vouches (ARC-SBX-001)
  'activities.trust.signed': 'SWARM-SAFETY-SIGNED',
  'activities.trust.featured': 'OPERATOR-FEATURED',
  'activities.trust.unsigned': 'Not signed',
  'activities.trust.unverifiedUpdate':
    '⚠️ UNVERIFIED UPDATE ({n, plural, one {# user vouched} other {# users vouched}} for previous build {previous}; {current, plural, =0 {0 vouches} one {# vouch} other {# vouches}} on current hash)',
  'activities.trust.signedHint':
    'The code of this exact build (hash {hash}) passed an automated audit and was signed by the operator. Its art, audio and other assets were not inspected.',
  'activities.trust.featuredHint': 'The operator chose this game for the front page.',
  'activities.trust.unsignedHint':
    'This build of the code has not been signed. Run it only if you trust whoever shared it.',
  'activities.trust.vouchCount':
    '{n, plural, =0 {No vouches for this build} one {# member vouched for this build} other {# members vouched for this build}}',
  'activities.trust.vouch': 'Vouch for Build',
  'activities.trust.unvouch': 'Withdraw vouch',
  'activities.trust.vouchHint':
    'Your vouch is tied to this exact build. If the developer ships new code, it no longer counts.',
  'activities.trust.vouchFailed': 'Your vouch could not be saved.',
  'activities.trust.loadFailed': 'The trust badge could not be loaded.',
  'activities.trust.notEligible': 'Only members in good standing can vouch for a build.',
} satisfies Record<string, string>;
