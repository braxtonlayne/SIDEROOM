// SEC-AUD-046 (D-577): list loads that failed are shown where the list is; a background sync never toasts.
// SEC-AUD-047 (D-578): the E2EE shield names a key new to this device's set as a new device or a new sign-in.
export default {
  'lounge.listLoad.channels': 'The channels could not be loaded.',
  'lounge.listLoad.members': 'The member list could not be loaded.',
  'lounge.listLoad.server': 'This server could not be loaded.',
  'lounge.listLoad.retry': 'Try again',
  'lounge.soundboard.favoritesUnsynced': 'Favourites are not saved to your account yet.',
  'lounge.keyPins.shieldNew': 'End-to-end encryption: a new privacy code',
  'lounge.keyPins.newCode': '{name} has a new privacy code (a new device, or a new sign-in).',
  'lounge.keyPins.compare': 'Compare the new code with them, then mark them verified.',
  'lounge.keyPins.compareHidden':
    'Turn on “Show privacy codes” in your settings to compare the new code.',
} satisfies Record<string, string>;
