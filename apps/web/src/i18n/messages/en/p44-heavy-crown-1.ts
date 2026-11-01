export default {
  // Profile vouches, tiers and the accountability multiplier (MOD-VCH-001)
  'profile.vouch.region': 'Vouches',
  'profile.vouch.count':
    '{n, plural, =0 {No vouches yet} one {# member vouches for this person} other {# members vouch for this person}}',
  'profile.vouch.tier.citizen': 'Citizen',
  'profile.vouch.tier.pillar': 'Pillar',
  'profile.vouch.tier.elder': 'Elder',
  'profile.vouch.badge': 'Vouch tier: {tier}',
  'profile.vouch.tooltipLead':
    'Vouches show how many members trust this person. More trust means more accountability.',
  'profile.vouch.tooltipTiers':
    'Citizen: under {pillar, number} vouches. Pillar: {pillar, number} to {elderBelow, number}. Elder: {elder, number} or more.',
  'profile.vouch.tooltipRule':
    'When a Pillar breaks a rule, automatic timeouts, holds and strike weights count {pillarX, number}x. For an Elder they count {elderX, number}x.',
  'profile.vouch.tooltipNow':
    'This person is a {tier} ({count, number}): automatic penalties count {mult, number}x.',
  'profile.vouch.tooltipNowPlain':
    'This person is a Citizen: automatic penalties count normally (1x).',
  'profile.vouch.tooltipFallout':
    'If a member you vouched for is banned for a serious reason, your vouch is removed and you get a private notice. Nothing happens to your account.',
  'profile.vouch.vouch': 'Vouch',
  'profile.vouch.revoke': 'Withdraw vouch',
  'profile.vouch.you': 'You vouch for this person.',
  'profile.vouch.cannot': 'You cannot vouch for this person right now.',
  'profile.vouch.failed': 'Your vouch could not be saved.',
  'profile.vouch.tooMany': 'You have vouched for a lot of members today. Try again tomorrow.',
  'profile.vouch.loadFailed': 'Vouches could not be loaded.',
} satisfies Record<string, string>;
