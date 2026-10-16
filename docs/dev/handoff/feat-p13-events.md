# Handoff: feat/p13-events

Branch `feat/p13-events` (base `integration/phase13`). Migration used: `0820_scheduled_event_image.sql` (one file carries all three columns).

## Done

| Item | What | Where |
|---|---|---|
| G-75, G-89 | Already built in phase 12; ticked with a note. P13 added the missing duplicate-migration-prefix test. | `crates/sr-db/tests/migration_versions.rs` |
| G-90 | `InviteObject.type` always present, `guild` is `null` (schema nullable) for groups. Group join refused with 404 when joiner and owner block each other (either direction); the 10-recipient cap was already there. Web: invite page, chat card, Join a Server and the store switch on `type` (only `1` is a group). Doc 05 invite row. | `routes/settings/invites.rs`, `sr-chat/src/social/service.rs`, `apps/web` (`InviteCard`, `ServerInvite`, `AddServerDialog`, `store.ts`) |
| G-91 | Pure rules `scheduled_events::advance` / `start_on_join`; service `advance_events`, `start_on_voice_join`; every move dispatches `GUILD_SCHEDULED_EVENT_UPDATE` and writes audit 101 (clock moves as the system user, D-187). `VoiceJoined` listener starts voice/stage events. | `sr-chat/src/settings/scheduled_events.rs`, `service/event_ops.rs`, `sr-server/src/events_jobs.rs` |
| G-92 | `scheduled_events.tick` singleton job (id 820) replaces the per-node 30 s task. The handler re-arms 30 s ahead before it works, so a node that dies mid-turn leaves a queued job. `advance` is idempotent. | `sr-db/src/jobs.rs`, `sr-server/src/events_jobs.rs`, `app.rs` |
| G-93 | `image` on create/patch (`MediaKind::EventCover`, 2 MiB, JPEG/PNG/GIF only, CDN kind `guild-events`, body limit layer on the two routes); stage events open/close a stage instance (`Gateway::stage_open_for_event`, `entity_id` = stage channel id, instance JSON carries `guild_scheduled_event_id`); start notification = user-scoped `SR_SCHEDULED_EVENT_START` plus Web Push (D-191 suppression, per-server "mute new events" honoured). Web: cover in Create Event and Events dialog (change/remove), start toast with "View event", `?event=<id>` link opens the dialog on the event. | `routes/settings/scheduled_events.rs`, `routes/media.rs`, `sr-storage/src/media.rs`, `sr-gateway/src/voice_stage.rs`, `apps/web/src/lounge/events/*` |
| Docs | Checklist G-75, G-89..G-93 ticked with notes; doc 04 columns, doc 05 invite object, event rules, cover, notification, CDN kind; inventory notes on LNG-CHN-011, LNG-INV-008/009 (no unchecked inventory boxes matched). | `docs/` |

Web fragment: `apps/web/src/i18n/messages/en/p13-events-1.ts`. OpenAPI and TS client regenerated (no new routes, so no route-matrix change).

## Still open / limits

- G-98 doc rows are only partly covered (invite row, event rules, `image_key`); the rest of G-98 (SYSTEM_TAG flag, `900123`, `sr_request_reason`, doc 07 2FA list, LNG-SSET-055 note) belongs to other items and was not touched.
- Voice occupancy and stage instances live in each gateway node's memory (existing design). The tick runs on one node and sees only that node's voice states, so a multi-node deployment could end an event after 15 "empty" minutes while people are connected through another node. Single-node (the default) is exact. A shared voice-state view is the real fix.
- `reap_stale` (jobs stuck `running` after a crash) is still never called in production; the tick handler avoids depending on it by re-arming first.
- Stage events started before this change (status `ACTIVE`, no `entity_id`) are not tied to a stage instance and are not ended by it.
- Starting a stage event whose channel already has a stage answers 400 on `status` (`GUILD_SCHEDULED_EVENT_STAGE_BUSY`); from a voice join the event just stays scheduled.
- Start notifications go to members marked interested (as the spec says); the creator is not added automatically.
- Table name: the spec says `scheduled_events.image_key`; the real table is `guild_scheduled_events`.

## Tests run

- `cargo test -p sr-api` (all), `-p sr-chat -p sr-storage -p sr-gateway`, `-p sr-db -- --include-ignored --test-threads=1` (own scratch DBs via `TestDb`), `-p sr-server -- --include-ignored --test-threads=1`; clippy `-D warnings` on sr-chat, sr-db, sr-storage, sr-api, sr-gateway, sr-server (all targets); `cargo fmt --check`.
- New: `sr-chat` unit tests per transition and service tests with a fake world; `sr-api --test lounge` (blocked joiner, full group, cover lifecycle, voice-join start + notification, stage open/close); `sr-gateway` event-opened stage; `sr-db --test p13_events` (two runners, one tick) and `p12_lounge_ch` (new columns); web `p13Events.test.tsx`.
- Web: `tsc`, eslint on touched dirs, prettier, full vitest (`--maxWorkers=2`; only `cinema/player/__tests__/signing.test.ts` "fails when a playlist cannot be read" fails, unrelated to this branch), `vite build` and `scripts/check-bundle-budget.mjs` (lounge 361 KB of 600).

## Decisions / owner questions

- One migration (`0820`) holds `image_key`, `entity_id`, `empty_since`; 0821 to 0824 unused.
- The web treats only `type === 1` as a group invite, so an older server that omits `type` still works.
- The Events dialog "Cover" toggle shows the picker to creators and `MANAGE_EVENTS` holders only.
- Question: should the creator of an event be notified without marking interest? (Currently no.)
