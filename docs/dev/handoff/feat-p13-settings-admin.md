# Handoff: feat/p13-settings-admin

Branch `feat/p13-settings-admin`, base `integration/phase13`. Backend contracts the web agents asked for, the remaining
backend half of the server-settings, admin and server inventories, and one web bug (snowflake epoch). Migrations used:
0840, 0842, 0843 and 0844 (0841 is unused). The contracts are written into `docs/05-api-and-gateway.md` section 5.3.

## Part A: contracts for the web agents (aligned with D-230 to D-233)

| Asked for | Built |
| --- | --- |
| Safety settings (D-230, LNG-USET-012/013) | `sensitive_media {friends, others, servers}`, `dm_spam_filter` (`off`, `non_friends` default, `all`) and `server_dm_overrides` on `GET/PATCH /users/@me/privacy` (migration 0840). Enforced: the spam filter level and the per-server override when a DM is opened. Not yet: applying `sensitive_media` to attachments per viewer (spoiler flag, `sr_filtered`), which needs a viewer-aware message pipeline for REST and the gateway; and server settings tightening the filter (D-224). |
| Age confirmation (D-230, LNG-USET-015) | `GET /users/@me/age-confirmation` `{confirmed_at, method}`; `DELETE` needs `current_password` and publishes `SR_USER_UPDATE`. |
| Burst reactions (D-231) | Not built. The reaction object carries the shim (`me_burst`, `count_details`, `burst_colors`), `?type` is accepted and ignored. (An earlier burst implementation on this branch was removed, with its migration.) |
| Emoji (D-232, LNG-EXP-011) | `GET /sr/emojis/{id}` (popout, source server only for members or Directory-listed servers, with `listing`) and `GET /sr/emojis/{id}/url` for any session. Bots are allowed on `/sr/emojis/`. |
| Upload limit (D-233) | `sr_max_upload_bytes` on `GET /guilds/{id}` and on the DM channels of `GET /users/@me/channels`. Not yet on `GUILD_CREATE`/`GUILD_UPDATE` (broadcast payloads are not per viewer) and the "server entitlement level" has no source in the code, so the value is the member's perk (same function as the 413 check). |
| Entrance copy (ENT-UI-010/025, H-09) | `GET /sr/entrance/copy` (public, ETag, max-age 60), `PUT|DELETE /admin/entrance/copy/{key}` (`MANAGE_FEATURES`, fresh second factor, 16 KiB, keys `no-invitation`, `house-notes`, `landing-note`), migration 0843. Stored in an `entrance_copy` table rather than `platform_settings`; the server does not parse the markdown (no entrance profile in `sr-markdown` yet). |
| OpenAPI name clash (H-10) | The connections body is now `ConnectionLinkBody` (a rename of the Rust struct instead of `schema(as)`). The extra OpenAPI check that every schema name maps to one Rust type is not added. |
| Keyword highlights | `GET/PUT /users/@me/notification-keywords` (migration 0842); the push notifier feeds the hits to the decision engine (it had `keyword_hit` hard-coded to false). |

Other H items in my area from `answers-followups-p13.md`: H-05, H-06, H-07, H-08, H-09, H-10 are covered as above (H-05 and H-06
partly, see the notes). H-11 to H-13 (2FA gate order, request dispatch, thread gate test) build on `feat/p13-mfa-protect` and are not in
this base; H-14 to H-27 are other areas.

## Part B: backend half of the settings, admin and server inventories

Every box left unchecked in 06, 10 and 04 was `[FE]` except one: LNG-INV-006 was stale (built, ticked with the file names).
LNG-CHN-024 was stale too (the guild object carries `public_updates_channel_id`; the header badge exists). The new
backend work, each with a test:

| Item | What |
| --- | --- |
| ADM-PPL-001 | `GET /admin/accounts?discord_id=` and `?ip=` (the IP lookup is validated and written to the access log). |
| ADM-PPL-002 | `GET /admin/accounts/{id}/sessions` (no token material, logged) and `/guilds`. |
| ADM-PPL-004 | `PUT /admin/persons/{id}/notes`, `POST /admin/persons/{id}/link`, `POST /admin/accounts/{id}/unlink`, `GET /admin/persons/{id}/suspicion-log`. Link and unlink refuse while a Person is banned (409). |
| ADM-GRAPH-004 | `POST /admin/accounts/{id}/freeze-invites` and `unfreeze-invites` (migration 0844, enforced in the invite-creation rules with the same generic wording); `invites_frozen` on the account drawer. |
| ADM-COM-002 | `GET /admin/guilds`, `GET /admin/guilds/{id}`, `POST /admin/guilds/{id}/transfer-ownership`, `DELETE /admin/guilds/{id}`, all `MANAGE_GUILDS_GLOBAL` (the bit existed, nothing used it) and reason required. Changes go through the chat stack acting as the owner, so members hear the usual events. |
| LNG-SSET-009 | `GET /guilds/{id}?with_counts=true` adds `approximate_member_count` and `approximate_presence_count`. |
| LNG-SSET-030 | `GET /guilds/{id}/audit-logs?target_id=` for the member drawer's history. |

## Part C: snowflake epoch

`snowflakeTime` in `apps/web/src/lounge/liveMap.ts` used Discord's 2015 epoch; it now uses `SIDEROOM_EPOCH_MS`
(1_767_225_600_000, `sr-core/src/snowflake.rs`), exported as `SIDEROOM_EPOCH_MS`. It was the only client-side
snowflake-to-time conversion (all other call sites go through it). Tests: `lounge/__tests__/snowflakeTime.test.ts` and the
fixed expectation in `live.test.tsx`. The web-fe branch's Welcome dialog uses the platform epoch separately; it should import this
constant when the branches merge.

Also built: `GET /admin/system` lists `integrations` and `POST /admin/system/email-test` mails the caller (ADM-DASH-003);
`submissions.max_open_per_member` is a platform setting (ADM-PLT-002); the web Overview server card shows the online and member
counts through `?with_counts=true` (LNG-SSET-009, `settings/server/sections/Overview.tsx`, test in `overview.test.tsx`).

## Still open, with reasons

- Web wiring of everything above except the Overview card: the USET-012/013/015 panels, the emoji popout Join,
  keyword list, Admin Entrance editor, the account and Person drawers (sessions, servers, notes, link/unlink, freeze), the Servers
  page (ADM-COM-002), the Setup checklist (ADM-DASH-003) and the member drawer's audit history (LNG-SSET-030). The routes, OpenAPI
  schema, generated client and MSW handlers are in place; these boxes stay unticked because they are `[FE]` items.
- LNG-SSET-027 join method and signals: nothing stores how a member joined (invite code, vanity, Directory, application) and
  "unusual activity" has no definition. Needs a join-source column on `guild_members` set by every join path, and an owner decision on signals.
- LNG-FRM-004 forum post attachments: upload slots are issued per existing channel, and the post thread does not exist before
  creation. Needs a design (upload against the parent channel, then adopt).
- ADM-PLT-002 API rate limits: still static config (`ApiSettings`), not registry settings; making them live needs a reload hook in the limiter.
- ADM-CNT-005 (ingest sources: watched folders, storage imports, naming tester) has no admin routes at all; it needs the ingest
  watcher design from `docs/specs/ingest-naming-conventions.md`, too large for this pass.
- ADM-COM-001 (Directory approvals): the queue and decisions exist; a staff preview of a pending listing server and a category
  manager (the categories are a constant, hiding is the `directory.hidden_categories` feature) are not built.
- ADM-PLT-005/006, ADM-CNT-003/004: their routes exist or belong to the content agents; not audited further here.
- Per-viewer sensitive-media handling of attachments (D-230) and `sr_max_upload_bytes` on gateway guild payloads (D-233).

## Tests run

- `cargo test -p sr-chat`, `-p sr-admin`, `-p sr-review`, `-p sr-api --lib`, `-p sr-api --test lounge` and the route matrix
  (regenerated and reviewed: all new rows are `self`, `other` or `staff`, no `?` left).
- Postgres (`cargo test -p sr-db -- --include-ignored --test-threads=1` on a scratch database, dropped afterwards): 34 suites green,
  including the new `p13_settings_admin.rs` (safety prefs, keywords, emoji lookup, audit target, entrance copy, link/unlink/
  notes/freeze, discord and IP search, sessions/servers/guild list, age confirmation reset) and the manual freeze in `member_invites.rs`.
- `cargo fmt --check`, `cargo clippy --all-targets -D warnings` on sr-chat, sr-admin, sr-db, sr-api, sr-events, sr-gateway,
  sr-features, sr-auth, sr-review, sr-server.
- Web: tsc, eslint, prettier, vitest (`--maxWorkers=2`, 160 files; `channelPerms` needs the wasm `dist` copied from the main checkout),
  `vite build` and `scripts/check-bundle-budget.mjs` (settings 260 KB of 600, admin 290 KB of 600).
- OpenAPI regenerated (`openapi.json`, generated schema, MSW handlers; `mocks:check` passes). Playwright was not run (shared ports).

## Open owner questions

- Should `sensitive_media` be applied in the REST message object only first, or REST and gateway together?
- Entrance copy permission: `MANAGE_FEATURES` today; should it be a dedicated bit?
- Staff server deletion/transfer acts as the owner and is audited; should it also DM the owner?
- Where should the server entitlement level for `sr_max_upload_bytes` come from (boost levels are not modelled)?
