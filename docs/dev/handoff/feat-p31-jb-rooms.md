# Handoff: feat/p31-jb-rooms (MUS-JB-003, 24/7 Listening Room channels)

Branch `feat/p31-jb-rooms`, base `integration/phase31`. Migration range 1580 to 1589 (only 1580 used). Job id 1580 (grepped: no other `*_JOB_ID` uses it).

## Design in one paragraph

A voice channel is a listening room exactly when it has a row in `listening_rooms`. The row is a cursor over a server playlist: which playlist item, how many ms in, as of when, plus a `seq` that counts track changes. The server never streams audio: the job only advances the cursor and publishes `LISTENING_ROOM_UPDATE`; each listener presigns the track from the server shelf and plays it locally, seeked to the cursor.

## Done

- `crates/sr-db/migrations/1580_listening_rooms.sql`: table `listening_rooms` (the flag is the row; a second boolean on `channels` could drift), `mode` cursor|stream with a CHECK that stream needs a DJ, `next_due_at` with a partial index so the job touches only due rooms.
- `crates/sr-server/src/jukebox_jobs.rs` (new, mine): job `jukebox.rooms` (singleton id 1580, every 2 s, registered and scheduled in `app.rs`, two lines).
  - `cursor` module, pure: `advance` (across boundaries, loops, any outage via `elapsed mod lap`, no overflow), `upcoming` (next three, wraps), `skipped`. 11 unit tests.
  - `step` and `room_payload`: publish on a track change at once, otherwise every 15 s; an emptied room is announced once; next look is the nearer of the track end and the 15 s publication. 6 unit tests.
  - `tick`: due rooms `FOR UPDATE SKIP LOCKED`, at most 200 per tick, one transaction, events returned and published after commit.
  - Rules: a track with no duration (null or 0) is skipped, not guessed (a cursor cannot know when it ends); the card says how many were skipped. A room plays only tracks on the server's own shelf or the instance bucket (a 24/7 room must not depend on a member's bucket, which vanishes when they leave, and the titles would disclose it).
- Event `DomainEvent::ListeningRoomUpdated` (sr-events), gateway dispatch `LISTENING_ROOM_UPDATE` (sr-gateway `hub.rs`, channel-scoped so a hidden voice channel never gets it; intent `GUILD_VOICE_STATES` for bots in `protocol.rs`), documented in `docs/05-api-and-gateway.md`, `gatewayDocs.generated.ts` regenerated. A gateway test covers channel visibility and the intent.
- `crates/sr-server/src/jukebox_pg_tests.rs`: six Postgres tests (see below).
- Web: `apps/web/src/lounge/rooms/` with `cursor.ts` (parse, order, `planJoin`, `follow`), `store.ts`, `join.ts` (`joinRoom` presigns and plays at the cursor, `followRoom` seeks only beyond 2.5 s drift, rejoins on a new track, leaves when a DJ takes over), `ListeningRoomCard.tsx` (description, now playing, next three, listen/stop), `rooms.css` (logical properties), mounted under a voice channel in `ChannelSidebar.tsx`; strings in `i18n/messages/en/p31-jb-rooms-1.ts`. The player is imported on first click, not with the lounge. Vitest: `lounge/rooms/__tests__/rooms.test.tsx`.
- `docs/dev/load-testing.md`: the `jukebox.rooms` row.

## Not done (and why)

- **REST for rooms** (turn a voice channel into a room, pick the playlist, read the current cursor on page load): not built. It needs channel permission plumbing (`MANAGE_CHANNELS`, view check), a `ListeningRooms` extension wired in `app.rs`, route-matrix rows, and OpenAPI regeneration, and the files (`routes/voice/*`, `app.rs` wiring) belong to jb-queue's route group, not my Split row. Until it exists a room can only be created by a row insert, and the card appears when the first `LISTENING_ROOM_UPDATE` arrives (at most 15 s after the session starts hearing it; there is no fetch on page load).
- **The channel settings card** (the toggle and the playlist picker): not built, same reason (nothing to call). The i18n fragment has no settings keys for that reason.
- **Stream mode for a present DJ**: the column and the event field exist and the job leaves a `stream` room alone (tested), but nothing sets it. Switching needs the voice presence of jb-sfu's music track (MUS-JB-001) and the REST above; the card shows the "a DJ is playing live" note when it arrives.
- **Playwright**: not run (rule for this wave).
- The CHANGELOG "## Phase 31 - 2026-10-30" section is the integrator's; suggested lines: "Voice channels can become 24/7 listening rooms: the server keeps the place in a server playlist and everyone who listens along hears the same track at the same moment. The server never streams the audio; your browser plays it from the server shelf."

## Tests

See the final report for what actually ran. Postgres tests could not run here (the password file `%TEMP%\sr_pg_pw.txt` is missing); CI must run, with `--include-ignored --test-threads=1`, `cargo test -p sr-server --lib jukebox_pg_tests`:
`the_cursor_crosses_a_track_boundary_and_a_loop`, `a_track_without_a_duration_is_skipped_and_counted`, `a_quiet_room_publishes_every_fifteen_seconds_not_on_every_tick`, `only_the_servers_own_shelf_and_the_instance_bucket_are_played`, `an_empty_playlist_is_announced_once_and_a_dj_stay_freezes_the_cursor`, `a_second_tick_inside_the_first_skips_the_locked_room`. They also exercise migration 1580 (never applied to a database in this session; the SQL was written without a way to run it).

## Decisions

- The flag is the row, not a `channels` column.
- Missing duration: skipped (not an assumed length).
- Shelf rule: the server's own shelf and the instance bucket only.
- Job tick 2 s, publish 15 s, 200 rooms per tick.

## Owner questions

1. A listening room shows track titles to everyone who can view the voice channel, whether or not they hold the server music grant (they cannot play without it). Acceptable, or should the titles be withheld from members without `VIEW_GUILD_MUSIC`?
2. Rooms play from the server shelf and instance bucket only. Should a server owner be able to allow a member's bucket for a room (it stops when that member leaves)?

## Conflict notes for the integrator

- `crates/sr-events/src/event.rs`, `crates/sr-gateway/src/hub.rs`, `protocol.rs`, `tests.rs`: one variant, one dispatch arm, one intent arm, one test; jb-queue adds two events in the same places, so expect adjacent-line conflicts.
- `crates/sr-server/src/app.rs`, `lib.rs`: two lines and one `mod` each.
- jb-queue's model was not available when I started (no checkpoint to merge), so the job reads `music_playlist_items` directly and does not depend on their tables.
