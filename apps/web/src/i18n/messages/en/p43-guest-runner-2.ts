// ARC-GST-002 (wave 43, D-385, D-397). The guest runner page (`/play/<token>`). Registered by the guest-play public
// surface alone (src/public/guest-play/messages.ts), never by the shared first-paint catalog: keep it small, every
// byte is sent to people without a login. The Share link dialog's strings are in p43-guest-runner-1.
export default {
  'guestplay.page.title': 'Join the game',
  'guestplay.page.intro':
    'You were invited to play in a browser. You only see the game: no chat, no members, no call.',
  'guestplay.join': 'Join the game',
  'guestplay.joining': 'Joining…',
  'guestplay.playing.as': 'Playing as {name}',
  'guestplay.playing.players': '{n, plural, one {# player} other {# players}}',
  'guestplay.frame.title': 'Game: {name}',
  'guestplay.error.invalid': 'This link is not valid. Ask for a new one.',
  'guestplay.error.nokey':
    'This link is incomplete: the part after the # is missing. Ask for the link again and copy all of it.',
  'guestplay.error.gone': 'This link has expired or the host closed it. Ask for a new one.',
  'guestplay.error.rate': 'Too many tries. Wait a minute and open the link again.',
  'guestplay.error.network': 'The game could not be reached. Check your connection and try again.',
  'guestplay.error.refused': 'The game would not let you in. Ask the host for a new link.',
  'guestplay.refused.voice':
    'This game uses in-game voice, which guest links do not support yet. Ask the host to play without it.',
  'guestplay.refused.notGuest': 'This game is not open to guests.',
  'guestplay.ended.title': 'The game connection ended',
  'guestplay.ended.body':
    'The host may have closed the link or the lobby. You can try joining again.',
  'guestplay.retry': 'Try again',
};
