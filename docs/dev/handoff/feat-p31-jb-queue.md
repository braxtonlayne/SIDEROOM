# feat/p31-jb-queue: MUS-JB-002 collaborative queue, vote-skip, synced local playback

## Resume here
Backend is done and committed (model, Postgres store, routes, wiring, matrix rows, OpenAPI fragment). Left:
1. Run the web gates: `pnpm install --offline --frozen-lockfile`, `pnpm --filter @sideroom/wasm build`, then typecheck, lint,
   prettier, `vitest src/voice/jukebox src/i18n src/lounge/voice --maxWorkers=2`, build + bundle budget. NOT RUN (C: was
   at 16 GB free; the node_modules install and wasm build were not started).
2. Write the Vitest for `voice/jukebox/controller.ts` (fake audio, fake api, fake clock; assert the seek on load, the
   nudge, silence on 404, `blocked`, `streamed`, and the overrun `ended` call) and `JukeboxStrip` (render states).
3. Regenerate OpenAPI: `cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, then the api-client
   generate script. NOT RUN. (The web client uses its own typed fetchers, `voice/jukebox/api.ts`.)
4. `e2e/tests/jukebox.spec.ts` (two contexts, synced mode, positions within 250 ms after 5 s) on ports 8081/5174. NOT WRITTEN.
5. Run the Postgres suite `cargo test -p sr-db --test jukebox_repo -- --include-ignored --test-threads=1` (the password
   file `%TEMP%\sr_pg_pw.txt` was missing, so it was written but never run or compiled) and the `route_matrix` /
   `openapi_names` tests (started, result not seen when this was written).
6. CHANGELOG line under "## Phase 31 - 2026-10-30" and docs/05 gateway line for `JUKEBOX_UPDATE`.

## Done
- Migration `1570_jukebox.sql`: `jukebox_sessions`, `jukebox_queue`, `jukebox_votes`, plus `jukebox_listeners` (a fourth
  table, the electorate for the vote ratio).
- `crates/sr-chat/src/jukebox/*`: `model` (Doc), `rules` (pure: settle on the server clock, enqueue caps 200 total /
  25 per member, vote-skip ratio, force skip, ended, pause/resume, mode, DJ hand-over), `store` ports (locked
  transaction), `memory` store, `ports` (access, events, event-name constants incl. `LISTENING_ROOM_UPDATE`), `service`.
  Tests: 17 unit tests pass (`cargo test -p sr-chat --lib jukebox`).
- `crates/sr-db/src/jukebox_repo.rs` `PgJukebox` (session row `FOR UPDATE`, diff writes). Tests in
  `crates/sr-db/tests/jukebox_repo.rs` (4, ignored, CI).
- `crates/sr-api/src/routes/voice/jukebox.rs`: 13 operations under `/channels/{id}/jukebox`. 6 unit tests pass:
  a track in a bucket the listener cannot read is never signed for them (`presignable`), only the playing and next
  track can be signed, titles hidden for unreadable mounts.
- Wiring: `sr-server/src/jukebox.rs` (channel standing from the chat repo: VIEW+CONNECT, SPEAK, MANAGE_CHANNELS;
  `JUKEBOX_UPDATE` over the bus as a user-scoped dispatch to a cached audience), `app.rs`, bot deny in `auth/policy.rs`,
  setting `jukebox.vote_skip_percent` in `sr-admin`.
- Route matrix rows (13): membership, presign is `storage_scope`; bots denied.
- Web: `apps/web/src/voice/jukebox/*` (api, controller reusing the Watch Party clock estimator and drift policy,
  hook, `JukeboxStrip`, css), fragment `i18n/messages/en/p31-jb-queue-1.ts`, lazy mount in `lounge/voice/VoiceChannelView.tsx`.
- Gates run: `cargo clippy -D warnings -p sr-chat -p sr-db -p sr-api -p sr-server -p sr-admin --all-targets` clean;
  fmt applied.

## Design notes and decisions
- `JUKEBOX_UPDATE` carries `{channel_id, guild_id, version}` only; clients refetch `GET`, which is filtered per viewer.
  No `sr-gateway` code change was needed (user-scoped dispatches pass through); the "two events" are only names.
- Presign is per listener at play time through the existing `objects::presign` (scopes, delist, CORS); an unreadable
  mount is the same 404 as an unknown row. The adder's reach never counts.
- The server advances tracks lazily: any read or write settles at the exact end instant, so there is no job; listeners
  that overrun the end call `ended`, which is honoured only near the server's own end.
- Roadmap name `jukebox.vote_skip_ratio` 0.5 is stored as `jukebox.vote_skip_percent` 50 (settings are integers).
  **Owner may rename.**
- Listeners are a heartbeat (`PUT .../listening` every 30 s, 60 s TTL); a vote counts while its voter is live.
- Unowned files touched, minimal: `routes/voice.rs` (mod + merge), `routes/music/tracks.rs` (`find_track`),
  `sr-admin/settings.rs`, `auth/policy.rs`, `sr-server/{app,lib}.rs`, `lounge/voice/VoiceChannelView.tsx`, `sr-db/lib.rs`.

## Still open
- Stream mode on the DJ's client (feeding the controller's audio into jb-sfu's music sender, MUS-JB-001): the controller
  plays locally for the DJ and reports `streamed` to others; the capture hook is integration work with jb-sfu.
- `PUT/DELETE /channels/{id}/listening-room` for jb-rooms: not added (out of time and disk); list for integration.
- Gapless prefetch between synced tracks (a short gap while the next presign loads).

## Owner questions
- Keep `vote_skip_percent` (integer) or add a decimal setting kind?
- Should a listener's duration of a private track be visible ("not shared with you" rows show length, not title)?
