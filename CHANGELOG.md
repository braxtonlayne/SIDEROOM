# Changelog

What changed in SIDE ROOM, one section per wave, newest first. The app shows this file at `/help/whats-new`, and the
first `##` heading below is the version the build embeds: when it changes, members see a one-time badge on the help
button. The integrator adds the new wave's section at the top from that wave's handoffs
(`docs/dev/handoff/feat-p<wave>-integration.md`); see the integrator's checklist in `AGENTS.md`.

Write for people who use the place, not for people who build it: what they can now do, what behaves differently, and
what the operator now has to know. Keep each wave to a handful of lines.

## Phase 66 - 2026-11-13

- **Server limits are real.** One account can be in at most 200 servers (created or joined) and one server can hold at most 25,000 members, bots included; the operator can change both in Admin > Settings. At a limit, creating or joining a server says so in plain words ("You're in the most servers one account can be in here", "This server is full."); nobody is removed when a limit is lowered, and the Staff Server is never counted.
- **Settings that did nothing now work.** The operator's standard attachment size and how long large attachments are kept take effect on the next upload. Two settings that could not work (message length, the invite credit price) are gone from the list.

- **Admin wording stays with staff.** The words of the Admin pages are no longer sent to members: the message catalog is split, and the staff half lives with the Admin code, served only to appointed staff.
- **Lounge.** The server menu lost two "coming soon" entries; invites can grant temporary membership; age-restricted channels, new servers and custom-status emoji show in the Lounge; member menus can start a call; Join a Server links to the Server Directory; bug reports include how often the connection dropped and how fast the last heartbeat came back; the developer docs' Gateway guide opens again.
- **For operators.** Releases now include the web build as `sideroom-<tag>-web.tar.gz`, listed in `SHA256SUMS`; the Staff Server gets `#new-members` and `#donations` (new installs from the bootstrap, existing ones at the next start); Admin > Bug reports warns once the beta reporter is on with 100 or more accounts; `GET /gateway/bot` now says `reset_after: 0` (it never counted down); `.sideroom.json` accepts `book` and `comic`.

## Phase 65 - 2026-11-13

- **Big games.** Activities and arcade games are no longer capped at 32 MB. Packages are checked as they arrive and never loaded into memory, so the limits are settings the operator controls: 512 MB for everyone, 4 GB for reviewed apps and 32 GB for official ones by default. Big uploads go up in resumable parts, and large files play in pieces as the game asks for them.
- **Staff tools stay with staff.** The Admin app's code is only sent to staff accounts, and never kept on the device.
- **Safer storage.** A storage bucket that doesn't honour safe download types gives out no download links, and older buckets are checked for it every day.
- **Steadier alerts.** Link-preview settings and the staff alert feed recover when the server falls behind.

## Phase 64 - 2026-11-12

- **Phones.** Tapping Friends or Message Requests opens the page, with a back button. In a DM, the header keeps the name, call and video in view and puts the rest under "More".
- **For operators.** The storage settings screen knows the connection test's new download-type check.
- **Beta bug reports.** When the operator turns on beta reports, press Alt+Shift+B (or use the help menu, or the You tab on a phone) to send a bug report with a screenshot you can black out and the exact debug details shown before sending. Staff read them in Admin.
- **Steadier connection.** If the server falls behind on updates, it quietly sends you a fresh copy of what you can see, without dropping your voice call.

## Phase 63 - 2026-11-12

- **Friends on phones.** The Friends page, message requests and the inbox fit a phone screen, and a friend's menu opens where you can tap it.
- **Blocking, explained.** The Block dialog now also says that a blocked person's messages in servers and group DMs you share are collapsed for you.
- **Safer storage links everywhere.** Every link to a file in a connected bucket (shelves, showcases, music, the jukebox, the scrapbook, artwork) plays media in the browser and downloads everything else. "Open file" on a document now downloads it instead of opening a tab. Audiobooks (.m4b) play.
- **For operators.** The storage connection test checks that the provider honours the download type, and fails one that does not.

## Phase 62 - 2026-11-12

- **Faster first load.** Help, the Library browser and the shelf pages now load when you open them, so the app's first load is about 16% smaller.
- **Fewer accidents.** Blocking someone, removing a friend, ending a poll and ending a stage ask you to confirm first.
- **Discord bridge on phones.** Each mirrored channel is shown as its own block, so Remove and any warning are always in view.
- **Safer storage links.** A link to a file in a connected storage bucket plays media in the browser and downloads everything else, so a file can never open as a web page.
- **For operators.** The voice server reports how much it drops from members who send more than the limits.

## Phase 61 - 2026-11-12

- **Discord bridge.** If another server on this instance also bridges one of your Discord channels, the bridge screen says so on that channel. Messages a deleted account left on Discord are retried every day, and any that still can't be removed are listed so you can delete them by hand. A hiccup no longer leaves a message posted twice.
- **Editing and settings.** If an edit fails to save, the editor stays open with what you typed. Layout changes you make while the app is offline are no longer lost.
- **Group DMs.** If some people can't be added, the group still opens and tells you who was missed.
- **Signing out.** Signing out also removes the Notes key from the device, so on a shared browser each account unlocks its notes again next time.
- **Voice.** The voice server limits how much each person can send, so one modified client can't flood a call.
- **For operators.** Files uploaded straight to a storage bucket can't be served as web pages or images that run code, and an Activity's old files are kept for a day after an update so people mid-session aren't cut off.

## Phase 60 - 2026-11-12

- **Discord bridge.** Deleting or editing a message reaches Discord even if you do it while the message is still being sent, and a queued post is no longer lost on a hiccup. A Discord channel can be bridged into only one server, and replacing the bot token keeps the bridge on the same Discord server.
- **Deleting your account.** Your personal data goes with it: watch and reading progress, bookmarks, notes, and saved or private lists. What you posted for others stays as "Deleted User", and any data export you asked for is withdrawn.
- **Sign-in.** Wrong two-factor codes count against the account wherever they come from, and a password-reset link no longer works on a suspended or banned account.
- **Servers.** A ban always wins over a join happening at the same moment. If the live connection falls behind, it re-checks which servers you belong to, so a missed kick still takes effect. A role mention only notifies that role's members, and event interest updates follow channel visibility.
- **For operators.** Without email set up, the server never writes a message body to its log. Seeding demo accounts on a non-local server needs its own password.
- **For staff.** Erasures, data exports and patron notices are all-or-nothing with their audit rows, feature and library changes are logged before they're made, and "Publish now" can't race a withdrawal.

## Phase 59 - 2026-11-11

- **Privacy.** When you're invisible, nobody sees what you're playing. Someone you've blocked no longer sees your online status anywhere, including the member list.
- **Events.** An event in a channel you can't see no longer shows up for you. An event starts on time even if its creator can't open the stage; a stage moderator can open it.
- **Safer moderation.** "Delete messages" on a ban also clears threads. A banned person can't be let back in by approving their application. A moderator can't move someone into an 18+ voice channel they haven't confirmed for, and webhooks can't be moved into channels their manager can't see.
- **Reliability.** Posts are refused instead of slipping through if a posting restriction can't be checked, a trailing dot no longer gets a link past the AutoMod blocklist, and failed library uploads and imports clean up after themselves.
- **For staff.** Automatic moderation never freezes or suspends a staff account on its own. Hold reviews, takedown reads and account searches are logged, and the audit logs can't be wiped from the database.

## Phase 58 - 2026-11-11

- **Game shelves.** The messages about a removed game are clearer: the removal day says UTC, and a game removed because its shelf stopped answering says to list it again once the shelf is back.
- **Storage.** Pressing Test on a bucket you switched off no longer switches it back on.
- **For staff.** The review tabs are now "App queue" and "All applications". A takedown's detail shows whether each file's fingerprint is recorded, being taken, waiting for a switched-off bucket, or matched by tag and size only. A fingerprint is never taken from a file whose size has changed since the takedown.

## Phase 57 - 2026-11-11

- **For developers.** If staff suspend or unsuspend your application, you get a message saying so and why.
- **Game shelves.** A game whose shelf hasn't answered for 7 days running is taken off the list, and the person who listed it is told.
- **For staff.** Review queues have an Applications tab: search every application, see who suspended one and why, and suspend or unsuspend it (with a second factor). Takedown fingerprints are taken in the background after a notice is upheld, so upholding no longer waits on large files, and older takedowns are brought up to date once.

## Phase 56 - 2026-11-11

- **Recovery codes for passkey users.** If you sign in with a passkey and have no authenticator app, you can now regenerate your recovery codes from the passkeys card.
- **Reply in Cinema.** Cinema comments have a Reply button, like the Library.
- **Clearer channel permissions.** The "Sync with category" switch and the Permissions tab follow your permissions in that channel, not just the server-wide ones.
- **Game shelves.** A shelf that is briefly unreachable no longer pulls its games. A game removed because its file is gone or broken tells the person who listed it, and the server's audit log records it.
- **For staff.** Warning from a case, restricting processing on a data request and suspending an application ask for a second factor; nobody can grant server-pool perks to a server they own. Takedowns also match a file by its contents, and the owner of a matched copy is told and can counter-notice.
- **Fixed:** the account removal card showed "{years}" instead of the number.

## Phase 55 - 2026-11-07

- **Your sign-in is harder to change behind your back.** Changing your password, removing an authenticator or passkey, regenerating recovery codes or linking Discord asks for your password and your second factor.
- **Encrypted calls know your devices.** Each person can have several known privacy codes (one per device), so joining from your phone no longer warns everyone; a code nobody has seen before shows a one-time "new device or new sign-in" notice.
- **Deleted comments keep their replies.** A deleted Cinema or Library comment that has replies shows "Comment deleted" instead of taking the replies with it, and Cinema comments have a replies view.
- **Quieter failures.** A channel or member list that can't load says so in place with Try again, instead of a pop-up.
- **Photos stay upright.** Very large photos keep their rotation.
- **For staff.** Every restriction, suspension and perk action asks for a second factor, and nobody can grant a perk to their own account. Takedowns follow a file's contents across renames and shelves.

## Phase 54 - 2026-11-07

- **You hear about failures.** When deleting, pinning, voting, blocking or another action fails, SIDE ROOM tells you and puts things back the way they really are. A channel that can't load says so instead of looking empty.
- **Encrypted calls warn about new keys.** If someone's privacy code changes, the shield in the call says so until you verify them again.
- **Signing out cleans up.** Signing out (or being signed out elsewhere) clears what the app kept on this device, including offline Library chapters and shared files.
- **More confirmation for sensitive changes.** Setting up an authenticator app asks for your password, and staff actions like bans, badge changes and delisting ask for a second factor.
- **Fairer limits and fewer races.** Daily download and report allowances can't be beaten with parallel requests, and settings saved at the same moment no longer undo each other.
- **For operators.** Release builds refuse test-only settings, background jobs keep retrying instead of stopping, and `gateway.max_unidentified_per_ip` is a setting.

## Phase 53 - 2026-11-07

- **Safer sign-in.** Signing in with Discord now asks for your passkey too, and logging out (or "log out everywhere") disconnects your other open sessions straight away.
- **Blocks work everywhere.** Someone you block can no longer message you in an existing DM, their mentions don't notify you, and you don't see them in comments, lists or your guestbook.
- **Your files stay yours.** Sharing a shelf shares its media, covers and subtitles, not every other file in the bucket.
- **No more lost data on a bad connection.** If a profile, note, birthday or game save can't be loaded, SIDE ROOM says so and won't overwrite it with an empty one.
- **Staff fixes.** Removing a staff member and revoking an invite now ask for a reason and work again. Channels holding moderation evidence can't be deleted until the hold is released.

## Phase 52 - 2026-11-07

- **Trimmer matches your theme.** The audio trimmer's sliders and number fields follow the theme, and the soundboard
  dialog shows a single Cancel button.
- **Privacy settings fit a phone.** The Profiles privacy rows stack their label above the choice, so nothing runs
  off a narrow screen.
- **Clearer empty search.** A file-picker search that finds nothing now says no file matches, instead of saying the
  shelf has nothing the game can use.

## Phase 51 - 2026-11-06

- **Smaller soundboard sounds.** The upload dialog now saves sounds as Opus where your browser can record it (WAV
  otherwise), and WebM/Opus files are accepted. If your browser cannot play a sound, it says so instead of failing
  silently; members whose browser can still hear it in a call.
- **Search the shelf picker.** The file picker in games and the profile showcase has a search box that looks
  through the whole shelf, not just the first 3,000 files.
- **Rejoin a watch party.** If your connection drops long enough to take you out of a party, "Rejoin the party"
  brings you back, including into a private party you were already in.
- **Zoom reaches the drawer and player.** Your zoom setting now applies to the drawer, gadgets, pin rail and player
  dock, and dragging a gadget follows the pointer at any zoom. Menus and tooltips opened inside a dialog now show
  above it.

## Phase 50 - 2026-11-06

- **Profile pages grew.** Your profile ring now shows around your picture on your page for the people you chose. A
  new Showcase block plays up to six songs or videos from your own shelves, for signed-in visitors only. Profile
  owners and visitors can report a guestbook note, and staff can remove it from the case.
- **24/7 listening rooms in channel settings.** A voice channel's Voice tab has a "24/7 listening room" card: turn it
  on and pick a server playlist. When someone goes live as the channel's DJ the room pauses for them, and picks up
  where it stopped when they finish.
- **Watch PeerTube together.** Links from the PeerTube instances your operator lists offer "Watch together", and HLS
  streams from listed hosts now play in Chrome and Firefox too. Operators: the new `SR_MEDIA_HOSTS` setting lists
  extra hosts that may serve those videos.
- **Player skins got real extras.** Valve has tape speed and a balance knob, Circuit a tempo fader, Tidepool a
  "Drift off" sleep timer, and Study an ambience mixer (rain, crackle, café) made in your browser.
- **One audio trimmer everywhere.** Soundboard uploads now use the same trimmer as voice messages and the Audio tool:
  pick the start and end, and long sounds are cut to fit.

## Phase 49 - 2026-11-05

- **Active lobbies on the Arcade.** A row at the top of the Arcade shows the games running right now in your servers,
  only in channels you could join: the game, where it is, how many play and for how long, never who. Join takes you
  there (into the call first for a voice channel); Return goes back to one you are in. Game cards show "N live".
- **New Arcade covers.** Featured picks, server cartridges and games still in development get painted scenes instead
  of placeholders, and the house games have more depth. A featured house game keeps its own cover.
- **Choose a prompt pack from your shelf.** In Sketch & Slander, "Choose a file..." opens the app's own shelf picker:
  only the file you pick is shared with the game. Pasting a link still works.

## Phase 47 - 2026-11-05

- **Sketch & Slander, a party game.** One player draws a secret prompt and everyone else races to guess it. Start it
  from a channel; friends without an account join from their phones through the Share link's QR code and draw on
  their screens. A server can load its own prompt packs from its shelf.
- **A new Arcade.** The Arcade is now a proper games front page, an editorial room like the Cinema: big cards for the
  house games, a Play button that starts the game where it belongs (your call, a voice channel it joins first, or a
  text channel), server cartridges and what's coming soon. The Emulator opens inside it. Pick its look under
  Appearance > Sections.
- **House notes formatting.** Headings in any language get working links, quotes keep their line breaks, and the
  preview in Admin shows exactly what members will see.

## Phase 46b - 2026-11-02

- **Everything was checked on a phone and a desktop, in light and dark.** Dozens of pages that overflowed, overlapped,
  ignored your theme or showed placeholder text now look and behave right, from settings and the Lounge to Cinema,
  The Study, the Arcade and the sign-in pages.
- **Links are easier to read.** In the dark Lounge themes, links and embed titles are a brighter blue; in Lounge Light
  they are a deeper blue instead of pale gold. Buttons in the Lounge now use their proper colours, so primary and
  danger buttons stand out.
- **Kick, Ban and Timeout moved into a "..." menu** on each member in Server Settings > Members, as in Discord.
- **A game already running in the channel?** The + menu now offers to join it instead of starting another.
- **Clearer messages when something didn't work:** expired sign-in and email links say so in the heading, a video that
  isn't ready takes you back to its title page, and message requests opened from a link show the request.
- **Saving a settings page no longer throws away what you typed while it was saving.**

## Phase 46 - 2026-11-01

- **Land.io keeps going when the host leaves.** If the member running the game leaves the call, the person who has
  been in the game longest takes over and everyone keeps their land, cards and place. Guests who came in on that
  member's guest link need a new link from someone still in the call.
- **Draft Night is now a three-round run.** Your first pick each round is a keeper you hold for the whole run, cards
  come in three families with set bonuses, and the run ends with standings and a champion. Guests on a phone can play
  it from a guest link.
- **Bots fill small games.** With only one or two people in a mode that needs more, Lumen bot and Tofu bot join to
  make up the numbers and step out as people arrive.
- **Arena 3D, a small 3D game and starter kit.** Walk, jump, crouch and shove boxes around with friends in a call; it
  is also the template for SIDE ROOM's 3D games.
- **Games can ask to open a file from your shelves.** A game that declares it asks you first, once for each file, and
  gets only that file for ten minutes.

## Phase 45 - 2026-11-01

- **If a screen breaks, you get a plain page with a Reload button** instead of a blank window. Behind it, the app
  sends the team a short crash note: which screen (by its name, never its address), where in the code, which browser
  features were present and the last few game or Activity button presses as codes. It never includes your messages,
  files, links, keys or anything you typed, and nothing is sent before you sign in.
- **For staff:** crash notes arrive in a new staff-only `#crash-triage` channel in the Staff Server, one card per
  distinct crash with a running count. A developer (or a coding agent working for one) can reproduce the bug from it
  and propose a fix; **nothing is ever deployed from a card**, every fix goes through a normal review.
- **Preview a lived-in SIDE ROOM (test deployments).** When the operator turns on the "Showroom" switch (off by
  default), members get a "Preview populated state" button in the rooms bar and the Arcade: ten sample servers in the
  rail, friends' and servers' shelves in The Study, and an Arcade split into "Playable now" and "Preview / in
  development". Everything in it is made up, a "Preview" banner stays on screen, and nothing you click in it is saved
  or sent. Turn it off with the same button or from the banner.

## Phase 44 - 2026-11-01

- **Moderation is now a set of proportionate tools.** Staff can warn, time out, pause someone's invites for a chosen
  time, revoke their unused invite codes, or ban with a stated reason. Every one of these is written to the audit log,
  and the member sees it in plain words under Settings > Privacy and can appeal it once.
- **A serious ban opens a review, not a purge.** Only accounts the banned member invited that never did anything (at
  least a week old, no messages, no voice time) are paused automatically; they can still sign in and appeal, and are
  removed after 30 days unless staff clear them. Everyone else is only listed for staff to look at, one by one.
- **Pausing someone's ability to invite takes two people.** It needs a written reason, a second staff member's
  approval and a second-factor check from both, lasts 12 months and can be appealed. Acting on a whole branch at once
  is reserved for the Operator, behind the same safeguards.
- **The public notice says what happened, not who.** A serious case shows a short banner with the kind of offence and
  how many accounts were affected; a monthly transparency page adds the counts up. No names, no invite tree.
- **Vouch for people you trust.** On a member's profile you can vouch for them (and take it back). The vouch count
  sets a tier: Citizen, Pillar (50+) or Elder (500+). Higher tiers carry more weight, so automated penalties on them
  are longer (1.5x, 3x), as the tooltip says. If someone you vouched for is banned for a serious reason, you get a
  private note and your vouch is removed; nothing happens to your account. Vouching needs a profile page you can see.

## Phase 43 - 2026-11-01

- **Invite someone to play without an account.** In a voice call, a member playing a game that allows guests can press
  "Share link" and send `/play/...` (or show its QR code). The person who opens it gets the game and nothing else: no
  chat, no member list, no profiles, no shelves and no part of the call. They play under a random name such as "Amber
  Otter 42".
- **The game's messages stay private.** The secret part of the link (after the `#`) is made in the sharer's browser
  and never reaches the server; the game's traffic between guests and the call is sealed with it.
- **Links run out.** A link works for 6 hours, a guest's visit for 2, and the sharer can revoke it at any moment;
  guests are disconnected within seconds. At most 5 links per game and 16 guests per link.
- **For game makers:** `guest_play` and `lobby_channel` are now manifest capabilities. `ingame_voice` is accepted in
  a manifest but guests are told it is not available yet.

## Phase 42 - 2026-11-01

- **The Arcade is a room of its own.** `/games` opens on the Operator's Featured games, your servers' Guild
  Cartridges and a Coming Soon showcase with a "Notify me" toggle. The emulator is a card inside it. The Arcade link
  is always in the main rail.
- **Two kinds of game, and nothing else.** Every game says whether it is an original (all its code, art and sound are
  the developer's own) or a clean-room reimplementation (desktop app only, no bundled assets). Games built on someone
  else's names, characters or assets are not accepted anywhere, unlisted cartridges included.
- **Tip the people who made it.** Games (and `/~handle` profiles) can show Ko-fi, Patreon, Bitcoin or Lightning links.
  The money goes straight to them; SIDE ROOM takes nothing.
- **Games run in a sealed box.** A game can no longer see the app's cookies or storage or reach other pages. It talks
  to SIDE ROOM only through the Activity bridge, keeps its saves there, and reaches only the servers its reviewed
  settings name. Games that send data to their own server keep working.
- **Know what you are running.** Cards show OPERATOR-FEATURED and SWARM-SAFETY-SIGNED (the operator signed this exact
  code after an automated check). Members in good standing can "Vouch for Build"; when the code changes, the badge
  turns to UNVERIFIED UPDATE with the previous build's count until people vouch again.
- **Cartridges from a shelf.** A server can list a game or mod straight from a shelf it can see; each member opens the
  file with their own access, and a file taken down disappears from the list.

## Phase 41 - 2026-11-01

- **Three rooms.** After the Entrance the main rail is now The Lounge, The Arcade and The Study. The Study opens on
  your shelves (`/study`), with Media (the Cinema) and Reading (the Library) next to them.
- **Shelves by source, not by genre.** The Study's sidebar lists My Files, one entry per friend who shares a shelf with
  you, and one per server whose shelves your role can see. Every card says whose bucket it is (`@handle's bucket` or
  `Guild: <name>`). Filter by Video, Audio, Reading, Game & Mod Packs, Images or Other. Covers come from the files
  themselves (album art, the first page of a comic or book, a frame of a video), never from an outside service.
- **Your words on your files.** The owner of a shelf (and a manager on a server's shelf) can give any file a title, a
  cover image from the same bucket and a note. Continue Watching, Continue Reading and Pinned Folders stay in your
  browser; Recently Added shows the newest files on the shelves you can see.
- **Sharing a shelf follows your account.** A friend or a server role you grant can open your shelf on any device. They
  never get your bucket keys: each file opens through its own link that works for that one file for ten minutes.
  The shelf's grant list shows who can see it, with the history. Staff can delist a single file after a valid notice;
  it disappears for everyone.
- **The Screening Room.** In a voice channel, Watch Together, a link to the shelves and the Live TV guide now sit
  together in one panel.
- **Storefront surfaces are off.** Movies, Series and Anime categories, trending lists, the Requests board, uploads
  for review and the staff review queue are switched off by default (they are hidden, not deleted), and nothing looks
  titles up on outside metadata services. Operators can switch each back on in Admin > Features.
- **A new default look for the Lounge.** The Editorial theme (warm charcoal, cream, muted gold, serif headings) is the
  Lounge's default; every other theme is still in Settings.

## Phase 40 - 2026-10-31

- **Help covers everything that is new.** The Help page now has a section each for your own storage, the player and
  the jukebox, the utility drawer, profiles with status rings and the scrapbook, the Discord bridge (how a server
  owner switches it on and off, and the two modes), Activities (the card table is play money), and retro games (bring
  your own ROMs, and your own BIOS for PlayStation), next to the existing game-servers line that says they are coming
  later.
- **For operators: the module switches, written down.** Help > If you run this server (`/help/operators`) lists every
  switch in Admin > Features and what it does. `deploy/OPERATOR.md` gains the storage, music, drawer, bridge and
  emulator settings and every background job of the feature phase, and the owner checklist lists each decision and
  download that is still waiting for a yes (SNES and Genesis cores, the homebrew games, the first real Discord run,
  tantivy 0.27 on or after 2026-11-17, and the rest).
- **Looking back.** Wave 33 was folded into the utility drawer of wave 32. Wave 38, game servers, is on hold at the
  owner's request: only the "coming soon" page exists, and nothing else is planned until the owner says.
- **Checked for the record.** The threat model now covers the Discord bridge (including its switch-off and the
  whole-server mode), profile pages, status rings, the emulator and the drawer; the retention table lists every
  new thing the instance keeps and for how long; and the lawyer brief has the question about music in voice.

## Phase 39 - 2026-10-31

- **Game servers: coming soon.** Server Settings has a new Game Servers page (and the Activities picker in voice a
  small tile) showing what hosting a game server could look like, "Host it yourself" and "Managed servers". It is a
  preview only: nothing on it works yet, and the help says game servers are coming later.
- **Invites.** User Settings > Invites now says when you hold the most unused codes allowed (20 by default). Unused
  codes older than 30 days are revoked automatically; the people you invited are never affected.
- **For operators: Lock the Doors.** Admin > Invites can close registration entirely (a reason is required and the
  change is logged). Nobody can join while the doors are closed, not even with a valid or emergency invite; members
  already here are unaffected, and the landing page says the doors are closed. Admin > Invites also shows the last
  cleanup pass and invite branches whose first member never activated. New settings `invites.default_credits` (was
  `invites.initial_credits`), `invites.max_outstanding` and `invites.unused_code_days`; see `deploy/OPERATOR.md`.
- **For operators: a switch for every module.** Admin > Features now lists the jukebox, listening rooms, the utility
  drawer (and each tool in it), the activity data channel, the emulator, Watch Together embeds and the game-servers
  page, beside the earlier ones; switching one off stops its routes. Nothing can switch off call encryption,
  authorization or the logs.
- **Upgrade note: public profiles need a second switch.** A public profile page now needs both
  `platform.public_profiles` and the new setting `profiles.access = opt_in_public` (default `walled`). If you had
  public profiles on, set the setting after upgrading or the public pages go dark (`deploy/UPGRADE.md`).

## Phase 37 - 2026-10-31

- **Games.** A new Games page (in the section bar) plays retro games in your browser: NES, Game Boy, Game Boy Color,
  Game Boy Advance and PlayStation. The ROMs come from the `emu/roms` folder of your own bucket and of servers that
  share theirs; the game runs on your device. Keyboard or gamepad, with a Controls screen to remap them. Save states
  and in-game saves go to your own bucket, never to this instance. The instance ships no games: only play ROMs you own
  or that are free to share. SNES and Genesis say "core not installed" for now.
- **PlayStation needs your own BIOS.** Put the BIOS file from your own console in `emu/bios` of your bucket. It is
  checked, used for that visit only and never stored or cached anywhere.
- **Play together.** In a server's voice call, the person running a game can host it: share the game screen and up to
  three others play as players 2 to 4 with their keyboard or gamepad, encrypted end to end. There is no netplay
  rollback, so slow and turn-based games work best; the round trip is shown. Taking a seat needs the "View shelf
  games" permission on that server; everyone in the call can watch.
- **For server admins.** Roles have a sixth storage toggle, "View shelf games", which lets members play the ROMs in
  the server's shelves.
- **For operators.** The emulator cores are not in the release: fetch them with `deploy/scripts/fetch-emu-cores.sh`
  (or `sr-cli emu fetch-cores`, or the installer's optional step), which downloads each file from its pinned URL and
  checks its SHA-256. `deploy/EMULATOR.md` has the table, the licences and the source links. Migration 1800.

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
