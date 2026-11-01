// ARC-GST-002 (wave 43, D-385, D-397). The guest runner page (`/play/<token>`). Registered by the guest-play public
// surface alone (src/public/guest-play/messages.ts), never by the shared first-paint catalog: keep it small, every
// byte is sent to people without a login. The Share link dialog's strings are in p43-guest-runner-1.
export default {
  'guestPlay.page.title': 'Join the game',
  'guestPlay.page.intro':
    'You were invited to play in a browser. You only see the game: no chat, no members, no call.',
  'guestPlay.join': 'Join the game',
  'guestPlay.joining': 'Joining…',
  'guestPlay.playing.as': 'Playing as {name}',
  'guestPlay.playing.players': '{n, plural, one {# player} other {# players}}',
  'guestPlay.frame.title': 'Game: {name}',
  'guestPlay.error.invalid': 'This link is not valid. Ask for a new one.',
  'guestPlay.error.nokey':
    'This link is incomplete: the part after the # is missing. Ask for the link again and copy all of it.',
  'guestPlay.error.gone': 'This link has expired or the host closed it. Ask for a new one.',
  'guestPlay.error.rate': 'Too many tries. Wait a minute and open the link again.',
  'guestPlay.error.network': 'The game could not be reached. Check your connection and try again.',
  'guestPlay.error.refused': 'The game would not let you in. Ask the host for a new link.',
  'guestPlay.refused.voice':
    'This game uses in-game voice, which guest links do not support yet. Ask the host to play without it.',
  'guestPlay.refused.notGuest': 'This game is not open to guests.',
  'guestPlay.ended.title': 'The game connection ended',
  'guestPlay.ended.body': 'The host may have closed the link or the lobby. You can try joining again.',
  'guestPlay.retry': 'Try again',
};
