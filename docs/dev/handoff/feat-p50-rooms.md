# Wave 50 handoff: rooms (MUS-JB-003)

Branch `feat/p50-rooms`, based on main `7ad12261`. Nothing pushed.

## Done
- **Server** (`crates/sr-server/src/jukebox_jobs.rs`): the `jukebox.rooms` tick now hands over (D-511 to D-513). A `cursor` room whose channel's jukebox is in `stream` mode with a seated DJ whose listener heartbeat is inside `LISTENER_TTL_MS` becomes `stream` (`stream_dj_id` = DJ, cursor untouched) and publishes at once; a `stream` room with no live DJ returns to `cursor` (`cursor_at = now()`, `next_due_at = now()`), resuming in place, and publishes. `room_payload` carries `dj_id` (null in cursor mode). `GET /channels/{id}/listening-room` also returns `playlist_id` (page-load read only, so the card preselects the playlist). Docs: `docs/05-api-and-gateway.md` entry, GET description in `rooms.rs`, openapi + generated client regenerated.
- **Tests (Postgres)**: `a_streaming_dj_freezes_the_room_and_their_leaving_resumes_it_in_place`, `a_synced_jukebox_is_not_a_dj`; the old "DJ stay" test now seats a live DJ. `cargo test -p sr-server --lib jukebox`: 27 passed. clippy `-D warnings` (sr-server, sr-api) clean, fmt clean.
- **Web**: `lounge/rooms/settings/ListeningRoomSettings.tsx` (lazy) is the "24/7 listening room" card on the Voice tab of a voice channel's editor (switch, playlist select from `GET /guilds/{id}/music/playlists`, status line, hint sentence, `MANAGE_CHANNELS` sentence, Save -> PUT or DELETE, follows `LISTENING_ROOM_UPDATE` while open). `RoomUpdate.djId`; the listening card names the DJ (`lounge.rooms.streamBy`). Strings in `i18n/messages/en/p50-rooms-1.ts`. `?channel=<id>&tab=<tab>` opens a channel's editor (Channels section).
- **Tests (web)**: `lounge/rooms/__tests__/settings.test.tsx` (5), `rooms.test.tsx` (named/unnamed DJ note), `settings/server/__tests__/channels.test.tsx` still passes. tsc, eslint, prettier, `check-css-tokens`, `i18n-audit`, `check-doc-links`, `playwright test --list` (907 tests) clean.
- **Docs**: D-509..D-514 appended to `docs/01-decisions-log.md`; T54 row added to `docs/15-threat-model.md`; MUS-JB-003 inventory row ticked with the "later" items named.
- Sweep: `settings-server-channel-voice` row (+ a seeded server playlist) in `e2e/tests/page-sweep.spec.ts`.

## Not done / deviations
- No link in the "Create a server playlist in the music library first" hint: the music library is a player-skin panel with no route to link to.
- The card's permission check is the viewer's server-level `MANAGE_CHANNELS` (channel overwrites are enforced by the server only).
- Threat-model numbering: my row is `T54` as drafted; `scripts/threat-model.test.mjs` fails the consecutive check until the profile builder's T53 exists (with T54 renamed T53 locally all 5 pass). Commander renumbers at integration.
- Lounge Dark was not shot separately; light (Lounge Light) and the default dark (Editorial) were, desktop and phone.

## Visual check
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p50-rooms/` (`settings-server-channel-voice-{desktop,phone}-{light,dark}.png`, `lounge-voice-*`) and `.../p50-rooms/flow-{1..4}-*.png`: a scripted click flow (turn on, pick playlist, save -> "On, nothing is playing yet"; a jukebox DJ in stream mode -> "Rowan (owner) is live, the playlist resumes when they finish"; DJ lets go -> back to On; switch off -> Off). Only console finding: the expected 404 of `GET .../listening-room` on a non-room (row has `expect4xx`).

## Owner questions

**Answered: D-535 (the hint opens the player's Library panel: built; the card stays visible and disabled, its check is the channel-level Manage Channels: built).** Questions below are kept for the record.
- Should the hint link to the music library once a route to it exists?
- Should the card be hidden (not just disabled) for members without `MANAGE_CHANNELS`? (Default here: visible, disabled, with the sentence.)
