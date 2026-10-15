# Handoff: feat/p12-lounge-ch-be

Branch `feat/p12-lounge-ch-be` (base `integration/phase12`). Migrations used: 0791, 0794, 0795, 0796, 0800.

## Done

| Item | What | Where |
|---|---|---|
| G-29 | `directory.hidden_categories` platform setting (list of category ids). Hidden categories leave `GET /directory/categories` and search, listings in them 404, new listings in them 400. | `sr-features/src/directory.rs`, `routes/directory.rs` (`DirectoryDeps::with_features`) |
| G-37 | `0791_backfill_free_bits.sql`: `@everyone` gains `USE_EXTERNAL_SOUNDS` and `START_WATCH_PARTY` on existing servers. | `sr-db/tests/p12_lounge_ch.rs` |
| G-36 (a) | `POST /guilds/{id}/mfa {level}` (owner only, needs the owner to have a second factor). `MfaGate` on kick, ban, unban, bulk ban, prune, role, channel, overwrite, server edit and delete routes: 403 `900004` until the session proved a second factor (`GET|POST /sr/auth/reauth`, same 15-minute window as `/admin/reauth`; bots exempt). Web: the existing app-root re-auth dialog now also works for non-staff (`admin/api.ts` falls back to `/sr/auth/reauth`) and `settings/server/api.ts` `send` retries through `withReauth`. | `routes/guilds/safety.rs` |
| G-36 (b) | `dm_spam_filter`, `explicit_media_filter_dms`, `hide_suspicious_links`, `warn_new_account_days` on `guild_community` (0795), PATCH fields, audit changes. Stored only: nothing filters DMs yet. | `routes/settings/community.rs`, Moderation → Protection |
| G-36 (c) | `system_channel_flags` (PATCH `/guilds/{id}`; bit 1 stops the join notice), `widget_enabled`, `widget_channel_id`, `GET|PATCH /guilds/{id}/widget`. Guild edits write `changes` for name, levels, flags, system channel, mfa and widget. | `routes/guilds/*`, Overview |
| G-44 | `0794_sticker_packs.sql`, `GET /sticker-packs`, `GET /stickers/{id}/url`. Pack stickers are standard stickers (guild null), which message create already accepted. No packs ship. | `routes/messages/sticker_packs.rs` |
| G-47 | Group DM invites on the existing invite routes (`0796`): 24 h, unlimited uses, owner makes and revokes, join adds the recipient with `CHANNEL_RECIPIENT_ADD`, `CHANNEL_UPDATE`, `CHANNEL_CREATE`. `InviteObject.guild` is now optional, `type` 1 for groups. Web: `/invite/{code}` page, chat invite card, Join a Server. | `routes/social/group_invites.rs` |
| G-48 | `sr_chat::profile::catalog`, `GET /sr/profile-catalog`, `PATCH /users/@me` validates `profile_effect` and `avatar_decoration` slugs. Effects `sparkle aurora snow embers`; no decorations. Web: an effect picker in User Settings → Profile. | `sr-chat/src/profile.rs` |
| G-50 | `MediaKind::ChannelIcon`, CDN kind `channel-icons`, `PATCH /users/@me/channels/{id}` stores a data URI icon (the old object is removed). `GROUP_ICON_UPLOAD` is `true`. | `routes/social/dms.rs` |
| (a) Scheduled events | `0800_scheduled_events.sql`; `GET|POST /guilds/{id}/scheduled-events`, `GET|PATCH|DELETE .../{id}`, `GET .../{id}/users`, `PUT|DELETE .../{id}/users/@me`; gateway `GUILD_SCHEDULED_EVENT_CREATE/UPDATE/DELETE/USER_ADD/USER_REMOVE` (intent `1 << 16`); status start/end/cancel by PATCH and by a 30 s clock task (`scheduled_events::advance`); audit actions 100 to 102; error `10070`. Web: Events row in the channel list (LNG-CHN-011), Events dialog (Interested, Start, End, Cancel, Delete), Create Event (server menu, rail menu). | `sr-chat/src/settings/scheduled_events.rs`, `routes/settings/scheduled_events.rs`, `web/src/lounge/events/` |
| (b) Voice events + JTC | `DomainEvent::VoiceJoined/VoiceLeft` published by the gateway (fresh join, move = leave + join, disconnect, lost session; not DM calls); `FeatureActions` gains `voice_occupants`, `move_member`, `create_voice_channel`, `delete_channel` (production: `GatewayVoice` over `Gateway::moderate_voice`); module `lounge.join_to_create` (FTR-JTC-001). Owner controls = a Manage Channels / Move Members overwrite on the room for its creator. The generic Server Settings → Features form renders its settings. | `sr-features/src/modules/join_to_create.rs`, `sr-gateway/src/voice.rs`, `sr-server/src/feature_actions.rs` |

Ticked: G-29, G-36, G-37, G-44, G-47, G-48, G-50 in `answers-followups-p12.md`; inventory LNG-CHN-011, LNG-INV-008, LNG-INV-009 (stale), LNG-EXP-001, LNG-SSET-013, 016, 052, 054, 055, FTR-JTC-001.

## Not done / open

- LNG-EXP-004 default sticker packs: waits for owner artwork (G-44 is plumbing only).
- Avatar decorations: none exist (G-48 default).
- LNG-SSET-052/054 do not filter anything: the settings are stored and audited; no DM spam filter or media scanner reads them.
- Scheduled events: no cover image, no recurrence, no `entity_id` for stage instances (a stage event is not linked to a live stage instance), no notification to interested members when an event starts. The clock task runs on every node, so a multi-node deployment may announce a start twice (the update itself is idempotent).
- `mfa_level` is not checked on message-level moderator actions (bulk delete, timeouts via `PATCH members` are guarded only for role changes), webhooks, invites or automod routes.
- Join-to-create: one room per member, at most 50 rooms per server, no "lock/limit" chat commands (the owner uses the normal channel editor).

## Tests run

- `cargo test -p sr-chat -p sr-events -p sr-gateway -p sr-features`, `-p sr-api --test lounge` (15), Postgres: `sr-db --test p12_lounge_ch` (6), `sr-server` `feature_actions_pg_tests` (join-to-create against real services).
- Web: new `lounge/__tests__/p12ChBe.test.tsx`, `settings/server/__tests__/p12safety.test.tsx`; updated `live.test.tsx` and `p11ShellRail.test.tsx` for the new permission flags and the live Create Event entry.

## Decisions to confirm

- Group invites reuse `POST /channels/{id}/invites` and make `InviteObject.guild` optional (a breaking change for typed clients that assumed it).
- 2FA gate scope (list above) and that bots are exempt.
- Event auto-start: every scheduled event starts at its start time, including voice and stage events (Discord waits for someone to start them).
