# Changelog

What changed in SIDE ROOM, one section per wave, newest first. The app shows this file at `/help/whats-new`, and the
first `##` heading below is the version the build embeds: when it changes, members see a one-time badge on the help
button. The integrator adds the new wave's section at the top from that wave's handoffs
(`docs/dev/handoff/feat-p<wave>-integration.md`); see the integrator's checklist in `AGENTS.md`.

Write for people who use the place, not for people who build it: what they can now do, what behaves differently, and
what the operator now has to know. Keep each wave to a handful of lines.

## Phase 36 - 2026-10-31

- **Activities can share what you do, privately.** An Activity started from a voice call can now keep a shared state
  and pass moves between the people in it. It all travels through the call, sealed with the call's own keys, so the
  server only relays bytes it cannot read. Each person's share is capped (`voice.datachannel_kbps`, 256 by default).
- **Four built-in Activities.** **Whiteboard**: pens, shapes, text, pictures, pages, undo and PNG export, drawn
  together. **Card table: Texas Hold'em** with play-money chips, blinds, a rotating dealer, side pots and showdown;
  your hole cards go to you alone, a 60-second turn timer keeps play moving, and the table says plainly that it is a
  friends' table, not cheat-proof. **Tabletop**: a grid or hex map with tokens, fog of war, initiative, measuring and
  a dice tray whose rolls show their seed so anyone can check them. **Land.io**, the territory game, with bots and
  every mode and map, hosted in the browser of whoever starts it.
- **The starter hosts the game.** Whoever starts one of these Activities runs it; when they leave it or the call, it
  ends for everyone.
- **For operators.** The four Activities are installed like any Activity: build the zips with
  `node scripts/build-activities.mjs`, upload them in the developer portal and approve them in the review queue.
  Bundles may now declare the `datachannel` capability in `sideroom.json` (migration 1760); bots get the same channel
  through `voice::DataChannel` in sr-sdk.
## Phase 35b - 2026-10-31

- **Bridge the whole server.** A server owner with the Discord bridge on can now choose between "Only bridge
  these" (pick channels, categories and threads) and "Bridge the whole server", which makes this server and the
  Discord server one: roles, categories, channels and active threads are created and kept in step on both sides, and
  every channel's messages flow both ways. Direct messages, encrypted conversations and voice are never mirrored.
- **Nothing changes until the owner says so.** A manager can prepare a plan: a checklist of what the bot needs on
  Discord (Administrator, the top role, the two intents), then a dry run listing what will be matched, created on
  each side, changed on Discord, imported from Discord (dangerous permissions in red) and left out. Only the owner
  confirms it, with the second-factor step.
- **Permissions are set here.** Role permissions, `@everyone` and channel permissions go from SIDE ROOM to Discord;
  a change made to them on Discord is undone and shows up in a new conflict log with who did it, where a person
  with the right to make that edit here can apply it with one click. SIDE ROOM-only permissions never leave; role
  order is set here. A channel deleted on Discord is hidden here for a day and can be restored.
- **Discord members fit in.** In whole-server mode they sit under the same hoisted roles in the member list, in the
  role colour, and role and channel mentions work both ways.
- **Switching off** still leaves Discord as it is by default; the owner can also remove what the bridge created there
  or put Discord's permissions back. The bot never deletes anything on Discord it did not create.
- **Operators:** three new settings, `bridge.delete_hold_hours` (24), `bridge.reconcile_minutes` (15) and
  `bridge.struct_writes_per_sec` (2). A bridge pauses itself after 200 refused Discord calls in ten minutes, every
  bridge after 2,000.

## Phase 35 - 2026-10-30

- **Discord bridge (off until a server owner switches it on).** Under Server Settings > Modules ("Integrations,
  more") the server owner can switch on the Discord bridge, connect their own Discord bot and map channels. Messages,
  edits, deletes, replies and reactions flow both ways: posts here appear on Discord with the author's name and
  avatar, and Discord messages appear here with a DISCORD tag. Attachments travel as links (Discord's links read
  "(link expired)" after a day). Encrypted channels are never bridged.
- **Discord people feel native.** Members of the mapped Discord server show in an "On Discord" group of the member
  list with their role colour (and online status when the bot is allowed to see it), have a profile pop-out, show up
  in the typing line, and can be @mentioned from here; they are not accounts and can never sign in, DM or moderate.
- **Everyone is told.** Every mirrored channel carries a banner, a system message marks when a channel is mapped or
  unmapped and when the bridge is switched off, and the privacy page lists the mirrored channels. There is no
  per-member opt-out: a bridged channel is mirrored. Data export and erasure cover your bridged messages.
- **Switching it off leaves nothing behind.** Switching the bridge off deletes the webhooks it created on Discord
  (and names any it could not), removes the mappings and the bot token, and says so in each channel.
- **Operators:** a kill switch for the bridge (Admin > Features, on by default), a Discord bridge tile on Admin >
  System (servers, mapped channels, queued and failed posts), and two settings: `abuse.bridge_messages_per_minute`
  (60) and `bridge.max_bridges` (10).

## Phase 34 - 2026-10-30

- **Profiles at `/~handle`.** New Links, Featured servers and Now playing blocks, and "Who can see my profile": pick a
  preset or set an audience per part (anyone on the web, members, people who share a server, friends, only me), and
  check it with the "view as" strip. **Profiles start private**: until you pick a preset, your page and its pop-out
  card are visible only to you. Public pages exist only if the operator turns on `platform.public_profiles` (off by
  default); they are never indexed by search engines and load only the code the page needs.
- **Public guestbook.** When you opt in, visitors without an account can leave a plain-text note; notes wait for your
  approval.
- **Play links here.** Links to YouTube, Vimeo (and PeerTube instances the operator names) get a "Play here" button.
  The privacy-enhanced player (`youtube-nocookie.com`, Vimeo with Do Not Track) loads only after you press it, or
  always if you choose "Load embeds" in Settings > Text & Images. Direct audio and video links play in the browser's
  own player with no download item.
- **Watch Together with a link.** Start a party from a pasted YouTube or Vimeo link or a media file: everyone presses
  Load, then play, pause and seek are shared. The server fetches nothing.
- **Status rings.** Share a photo, a short clip, a line of text or a track on your avatar for 24 hours with your
  mutual friends or the servers you choose (up to 20 in any 24 hours). Rings are deleted for good when the day is
  over, and only you can see who opened yours. A ring can be reported; a reported ring is kept for moderators until
  the case is decided.
- **Scrapbook.** A server can keep its best moments: when enough people react, or a permitted member sends it, a
  message and its files are copied into the server's own storage and shown in a gallery with filters. Deleting the
  message deletes the entry and its copies; a copy the bucket refuses to delete is shown to the server's managers
  and retried. Operators: the module (`lounge.scrapbook`) stays hidden until offered at platform scope.
- **Five new player skins.** Valve & Walnut, Tidepool Glass, Night Circuit, Pastel Study Deck and Steel Stack: painted
  artwork where only the real controls are live, with VU and LED meters, a spectrum, cue pads, an A-B loop, an
  equalizer and a playlist window. Motion follows the music and stops under reduced motion. The Study Deck's focus
  timer is the drawer's pomodoro and its notes pad writes to your encrypted notes.
- **Shuffle in the floating player.** One switch, shared by the library and every skin: it reorders what is still to
  come and restores the order when turned off. The player also shows a track's file format where a skin has a readout.
- **Your data.** "Request my data" now lists your profile privacy settings and held guestbook notes, your active
  rings (`rings.json`) and scrapbook entries about your messages (`scrapbook.json`); deleting your account deletes
  them.
- **Operators.** New platform feature `platform.public_profiles` (off), `lounge.rings` (on, the kill switch for
  rings) and `lounge.scrapbook` (hidden until offered). The app's security policy now allows framing
  `youtube-nocookie.com` and `player.vimeo.com` and https media; public and no-script pages are unchanged. Set the
  same `SR_PEERTUBE_HOSTS` for `sr-server` and the web build to offer PeerTube.

## Phase 32 - 2026-10-30

- **The utility drawer.** Ctrl+K (Command+K on a Mac) now opens a drawer: its first tab, Go to, is the quick
  switcher you know and works as before; type `>` for tools. The search finds a tool by name, by a typo ("Did you
  mean ...?") or by what you want to do ("convert miles to km"), right in your browser. A tool can pop out into a
  floating window (up to six, draggable and resizable, remembered for your account) or be pinned to a side rail,
  and it stays open while you move around the app.
- **Calculators and time.** A calculator, a programmer calculator, unit and percentage converters, a currency
  converter and a time zone meeting finder that copies a `<t:...>` timestamp. Currency rates are the European
  Central Bank's daily reference rates, fetched by the server; your browser never calls a third party. Operators:
  `tools.currency_rates` switches the converter off, and an outbound allow list must include `www.ecb.europa.eu`.
- **Developer tools, dice and timers.** JSON, regex, Base64, URL, JWT (decode only), Markdown tables, counters,
  hashes and diff; dice in RPG notation, a coin, a wheel and loot tables for you or your server, plus `/roll` in
  chat. Pomodoro, stopwatch and countdown timers can be shared with your voice channel.
- **Private notes.** A scratchpad encrypted in your browser: the server keeps only ciphertext. Keep the recovery
  file and passphrase safe; nobody, not the operator either, can recover a lost key.
- **Media tools.** An image and meme studio (crop, resize, captions, redact and pixelate, palette, export), video
  to GIF and an audio trimmer and normalizer, all on your device. "Edit image" in the attachment tray, "Open in
  studio" on a copy from the media viewer, and voice messages can be trimmed before sending.
- **Together.** Tier lists and poll builders that several people edit live in a channel or DM, posted as a message
  or as an image; and invite cards for a Cinema title, a Live TV slot or a game night, with seats to claim, an
  "Add to calendar" file and optional reminders by direct message an hour and ten minutes before (they follow your
  direct message settings). A Cinema card offers a watch party when it starts.
- **Your data.** "Request my data" now includes `tools.json`: your notes (still encrypted), your own loot tables and
  the invite card seats you claimed.

## Phase 31 - 2026-10-30

- **A jukebox in every voice channel.** Anyone who can speak queues tracks from their own music library; the
  queue is shared, listeners vote to skip (half of them skips), and a member can take the decks as DJ. In synced
  mode everyone plays the same track at the same moment from a bucket they can read; a track from a bucket you
  cannot read shows as "not shared with you" and stays silent for you. Server managers can turn a voice channel
  into a 24/7 listening room on a server playlist. Operators: the skip share is the setting
  `jukebox.vote_skip_percent`.
- **Music in the call.** Members with Speak can play music into a voice channel in stereo at up to 320 kbit/s,
  end-to-end encrypted like speech; your microphone stays mono. The DJ of a jukebox in stream mode does this
  for the room, and bots can too. Listeners have their own music mute and volume, apart from the voices.
  Server managers can switch music off for a channel or set its bitrate (`sr_music_allowed`,
  `sr_music_bitrate_kbps` on the channel, default on and 256).
- **Listening rooms.** The server never streams the audio: it keeps the place in the playlist, and everyone who
  listens along plays the same track at the same moment from the server shelf. The channel card shows what is
  playing and the next three, and disappears as soon as a manager switches the room off. Operators: a new gauge
  `sr_voice_music_egress_bits_per_second` shows the music share of voice egress.

## Phase 30b - 2026-10-30

- **The front door shows nothing of the inside.** A visitor who is not signed in now gets only the page in front
  of them: the landing page, the sign-in and registration pages and the invite page each load their own small
  bundle, and nothing in it names or fetches any part of the app. The app itself is sent only to a signed-in
  member. The landing page names no section.
- **Help is for members.** The handbook and What's new are behind sign-in. The sign-in form has a short help
  panel of its own, and House Notes (terms, privacy, rules, credits) is a plain page anyone can read.
- **Signing in and out.** Signing in opens the app at `/app`, or the page you were sent to sign in for. Signing
  out clears the app from the browser's caches and storage; a session ended on another device does the same at
  the next page load.
- **A music button in the left rail.** A music-note button beside the Discover button opens the player on its
  Library tab, so you can start music from the library even when nothing is queued. It is part of the signed-in app
  only.
- **For the operator:** `sr-server` now serves the app to signed-in members from `[web] root` (`SR_WEB__ROOT`)
  and refuses to start without the web build's `app/csp.txt`. Take the new Caddyfile and compose file, unpack the
  web build before restarting the server, and follow `deploy/UPGRADE.md` ("the web app is served by the server").

## Phase 30 - 2026-10-29

- **A music player that follows you.** Music plays in a small floating player that keeps going while you move
  between the Lounge, Cinema, Library, Settings and Admin. Pick one of three looks in Settings > Appearance: a
  retro MP3 player (the default), a compact pill, or a full library manager. It has lock-screen controls, keys
  (Alt+Shift+P to play or pause, Alt+Shift+Left and Right to skip, Alt+Shift+M to mute) and it docks in any
  corner. It is part of the signed-in app only: it never loads on the landing page, the login form or other
  public pages, and it stops when you sign out.
- **Your music, as a library.** Audio in your buckets is listed by artist and album, and the app reads the real
  titles, artists and covers from the files in the background. Your own edits to a track's details are kept, even
  when you rescan the tags.
- **Playlists, favorites and more.** In the library manager: search, your playlists and a server's (people with
  the upload right edit those), favorites, recently played, a smart queue (by album, artist, genre or "more like
  this", shuffle without repeats), a 10-band equalizer with presets, and gapless playback or a crossfade of up to
  12 seconds.
- **For the operator:** new settings `music.max_playlists` (default 100) and `music.max_items` (default 5000, also
  the favorites cap). The bucket CORS rule now also allows the `Range` request header; buckets set up earlier
  keep working, and running `sr-cli storage ensure-cors` again (or Apply CORS on a member's bucket) adds it. The
  tag reader uses the `lofty` library; its notice is in `THIRD_PARTY_NOTICES.md`.

## Phase 29 - 2026-10-29

- **Convert to play, in your browser.** Videos a browser cannot play (MKV, AVI, MOV, TS) can be converted while
  you upload them, in the bucket browser and when you submit a film to Cinema. Most files are only repackaged,
  which takes a minute or two; files whose picture or sound the browser cannot play are re-encoded in the tab
  where the browser can do it. Text subtitles come out as `.srt` or `.ass` files beside the video. Nothing leaves
  your browser for this, the original stays where it was, and the shelf plays the converted copy.
- **Big files go to a worker.** The browser converts files up to 2 GiB by default. For bigger ones, or ones the
  browser cannot convert, the owner of a bucket can run the transcoder on their own machine with a worker token
  for that bucket, or the operator can offer theirs.
- **Live TV for a server.** A Live TV channel can belong to a server and air only that server's shelves; everyone
  who may watch the server's Cinema sees the same guide, with a "Server shelf" chip. The guide lists the
  instance's channels first, then each server's, then your own.
- **Tabs that hide themselves.** With Cinema or Library on Auto, the tab appears once you can reach something to
  watch or read, and updates as buckets are connected or shared. Empty shelves say how to fill them.
- **For the operator:** new settings `byos.client_convert_max_mib` (default 2048, the most the bundled converter
  handles reliably) and `byos.worker_for_members` (off: your transcoder workers do not convert members' files).
  `byos.require_client_remux` now hides "upload as they are" in the Convert step. Server channels are created by
  staff through the admin API for now, and a channel with an imported guide must be an instance channel. The
  in-browser converter is GPL software loaded as separate files only when someone converts; its notice is in
  `THIRD_PARTY_NOTICES.md`.

## Phase 28 - 2026-10-27

- **Fill your buckets from the app.** The bucket browser uploads files or a whole folder (drag and drop works), in
  16 MiB parts that resume after a reload. You can make folders, rename and move files and folders, and delete them.
  Files over 5 GiB have to be moved at your provider.
- **Shelves.** Films, series, anime, manga, comics and books found in your buckets now show on shelves in Cinema and
  Library: your own, friends' shelves and server shelves, each with a chip that says whose it is. Files the browser
  can play (MP4, WebM, MP3, FLAC, OGG) play directly; other files are greyed out with "Convert to play" until a
  later wave. Comic archives open page by page without being downloaded in full. You can correct the title, year,
  kind, episodes and artwork, and "Fetch details" looks a title up. Your edits are kept when the bucket is
  re-indexed.
- **Voice is always end-to-end encrypted.** There is no setting that turns it off. A client that does not take
  part in the encryption is refused.
- **Only the front page shows up in search engines.** Every other page, the API and the media links tell crawlers
  not to index them.
- **For the operator:** new settings under Admin > Limits > storage: `byos.endpoint_allow`, `byos.who_may_mount`,
  `byos.require_client_remux`, `byos.max_mounts_per_user`, `byos.max_mounts_per_guild` and
  `byos.upload_parts_per_hour`. There is a `platform.byos` feature switch, plus two server choices: whether a
  server may connect buckets, and whether members' shelves show inside it. The `voice.e2ee` config field is gone,
  and `e2ee = false` is now a configuration error. Admin > Storage lists every bucket and can delist or relist
  one. Takedown notices can name an object in a member's bucket: an object in a server's bucket counts against
  the member who uploaded it when that upload is on record, and otherwise against the server owner. New jobs:
  `storage.uploads_sweep` (hourly) and `storage.index` (daily, and after uploads and moves). Data-subject exports
  list a member's buckets, and account erasure wipes their keys.

## Phase 27 - 2026-10-26

- **Bring your own storage.** Under User Settings > Storage you can connect your own S3-compatible bucket (S3, R2,
  B2, Wasabi, MinIO or Garage). A connection test shows what works and what does not, with least-privilege policy
  examples for your provider, and a CORS card checks the bucket and can apply the rule for you. Your secret key is
  sealed on the server and never shown again; the list shows only the last four characters of the key id.
- **Servers can connect buckets too.** Guild Settings > Storage does the same for a server, for those who manage it.
  Five new storage grants in the role editor (under Permissions, Storage) decide who may view, upload or manage the
  server's shelves.
- **Friend Shelf.** On a mutual friend's profile, or from the Friends page row menu, you can share your cinema,
  library or music shelf with that friend, kind by kind. Removing the friend removes the share.
- **Browse a bucket.** `/storage/{mount}` lists the folders and files you are allowed to see; every file is opened
  through a short-lived signed link, and a bucket that has not passed its CORS check opens nothing.
- **Choose which tabs show.** Appearance > Sections sets each media tab to show, hide or Auto.
- **For the operator:** the instance bucket is now "mount 1" and is recorded at start; the server refuses to start
  if `storage.bucket` names a different bucket (see UPGRADE.md and `sr-cli storage set-instance-mount`). A daily
  `storage.cors_recheck` job re-checks every mount, `sr-cli storage ensure-cors --mount/--all` applies the rule, new
  settings `byos.presign_ttl_secs`, `byos.presign_ttl_cap_secs` and `abuse.storage_tests_per_hour`. Staff reads of a
  member's bucket are written to the access log.

## Phase 26 - 2026-10-26

- **Phase 6 is proven.** The first scheduled SDK compatibility run passed 7 of 7 for discord.js and serenity, so the developer platform's exit gate is closed (P26-NEXT-1).
- **Emoji, stickers and sounds work everywhere.** Your server's emoji, stickers and sounds can be used in your other
  servers, in DMs and in group DMs. The emoji, sticker and soundboard pickers show one section per server you belong
  to.
- **Servers decide whether their expressions leave.** A new role permission, "Use Expressions Externally" (on by
  default), controls whether members with a role may use the server's emoji, stickers and sounds elsewhere. A
  channel that does not allow external emoji, stickers or sounds shows other servers' sections greyed out with the
  reason.
- **View as Role shows the member list and threads too.** While previewing a role, the member list and the thread
  lists show only what that role could see.
- **The DM spam filter's "All" level now covers friends.** A friend's message that looks like spam stays in the
  conversation behind a "likely spam" notice you can open; it is never moved to Message Requests.
- **The installed app updates and works offline more reliably.** After a visit to the offline page the app no longer
  kept showing "You're offline" once you were back; the What's new badge also clears in every open tab at once.
- **Phone fixes.** The Library footer links wrap and the Cinema's top bar scrolls instead of pushing the page
  sideways.
- **Bot developers:** both bot SDKs can now encode and decode Opus audio (`voice-opus` in sr-sdk, `OpusEncoder` in
  @sideroom/bot), so a bot can play a sound without bringing its own encoder; the TypeScript bot also rejoins its voice
  channel after a fresh gateway session. Building sr-sdk with `voice-opus` needs cmake.

## Phase 25 - 2026-10-26

- **The developer portal's Review tab works again.** Opening Review on an application that had a version crashed
  the page; it now lists what changed in each version.
- **Watch parties keep their playback to themselves.** Someone who is not in a watch party no longer learns what is
  playing or where it is; a participant who was dropped still sees "You are no longer in this watch party".
- **Duplicate-account checks look at the newest ban first.** When a new account is linked to several banned people,
  staff now always get the signal for the most recent ban.
- **Operators:** Admin > System has a "Stored data" card with the size of the object store, Library media and
  Cinema media, measured once a day (a very large bucket shows a lower bound until you raise
  `ops.storage_measure_max_objects` in Limits & settings); the same figures are on the `sr_storage_bytes` gauge.
  Backups now remove a half-written `.dump.partial` older than an hour. `deploy/UPGRADE.md` gained the deploy-file
  and `.env.prod` step, the Compose backup command and a rollback that restores into a fresh database, and CI now
  rehearses an upgrade and a rollback on seeded data.

## Phase 24 - 2026-10-25

- **Group DM invite links from the picker.** Creating a group DM now offers its invite link right away, with a copy
  button; the owner sees the same link when adding people to an existing group.
- **The emoji picker has a category bar.** One button per section (favorites, frequently used, this server, then
  each category) beside the grid; it follows your scroll and works with the arrow keys.
- **Profiles read better.** About Me shows bold, italic, spoilers, links and emoji, and the Settings preview wears
  your profile theme and server tag.
- **Likely spam in Message Requests.** With the DM spam filter on, a stranger's pending request that looks like
  spam (floods, repeats, many different links) moves to a "Likely spam" section; you can still accept it.
- **View Server As Role in the Lounge.** Server Settings > Roles > View as Role can now preview the channel list as
  those roles, with a banner and an Exit button. Nothing is saved.
- **GIF autoplay is a setting.** Settings > Accessibility: when it is off, or the window is not focused, or reduced
  motion is on, GIFs show their first frame and play on hover or focus.
- **Faster start with many servers.** Notification settings for all your servers load in one request instead of
  one per server.
- **Bots in voice.** Both bot SDKs (Rust and TypeScript) can join an encrypted voice call and reconnect after a
  dropped connection.
- **Operators:** new Prometheus gauges and alert rules for the job queue, backups, restore drills, the WAL archive,
  disk, invoices and CSP reports, each linked to its runbook entry (a fresh install alerts until its first backup
  and restore drill). `sr-cli seed --scale` loads a large test dataset, and the purge, audit and search jobs work in
  bounded batches.

## Phase 23 - 2026-10-25

- **A welcome after your first sign-in.** A short tour of the Lounge, the Library and the Cinema, then the
  appearance, a nudge to turn on two-factor sign-in, the app install where your browser offers it and the home
  server's own onboarding. Every step can be skipped, and Help reopens it. Notifications are only asked for when you
  open Settings > Notifications or press a bell.
- **Help is a handbook.** `/help` covers invites and credits, two-factor and passkeys, installing the app on iOS and
  Android, the Lounge for people who know Discord, submitting to the Library and the Cinema, watch parties, Live TV
  and privacy. This page is "What's new"; the help button shows a badge once when there is news, and Settings >
  What's New opens it too.
- **Keyboard and screen readers.** Every section has a skip link to its content; arrow keys move through the member
  list, the messages, the Library and Cinema grids and the TV guide; Settings and menus hand focus back to what
  opened them. Reduced motion and high contrast now reach every section.
- **Operators:** one handbook, `deploy/OPERATOR.md`, maps every deploy page and the daily, weekly and monthly checks.
  Every alert and every red Admin > System tile has a playbook, and the runbook opens inside Admin (Admin >
  Runbook). Incident playbooks cover takedown notices, data requests, a compromised staff account, a lost owner
  second factor, a full disk, a restore and a CSAM report (the lawyer-dependent steps are marked).

## Phase 22 - 2026-10-24

- **Submit a show or film from the browser.** Cinema has a new Submit page (`/cinema/submit`): start a draft, add
  seasons and episodes, drop the files in (a naming tester shows how each file name is read), and send it for review.
  Large uploads continue where they stopped if the connection drops. Reviewers can open a proxy player on the
  submission before deciding.
- **Proofs, not features.** A real ffmpeg now transcodes in CI (`media.yml`): the pipeline, re-transcodes, downloads,
  Live TV and the failure paths. Voice is tested through a TURN relay, chat attachments over real S3 multipart, and a
  10-person voice soak can be dispatched by hand.
- **Operators:** the Cinema avatar-menu entries point at the new Submit page; the Library's own list keeps to Library
  kinds (D-367).

## Phase 21 - 2026-10-23

- **Every permission claim is a test.** A matrix walks all API routes as each kind of caller (about 23,000 calls) and
  fails when a route lets someone in who should be out.
- **Sensitive staff actions ask again.** Banning, transferring a server, changing the donation wallet and similar
  actions ask for a fresh second factor (a 15-minute window); the list is in doc 07.
- **Blocked content reports.** The browser reports content-security-policy blocks to the server, which keeps counters
  only (no personal data); Admin > System shows them.
- **Upload hygiene.** Fuzzed archives, images, packs and sounds are refused cleanly. One defect was found and fixed: a
  crafted sound file could overflow a duration calculation.
- **Secrets and compromise drills** are scripted and written into the runbook (lost secret key, tampered access log,
  compromised staff account). The threat model was refreshed to the system as built.

## Phase 20 - 2026-10-22

- **A first real install.** The production Compose stack is rehearsed in CI from the shipped files, with backup,
  restore and WAL drills, and the systemd path is verified.
- **`sr-cli preflight`.** One command checks the database, storage, mail, public URL headers, TURN, VAPID, disk space,
  clock skew and secret-file modes, and prints a table. Admin > System has the same check as a card and as the last
  step of the setup checklist. `--alert-test` sends a test card to the staff alerts channel.
- **Owner packet:** a lawyer brief, an owner checklist and `deploy/GO-LIVE.md`. The README status and operator docs
  were brought up to date.
- **Moderators:** 18+ marks on Library series and Cinema titles from their pages, search follows the mark at once, and
  the Review hub shows a Reports tile.

## Phase 19 - 2026-10-22

- **Moderation dashboard.** A reports inbox you can dismiss in bulk, takedown and data-request cards, bans in force
  by reason, a health view (reports per day, time to first action, reopen rate) and a retention check.
- **Suspicion.** Staff see a person's suspicion level and history, an automatic case opens when a threshold is crossed,
  and evasion signals (shared sign-in address, shared invite lineage) are listed for review. A paused inviter sees a
  neutral notice.
- **Age handling.** Mark channels, series and titles as 18+ with a logged reason; an under-18 or under-13 finding
  locks or removes the account; embeds from age-restricted channels do not travel into other channels.
- **Data requests** can hide an account's shared activity while open, and record that an account belongs to a person
  or not.

## Phase 18 - 2026-10-21

- **Moderation cases.** Reports become cases with evidence, notes, a timeline, assignees, escalation, appeals and
  holds. Staff can restore a deleted message or remove one attachment from a case.
- **Takedown register.** Notices are logged with items, a hold on the content, a templated reply, counter-notices, a
  court-filing note, strikes and a restore date; blocked media answers 451.
- **Data requests.** Export and erasure requests get a due date, a single extension (up to 60 days), verification, and
  a named list of what is retained.
- **Modules at channel and user scope.** A module can now have per-channel settings (Edit Channel > Modules) and
  per-person settings (Settings > Modules); message quotes and level-up announcements use them.

## Phase 17 - 2026-10-20

- **Subtitles with styling.** Cinema plays ASS/SSA subtitles with their original styling (JASSUB, LGPL).
- **The composer highlights** mentions, channels and formatting as you type.
- **Compliance settings.** Registration asks for an age confirmation; operators can set their country and contact
  address, which appear on the privacy summary and the Help page.
- **Starter sticker pack** ("side-room-words") and a `sr-cli package seed` command to load packs.
- **Developers:** discord.js and serenity bots run against SIDE ROOM weekly in CI; commands that only apply to
  user installs are never shown in servers.
- **Operators:** the Discord account-link redirect URI shows in Admin > System; the optional Bitcoin node (electrs)
  is a Compose profile; integration policy changes apply without a restart.

## Phase 16 - 2026-10-19

- **Large attachments resume.** Chat uploads over 64 MB are sent in parts and carry on after a reload or a dropped
  connection.
- **Support page.** `/support` explains why to donate, the hosting bar, Patron perks, server levels and invite credits.
- **Server look.** Servers can pick a theme and an accent colour for everyone, and add emoji, sticker and sound packs
  from the package browser.
- **Watch parties** show round camera bubbles in a call; Server Settings has a Watch Parties and Library page.
- **Voice** shows each remote person's connection quality; phones get a call-controls sheet.
- **Live TV** loops a slate between programmes, so a channel never goes blank.
- **Operators:** sample Library series for a fresh install (`sr-cli seed`), and an entrance-journey test that walks
  invite, registration, authenticator app and passkey sign-in.
