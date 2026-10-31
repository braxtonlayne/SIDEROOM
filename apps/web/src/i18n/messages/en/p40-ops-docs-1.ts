// OPS-DOC-001 (wave 40): the member handbook's pages for the feature phase and the operator page for the module
// switches. Keys live under help.handbook.<section>.* and help.operators.*.
export default {
  'help.handbook.storage.title': 'Your own storage',
  'help.handbook.storage.body1':
    'Under Settings > Storage you can connect a bucket of your own (any S3-compatible storage). Your movies, shows, books and music stay in your bucket: this server only hands out short-lived links to them and never keeps a copy. The connect screen tests the bucket and sets it up for the browser. Give the key access to that one bucket only.',
  'help.handbook.storage.body2':
    'From the same screen you can share your bucket with a friend (a Friend Shelf: they see only the kinds you choose). A server owner can connect a bucket for the whole server and decide which roles may view or upload. If you remove a bucket, nothing is deleted from it: the files stay yours. Videos your browser cannot play directly are converted in your browser first; very large ones may need a worker, which whoever runs this server can set up.',
  'help.handbook.storage.link': 'Open Storage',

  'help.handbook.player.title': 'The player and the jukebox',
  'help.handbook.player.body1':
    'Music and videos you start keep playing as you move around: the floating player stays in view, with an equaliser, gapless playback and several looks (skins) you pick under Settings > Appearance. Your music library (tags and covers read from your own files), playlists and favorites are in the player’s Library tab.',
  'help.handbook.player.body2':
    'In a voice channel the jukebox lets people queue tracks for everyone and vote to skip, and one person can share a track into the call (it is encrypted like the rest of the call). A listening room keeps playing around the clock. This server stores no music: play your own music, with your friends.',
  'help.handbook.player.link': 'Open Appearance',

  'help.handbook.drawer.title': 'The utility drawer',
  'help.handbook.drawer.body1':
    'Press Ctrl+K (Cmd+K on a Mac) to open the drawer. It goes to places, runs commands and opens tools: calculators and unit conversion, a currency converter (European Central Bank rates, updated daily), dice and timers, a private notes pad that only you can read, and image and audio tools that work on your device. Tools can be pinned to the side rail or opened as floating gadgets.',
  'help.handbook.drawer.body2':
    'Your notes are locked with a passphrase that only you hold: if you lose it, the notes cannot be recovered. Shared boards (a tier list or a poll) and event cards can be posted in a channel; a shared board lasts seven days.',
  'help.handbook.drawer.link': 'Open the Lounge',

  'help.handbook.profiles.title': 'Profiles, rings and the scrapbook',
  'help.handbook.profiles.body1':
    'Your profile page is at /~yourname and starts private. Under Settings > Profile you choose who sees the page and each part of it, from only you up to anyone on the web (only if the people running this server allow public profiles). The “view as” strip shows what each kind of viewer sees. Visitors without an account can leave a guestbook note only on a public page, and it shows nothing until you approve it.',
  'help.handbook.profiles.body2':
    'Where this server has them on, a status ring puts a photo, a short clip, a line of text or a track on your avatar for exactly 24 hours, for your mutual friends or the servers you choose. Only you see who opened it, and it can be reported. A server can also keep a scrapbook of messages its members chose to save, with copies of the files in the server’s own bucket.',
  'help.handbook.profiles.link': 'Open Profile',

  'help.handbook.bridge.title': 'The Discord bridge',
  'help.handbook.bridge.body1':
    'A server owner can connect a server, or some of its channels, to a Discord server. It is off for every server until the owner switches it on under Server Settings > Modules > Discord bridge: the owner makes a bot in Discord’s developer portal, pastes its token there, and chooses a mode. Only the owner can switch it on or off.',
  'help.handbook.bridge.body2':
    'Only bridge these: the channels the owner maps are mirrored in both directions, with names, avatars and messages. Bridge the whole server: roles, categories, channels and threads are created and kept in step on both sides, permissions are set here and never on Discord, and the owner reviews a plan before anything is written. Direct messages, voice and encrypted content are never mirrored.',
  'help.handbook.bridge.body3':
    'The owner switches it off on the same card. The webhooks it made on Discord and its records here are deleted, and messages already mirrored stay on both sides. In whole-server mode the owner can also choose to remove what the bridge created on Discord and to put Discord’s permissions back. The privacy section lists which of your channels are mirrored.',
  'help.handbook.bridge.link': 'Open the Lounge',

  'help.handbook.activities.title': 'Activities',
  'help.handbook.activities.body1':
    'Start an Activity from the Activities button in a voice channel. The built-in ones share what you do over the call’s encrypted channel: a whiteboard, a card table, a tabletop with dice and tokens, and Land.io, a small arena game. Nothing is stored, and when the person who started one leaves, it ends.',
  'help.handbook.activities.body2':
    'The card table plays Texas Hold’em with play money only: a friends’ table, not a casino, and it cannot stop a determined cheater.',
  'help.handbook.activities.link': 'Open the Lounge',

  'help.handbook.emulator.title': 'Retro games in your browser',
  'help.handbook.emulator.body1':
    'The Games page plays NES, Game Boy, Game Boy Color, Game Boy Advance and PlayStation games in your browser. This server ships no games: you bring your own ROMs, in the emu/roms folder of your own bucket (or of a server that shares one with a games role). Only play games you own or that are free to share, such as homebrew.',
  'help.handbook.emulator.body2':
    'PlayStation games also need the BIOS from your own console, in emu/bios of your bucket; it is checked, used for that visit only and never stored. Save states and in-game saves go to your own bucket. In a voice call the person running a game can let others join as players: they see the picture and send their buttons, and the game files never leave the host. SNES and Genesis say “core not installed” for now.',
  'help.handbook.emulator.link': 'Open Games',

  'help.handbook.operators.title': 'If you run this server',
  'help.handbook.operators.body':
    'The people who run an instance can switch whole modules on or off. The operator page lists each switch and what it does.',
  'help.handbook.operators.link': 'Open the operator page',

  'help.operators.title': 'Module switches',
  'help.operators.intro':
    'For the people who run this instance (Admin > Features). Every module of SIDE ROOM has a switch for the whole instance, and some also have one for each server under Server Settings > Modules. A module that is switched off stops answering: the buttons disappear and its requests are refused. Nothing can switch off call encryption, authorization or the audit and access logs.',
  'help.operators.listTitle': 'The switches',
  'help.operators.byos':
    'Your own storage: connecting buckets, Friend Shelves, server shelves and the shelves they feed.',
  'help.operators.jukebox': 'The voice jukebox: the shared queue and vote-skip in a voice channel.',
  'help.operators.rooms': 'Listening rooms: voice channels that play a playlist around the clock.',
  'help.operators.tools':
    'The utility drawer. Each of its tools also has its own switch under it, except the guide.',
  'help.operators.datachannel':
    'The Activities data channel: lets built-in Activities share state over the call.',
  'help.operators.emulator':
    'The emulator: the Games page and the core files. Off, the cores are not served.',
  'help.operators.embeds':
    'Watch Together embeds: parties with a YouTube, Vimeo, PeerTube or direct link.',
  'help.operators.bridge':
    'The Discord bridge. Off on every server until its owner switches it on; switching the instance switch off tears every bridge down.',
  'help.operators.scrapbook': 'The scrapbook vault, offered to servers that choose a shelf for it.',
  'help.operators.rings': 'Status rings. Off until you switch them on.',
  'help.operators.profiles':
    'Public profiles. A public page needs this switch and the setting profiles.access set to opt_in_public; members still choose what to show.',
  'help.operators.gameServers':
    'The Game Servers page. It only shows or hides the “coming soon” preview; no game server exists behind it.',
  'help.operators.moreTitle': 'Settings and jobs',
  'help.operators.more':
    'The numeric settings (bucket limits, link lifetimes, bridge figures), the environment variables and the background jobs of these modules are in the operator handbook, deploy/OPERATOR.md, which ships with the instance.',
  'help.operators.back': 'Back to Help',
} satisfies Record<string, string>;
