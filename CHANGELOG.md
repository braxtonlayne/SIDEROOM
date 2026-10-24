# Changelog

What changed in SIDE ROOM, one section per wave, newest first. The app shows this file at `/help/whats-new`, and the
first `##` heading below is the version the build embeds: when it changes, members see a one-time badge on the help
button. The integrator adds the new wave's section at the top from that wave's handoffs
(`docs/dev/handoff/feat-p<wave>-integration.md`); see the integrator's checklist in `AGENTS.md`.

Write for people who use the place, not for people who build it: what they can now do, what behaves differently, and
what the operator now has to know. Keep each wave to a handful of lines.

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
