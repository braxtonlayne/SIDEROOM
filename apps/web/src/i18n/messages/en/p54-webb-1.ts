// SEC-AUD-038 (wave 54, D-564): the E2EE key-change notice in the call shield.
export default {
  'lounge.keyPins.shieldChanged': 'End-to-end encryption: a privacy code changed',
  'lounge.keyPins.changed': 'The privacy code of {name} changed since you last saw it.',
  'lounge.keyPins.compare':
    'A new device can cause this. Compare the new code with them, then mark them verified again.',
  'lounge.keyPins.compareHidden':
    'A new device can cause this. Turn on “Show privacy codes” in your settings to compare the new code.',
} satisfies Record<string, string>;
