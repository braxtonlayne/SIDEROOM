# Handoff: feat/p2-settings-gaps

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p2-settings-gaps, branch feat/p2-settings-gaps (base integration/phase2). Local commits only, nothing pushed.

Backend gaps the web agents hit while building the User Settings and Server Settings screens.

## Done

| # | Gap | Endpoint(s) | Notes |
|---|---|---|---|
| 1 | Change password | `POST /sr/auth/password` | Re-auth with the current password (shares the login lockout), strength rules with the account's username/email as context, `revoke_other_sessions` (default true) also revokes trusted devices, notice mail, platform audit row `account.password_change` |
| 2 | Display name | `PATCH /users/@me` with `global_name` | Profile field, so it gets the normal `USER_UPDATE` fan-out. 32 chars, no control characters, `null` clears |
| 2 | Username | `PUT /sr/auth/username` | Password re-auth, signup rules, 2 changes per day, `USER_UPDATE` fan-out, audit row |
| 2 | Email | `POST /sr/auth/email/change` then the existing `POST /sr/auth/verify-email` | Link goes to the NEW address and uses the existing `/verify-email/{token}` web page. The token row (`change_email` enum value, existing since 0001) remembers the pending address. On confirm: swap, mark verified, notice to the old address, audit row. No migration needed |
| 3 | Settings blob | `GET/PATCH /users/@me/settings` | JSON merge patch (RFC 7386), 64 KiB / 8 levels / 256 top-level keys, version column with optimistic concurrency, `SR_USER_SETTINGS_UPDATE` dispatch (not `USER_SETTINGS_UPDATE`, which already carries presence) |
| 4 | Global default notification level | `GET/PATCH /users/@me/notification-defaults` | `message_notifications` 0/1/2/null. Used by `resolve_notifications` between the per-server choice and the server default; servers only |
| 5 | DM / friend privacy | `GET/PATCH /users/@me/privacy` | Exposes the existing `user_privacy` rules (friend-request sources, DMs from server members, Message Requests). `SR_PRIVACY_UPDATE` dispatch |
| 6 | Member search, role filter, role members | `GET /guilds/{id}/members?query=&role_id=`, `GET /guilds/{id}/members/search`, `GET /guilds/{id}/roles/{role_id}/members` | Case-insensitive substring over nick, username, display name (LIKE metacharacters escaped), `@everyone` matches all |
| 7 | Prune | `GET /guilds/{id}/prune?days=&include_roles=` (count), `POST /guilds/{id}/prune` | `KICK_MEMBERS`, hierarchy respected, owner exempt, 1-30 days, at most 10,000 per call, `GUILD_MEMBER_REMOVE` per member, one `MEMBER_PRUNE` audit entry (not written for a no-op) |
| 8 | Webhook URL | skipped as asked; `POST /webhooks/{id}/reset-token` instead | Tokens are hash-only by design (Q137). Reading them back needs reversible storage, which weakens that. Reset issues a new URL and kills the old one; audit entry has no value. Owner decision 25 |
| 9 | AFK | `afk_channel_id`, `afk_timeout` on the guild object and `PATCH /guilds/{id}` | Columns existed since 0010 (FK `ON DELETE SET NULL`). Voice channel of the guild only; 60/300/900/1800/3600 |
| 10 | Guild icon upload | already implemented | `PATCH /guilds/{id}` with `icon`/`banner`/`splash` data URIs goes through `MediaDeps` and is served at `/cdn/icons/{guild}/{hash}`; test `guild_icon_banner_splash_upload_replace_remove` exists. The web handoff note was out of date. Nothing added |

Migration: `0140_user_settings.sql` (only one needed; range 0140-0159 otherwise unused).

Docs: `docs/05-api-and-gateway.md` (section 4 bullets and the `/sr/auth` table), `docs/inventory` (LNG-USET-002 ticked, LNG-GLD-005 annotated), `docs/open-questions.md` items 25-28. OpenAPI and the generated client regenerated (`cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, then `pnpm --filter @sideroom/api-client generate`). The three new `/sr/auth` routes are documented with utoipa (the other `/sr/auth` routes still are not).

## Tests added

- sr-api auth: 6 (password x3, username, email x2); sr-api social: 4; sr-api guilds (tests/guilds.rs): 3; sr-api settings: 1; openapi path check extended.
- sr-chat: social 3 + 4 unit tests of the blob rules, admin rules 2, admin service 2.
- Postgres-backed (ignored by default): auth_repo 1, social_repo 1, guild_admin_repo 2, settings_repo 1.
- Last run: whole workspace `cargo test` green (sr-api lib 259, sr-chat lib 226, sr-api guilds 11, and the rest), and all 59 ignored Postgres tests green on a fresh database. One older test (`admin_repo::suspend_revokes_sessions_and_reactivation_respects_the_cap`) fails on a database that already holds many accounts; it needs a clean database.
- Run them: create a scratch database, then `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db -- --ignored --test-threads=1`.

## Not done / for the web agents

- No FE work (out of scope). Inventory FE items (LNG-USET-004/012-014, LNG-SSET-015/029) stay unticked.
- Web needs: a `/verify-email/{token}` page that also handles the email-change case (same endpoint, so the existing page works; the copy "Welcome in" is signup-specific), `global_name` in the profile form, the new endpoints from `packages/api-client`.
- Guild templates do not snapshot AFK settings.
- Link/unlink Discord for a signed-in user (listed by the user-settings handoff) was not in this task and is still missing.
- Username/email changes are not reflected in the search index or any cached summaries beyond what `USER_UPDATE` carries; check sr-search if usernames are indexed.
- Intermediate commit f3306a4 does not compile on its own (the username route calls a helper added in the next commit); the branch tip is fine.

## Owner decisions

See `docs/open-questions.md` items 25-28: webhook URL retrieval, notification-default precedence, prune definition of inactive (messages only), email change without a second factor.

## Resume

```
cd C:/Users/braxt/Desktop/SIDEROOM_wt/p2-settings-gaps
export CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p2-settings-gaps-target CARGO_PROFILE_DEV_DEBUG=0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo deny check
```
