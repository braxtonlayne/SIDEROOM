# Handoff: feat/p13-settings-admin

Branch `feat/p13-settings-admin`, base `integration/phase13`. Backend contracts the web agents asked for, the remaining
backend half of the server-settings, admin and server inventories, and one web bug (snowflake epoch). Migrations used:
0840 to 0844. The contracts are written into `docs/05-api-and-gateway.md` section 5.3.

## Part A: contracts for the web agents

| Asked for | Built |
| --- | --- |
| User safety settings (LNG-USET-012/013) | `sensitive_media {friends, others, servers}` (show/blur/block), `dm_spam_filter`, `server_dm_overrides` on `GET/PATCH /users/@me/privacy` (migration 0840). The per-server override is enforced when a DM is opened. The media filter is a client display preference; the spam filter is stored, no detector reads it yet. |
| Age confirmation (LNG-USET-015) | `GET/DELETE /users/@me/age-confirmation` (`{confirmed, confirmed_at, method}`); reset publishes `SR_USER_UPDATE`. |
| Burst reactions (LNG-EXP-012) | `?type=1` on add, remove (own and other's) and list; `count_details`, `me_burst`, `burst_colors` on the reaction object; `burst` and `type` on `MESSAGE_REACTION_ADD/REMOVE` (migration 0841: `burst` joins the primary key so a member can hold both kinds). Cap: 3 bursts per member per message. |
| Emoji popout (LNG-EXP-011) | `GET /emojis/{emoji_id}`: name, animated, signed URL, and the owning server only when it is Directory-listed (`joinable`) or the caller is a member. |
| Entrance copy (ENT-UI-010/025) | `GET /sr/entrance/copy` (public) and `PUT /admin/entrance/copy/{key}` (`MANAGE_FEATURES`, audited with key and length), migration 0843. Keys: `no-invitation`, `house-notes`, `landing-headline`, `landing-subhead`, `login-notice`. |
| OpenAPI name clash | The connections body is now `ConnectionLinkBody` (it was `LinkBody`, which resolved to the Cinema staff schema). |
| Keyword highlights | `GET/PUT /users/@me/notification-keywords` (migration 0842). The push notifier now feeds them to the decision engine, which had `keyword_hit` hard-coded to false. |

Decisions taken (conservative defaults, please confirm):
- Safety settings ride on the privacy object instead of a new route, as the web agent asked.
- Defaults: friends `show`, others `blur`, servers `show`.
- Burst cap of 3 per member per message for everyone; no Patron allowance yet.
- Reaction roles and the starboard ignore burst reactions.
- Keywords: at most 50 per user, 100 characters each, whole-word match (the existing `matches_keyword`).

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

## Still open

See the final lines of this file, filled in at the end of the run.
