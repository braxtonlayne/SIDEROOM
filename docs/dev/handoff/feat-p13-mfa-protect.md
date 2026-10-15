# Handoff: feat/p13-mfa-protect (G-94, G-95, G-97, G-98 in part)

Branch `feat/p13-mfa-protect`, based on `integration/phase12`. Migration used: `0825_dm_request_reason.sql`.

## Done

- **G-94 server 2FA scope (D-223).** `sr-core` error `SrMfaEnrollmentRequired = 900123` (403). `MfaGate` (`routes/guilds/safety.rs`) gained `check_level(user, mfa_level)`, `applies()` and the bot rule: a bot passes when the application's owner has a TOTP or passkey, else `900123`; a session with no second factor gets `900123` (it used to get `900004`), one with a stale factor `900004`. Gated now, besides the routes that already had it: member role and timeout edits (`PATCH /guilds/{id}/members/{id}` with `roles` or `communication_disabled_until`), deleting another member's message, bulk delete, pin and unpin (`MessagesDeps::require_mfa`), thread edit and delete by someone other than the creator, webhook create, edit, reset-token and delete, emoji, sticker and soundboard create, edit and delete, AutoMod rules, `PATCH sr/community`, onboarding, welcome screen, Server Guide, tag, templates, feature modules, membership applications and invite deletion. In `settings/*` the gate runs only when the caller holds the permission (`GuildAccess::require_mfa`), so a member without it still gets `50013`. `POST /guilds/{id}/mfa` checks the owner's enrolment only for the owner (other callers now get the 403 instead of a 400 once an `AuthService` is registered).
- **G-95 Protection settings (D-224).**
  - Routing: `dm_access_protected` in `sr-chat/src/social/dm.rs`; `SocialRepository::shared_protection` (Postgres: one query over the shared servers, "every" = all shared servers have `dm_spam_filter`), `new_account` wins over `server_filter`; friends never affected, settings never open a closed door. `OpenedDm` carries the reason; stored as `channel_recipients.request_reason` (migration 0825). `sr_request_reason` is on the recipient's DM object in `GET /users/@me/channels` and READY (`DmChannelObject`), next to `is_message_request`.
  - Spoilers: `SocialService::spoiler_attachments`; `POST /channels/{id}/attachments` forces the spoiler flag (`SPOILER_` name) for a sender in a 1:1 conversation pending in the other person's requests when a shared server has `explicit_media_filter_dms` on. No scanner.
  - Links: `Guild.hide_suspicious_links` (read from `guild_community` in `GUILD_COLS`) is on the guild object for every member (GET, GUILD_CREATE, GUILD_UPDATE). Web: `lounge/suspiciousLinks.ts` shows a masked link whose text names another host as its raw target plus a warning, in `MessageItem`.
  - Web request card shows "This account is N days old." or the server-filter sentence (`lounge/shell/requestReason.ts`).
- **G-97** `withReauth` opens an enrolment modal on `900123` (title "This server requires two-factor authentication", button to Settings, My Account) and rethrows; `900004` keeps the re-auth dialog. Message delete, pin, unpin and thread edit and delete now use `withReauth`.
- **G-98 (partly)** D-223 and D-224 rows in docs 04, 05, 07 and inventory LNG-SSET-052, 054 and 055. See open items.
- OpenAPI, api-client and MSW handlers regenerated (new fields only). Route matrix unchanged (no new routes).
- i18n fragment `p13-mfa-protect-1.ts`.

## Still open

- **G-98 rows for D-221/D-222**: guild-invite `type`/`guild` wording, scheduled-event status rules and `image`, `scheduled_events.image_key`. They describe G-90 to G-93, which other branches build; adding them here would conflict with those docs edits. `SYSTEM_TAG` is already in doc 05.
- Doc 07 "section 4" in the checklist is the Staff Server; the server 2FA list went into section 2 (guild permission bits) as its own sub-heading.
- No automated test for the thread gate (the thread router is not mounted in the `lounge` or `developer` suites); the code path is `thread_gate` in `guilds/channels.rs`.
- The sender's CHANNEL_CREATE does not carry `sr_request_reason`; the recipient only learns of a pending request through the list and READY.
- `lounge/liveMap.ts` `snowflakeTime` uses Discord's epoch (1420070400000) while the server uses `SIDEROOM_EPOCH_MS` (1767225600000); `requestReason.ts` uses the server's. Looks like an existing bug in `snowflakeTime` (message times from snowflakes would be off); not touched.

## Tests run

- `cargo test -p sr-api` (all, incl. `--test lounge` with the new gate test, `developer::mfa_tests`, social `message_request_carries_the_server_protection_reason`), `cargo test -p sr-chat --lib social`, Postgres (scratch DB, `--include-ignored --test-threads=1`): `sr-db --test p13_mfa_protect`, `p12_lounge_ch`, `social_repo`, `chat_repo`, `settings_repo`.
- `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on sr-core, sr-chat, sr-db, sr-api, sr-server.
- Web: typecheck, lint, prettier --check, vitest (`src/lounge src/store src/settings src/moderation src/admin src/i18n`, 1000+ tests), build and bundle budget.

## Decisions and owner questions

- 900123 for a *session* without any second factor replaces the old `900004` there (D-223 text).
- Gating runs before the permission check on the older guild routes (kick, ban, ...), so a member without the permission and without a second factor sees `900123` there; the `settings/*` routes check the permission first. Say if you want the older routes aligned.
- Member nickname edits (`MANAGE_NICKNAMES`) are not gated (not in the D-223 list); role assignment and timeouts are.
