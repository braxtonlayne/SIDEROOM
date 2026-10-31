export default {
  // ---- ACT-EMU-002: co-op play by input forwarding ----
  'emu.coop.button': 'Play together',
  'emu.coop.dialog.title': 'Play together',
  'emu.coop.dialog.description':
    'One person runs the game and shares their screen. Everyone else watches that screen and plays on player 2, 3 or 4.',
  'emu.coop.dialog.close': 'Close',
  'emu.coop.noRollback':
    'There is no netplay rollback. Your buttons travel to the host and the picture travels back through the screen share, so fast games feel delayed. Slow and turn-based games work best.',
  'emu.coop.host.heading': 'Host a game',
  'emu.coop.host.noGame': 'Open a game from Games first, then come back here to host it.',
  'emu.coop.host.start': 'Host {title}',
  'emu.coop.host.hosting': 'You are hosting {title}.',
  'emu.coop.host.shareHint': 'Share your screen or the game tab so the others can see it.',
  'emu.coop.host.share': 'Share screen',
  'emu.coop.host.noPlayers': 'Nobody has taken a seat yet.',
  'emu.coop.host.seat': 'Player {n}: {name}',
  'emu.coop.host.kick': 'Remove {name}',
  'emu.coop.host.stop': 'Stop hosting',
  'emu.coop.host.ping': 'Your connection to the call: {ms} ms.',
  'emu.coop.guest.heading': 'Games in this call',
  'emu.coop.guest.none': 'Nobody is hosting a game in this call.',
  'emu.coop.guest.hostLine':
    '{name} is hosting {title}. {free, plural, =0 {No seat free} one {# seat free} other {# seats free}}.',
  'emu.coop.guest.join': 'Join as a player',
  'emu.coop.guest.joining': 'Asking for a seat.',
  'emu.coop.guest.full': 'That game has no free seat.',
  'emu.coop.guest.closed': 'The host ended the game.',
  'emu.coop.guest.kicked': 'The host removed you from the game.',
  'emu.coop.guest.keys':
    'Controls: arrows move, Z is B, X is A, A is Y, S is X, Q and W are L and R, Enter is Start, Backspace is Select. A gamepad works too.',
  'emu.coop.status.region': 'Co-op game',
  'emu.coop.status.player': 'Player {n} in {title}',
  'emu.coop.status.hosting':
    'Hosting {title}: {count, plural, one {# player} other {# players}} joined',
  'emu.coop.status.latency': 'Input round trip {ms} ms, plus the video delay',
  'emu.coop.status.measuring': 'Measuring the round trip',
  'emu.coop.status.controlsOn': 'Controls on',
  'emu.coop.status.controlsOff': 'Controls off',
  'emu.coop.status.leave': 'Leave the game',
  'emu.coop.status.stop': 'Stop hosting',
} satisfies Record<string, string>;
