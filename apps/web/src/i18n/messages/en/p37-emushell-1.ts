// ACT-EMU-001 (wave 37): the emulator page (/games), the controls mapping screen, the PlayStation BIOS panel, and the
// sixth storage grant in the role editor.
export default {
  // ---- the role editor's sixth toggle (the other five are in p27-byos-ui-1)
  'settings.storageGrants.grant.viewGames': 'View shelf games (ROMs under emu/roms)',

  // ---- the page
  'games.page.title': 'Games',
  'games.page.intro':
    'Play retro games in your browser. ROMs come from the emu/roms folder of your own bucket and of the servers you are in; the game runs on your device and your saves go to your own bucket, never to this instance.',
  'games.page.legal':
    'Only play ROMs you own or that are free to share (homebrew). This instance does not provide any.',
  'games.page.loadFailed': 'The game list could not be loaded',
  'games.page.cores': 'Emulator cores (separate files, source linked by their projects):',

  'games.list.title': 'Your games',
  'games.list.filter': 'Search games',
  'games.list.loading': 'Looking for games...',
  'games.list.emptyTitle': 'No games yet',
  'games.list.empty':
    'Put ROM files in an emu/roms folder of your bucket (or ask a server owner to share theirs with the View shelf games grant). They appear here after the next index run, or at once when they are in your own bucket.',
  'games.list.play': 'Play',
  'games.list.platformUnknown': 'Platform decided when it starts',
  'games.list.fromYou': 'your bucket',
  'games.list.fromServer': 'server shelf {server}',

  // ---- the player
  'games.player.label': 'Game player',
  'games.player.canvas': 'Game screen',
  'games.player.pause': 'Pause',
  'games.player.resume': 'Resume',
  'games.player.restart': 'Restart',
  'games.player.controls': 'Controls',
  'games.player.fullscreen': 'Full screen',
  'games.player.close': 'Close game',
  'games.loading.rom': 'Reading the game from your bucket... {percent}%',
  'games.loading.download': 'Fetching the emulator core...',
  'games.loading.unpack': 'Unpacking the emulator core... {percent}%',
  'games.loading.start': 'Starting...',

  'games.saves.save': 'Save state {slot}',
  'games.saves.load': 'Load state {slot}',
  'games.saves.noMount':
    'Saving is off: connect a bucket of your own (with its CORS check passed) in Settings, Storage. Nothing is saved on this instance.',
  'games.saves.ready': 'Saves go to your bucket "{mount}" under emu/saves.',
  'games.saves.saving': 'Saving...',
  'games.saves.saved': 'Saved to your bucket.',
  'games.saves.failed': 'The save did not go through. Check that your bucket is online.',

  // ---- start-up problems
  'games.error.coreMissingTitle': 'This emulator core is not installed here',
  'games.error.coreMissing':
    'The {core} core is not on this instance. Ask the operator to install the emulator cores (deploy/EMULATOR.md).',
  'games.error.webgl2': 'This core needs WebGL 2, which this browser does not offer',
  'games.error.platform':
    'Could not tell which console this file is for. Rename it with its usual extension (.nes, .gba, .sfc, ...).',
  'games.error.biosTitle': 'A PlayStation BIOS is needed',
  'games.error.bios':
    'Choose a BIOS file from your own bucket in the BIOS panel below. This instance cannot supply one.',
  'games.error.rom': 'The game file could not be read from the bucket',
  'games.error.failed': 'The game could not start',

  // ---- the mapping screen
  'games.mapping.title': 'Controls',
  'games.mapping.description':
    'The keyboard always works. A connected gamepad drives player 1 together with the keyboard, further pads drive players 2 to 4.',
  'games.mapping.mapKeyboard': 'Set keyboard keys',
  'games.mapping.mapGamepad': 'Set gamepad buttons',
  'games.mapping.pressKey': 'Press the key for {button}.',
  'games.mapping.pressButton': 'Press the gamepad button for {button}.',
  'games.mapping.skip': 'Skip',
  'games.mapping.reset': 'Reset to defaults',
  'games.mapping.cancel': 'Cancel',
  'games.mapping.save': 'Save controls',
  'games.mapping.colButton': 'Control',
  'games.mapping.colKeyboard': 'Keyboard',
  'games.mapping.colGamepad': 'Gamepad',
  'games.button.b': 'B',
  'games.button.y': 'Y',
  'games.button.select': 'Select',
  'games.button.start': 'Start',
  'games.button.up': 'Up',
  'games.button.down': 'Down',
  'games.button.left': 'Left',
  'games.button.right': 'Right',
  'games.button.a': 'A',
  'games.button.x': 'X',
  'games.button.l': 'L',
  'games.button.r': 'R',
  'games.button.l2': 'L2',
  'games.button.r2': 'R2',
  'games.button.l3': 'L3 (stick press)',
  'games.button.r3': 'R3 (stick press)',

  // ---- the PlayStation BIOS
  'games.bios.title': 'PlayStation BIOS',
  'games.bios.why':
    "PlayStation games need the console BIOS, which is Sony's property. This instance never provides, stores or keeps one: you supply your own, it is read straight from your bucket into this browser tab, checked against the list of known BIOS dumps, and forgotten when you close the game.",
  'games.bios.where':
    "Put your BIOS file (512 KiB) in the emu/bios folder of your own bucket. Files from a server shelf or anyone else's bucket are not accepted.",
  'games.bios.none': 'No 512 KiB file found in emu/bios of your buckets.',
  'games.bios.use': 'Use this BIOS',
  'games.bios.accepted': 'BIOS accepted: {file}',
  'games.bios.memoryOnly': 'It stays in memory for this visit only.',
  'games.bios.forget': 'Forget it now',
  'games.bios.readFailed': 'The BIOS file could not be read from your bucket',
  'games.bios.refused': 'This file is not a known PlayStation BIOS',
  'games.bios.refusedSize': 'A PlayStation BIOS is exactly 512 KiB.',
  'games.bios.refusedUnknown':
    'It does not match any of the {count} known dumps, so it was not used. Its SHA-1 is {sha1}.',
} satisfies Record<string, string>;
