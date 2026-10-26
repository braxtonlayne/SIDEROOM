# Changelog

What changed in SIDE ROOM, one section per wave, newest first. The app shows this file at `/help/whats-new`, and the
first `##` heading below is the version the build embeds: when it changes, members see a one-time badge on the help
button. The integrator adds the new wave's section at the top from that wave's handoffs
(`docs/dev/handoff/feat-p<wave>-integration.md`); see the integrator's checklist in `AGENTS.md`.

Write for people who use the place, not for people who build it: what they can now do, what behaves differently, and
what the operator now has to know. Keep each wave to a handful of lines.

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
