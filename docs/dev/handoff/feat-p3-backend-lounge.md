# Handoff: feat/p3-backend-lounge

Branch `feat/p3-backend-lounge` (based on `integration/phase2`). Backend follow-ups from [answers-followups.md](../answers-followups.md): F-02, F-03, F-10, F-11, F-12, F-13, F-15, F-30. Migrations used: `0160`, `0161` (range 0160-0179).

## Done

| Item | What changed | Where |
|---|---|---|
| F-02 (D-166) last-seen | `accounts.last_seen_at` (migration 0160). The gateway marks a user seen on IDENTIFY/RESUME and again on a heartbeat at most every 10 minutes per session (`Gateway::drain_seen`); `sr-server` flushes the batch every 30 s with one `UPDATE ... WHERE id = ANY($1)` (`sr_db::accounts::touch_last_seen`). A login (session insert) stamps it in the same statement. `PruneCriteria.seen_before_ms`; `prune_candidates` skips members seen since the cutoff (never-seen counts as inactive). The column is not in any user object. | `sr-gateway/src/{hub,conn}.rs`, `sr-server/src/app.rs`, `sr-db/src/{accounts,auth_repo,guild_admin_repo}.rs`, `sr-chat/src/{admin_repo,admin_rules}.rs` |
| F-03 (D-167) email change | `mfa: {method, code}` on `POST /sr/auth/email/change` when TOTP is on (TOTP or recovery code; passkeys plug into `require_second_factor` / `check_second_factor` later). A check on the same session in the last 15 minutes counts (`sessions.mfa_verified_at`, migration 0161; set by login 2FA and by the re-prompt). New token purpose `cancel_email_change`, `POST /sr/auth/email/cancel {token}` (cancels a pending change, else revokes all sessions and trusted devices), `mail::change_email_notice` (on request) and `mail::email_changed` (on completion), both to the old address with a "This wasn't me" link `/email-change/cancel?token=`. Login second step now uses the shared `check_second_factor`. Doc 05 §5.1 and doc 06 §4 updated. | `sr-api/src/routes/auth/{identity,login,mail,common,memory}.rs`, `sr-auth/src/{email_token,store}.rs`, `sr-db/src/auth_store.rs` |
| F-10 (D-177) | `spoiler: true` prefixes the stored filename with `SPOILER_` once; a `SPOILER_` name counts as a spoiler without the flag. | `sr-api/src/routes/messages/attachments.rs` |
| F-11 (D-180) | Eight built-in templates as JSON in `sr-chat/src/settings/builtin_templates/` (same `serialized_source_guild` shape as a user template). `GET /guilds/templates/builtin`, `POST /guilds/templates/{code}` `{name, icon?}`, `POST /guilds {name, template}` (built-in id or user template code). `guild_rules::plan_guild_from_snapshot` + `GuildBlueprint.extra_roles/overwrites`; `insert_guild` writes roles, channels and overwrites in one transaction; one `GUILD_CREATE`; user templates count a use. | `sr-chat/src/{guild_rules,service,settings/templates}.rs`, `sr-db/src/chat_repo.rs`, `sr-api/src/routes/{guilds,settings}/` |
| F-12 (D-181) | READY `private_channels` are built with `DmChannelObject` (full `recipients` user objects, viewer excluded); `recipient_ids` removed. DM `CHANNEL_CREATE` already carried `recipients`. No compatibility shim: nothing in `apps/web` reads `recipient_ids`. | `sr-server/src/gateway_pg.rs` |
| F-13 (D-158) | `moderation.audit_retention_days` was already declared in the Admin platform-settings catalog (`sr-admin/src/settings.rs`, group `retention`, default 45, 0 = forever). Added a test that the key the purge job reads is that catalog entry. The `sr-features` registry holds feature modules, not platform limits, so nothing was added there. | `sr-server/src/jobs.rs` |
| F-15 (D-162) | `create_staff_guild` already instantiated the roles; the step is now `ensure_staff_roles` (match by name, `ON CONFLICT DO NOTHING` for Owner role and appointment) and exposed as `sr-cli staff ensure-roles`. `bootstrap` itself still refuses to run on an instance that has accounts. | `sr-cli/src/{guilds,commands,main}.rs` |
| F-30 | `sr_gateway_dispatch_bytes` histogram (label `compression`), uncompressed frame size of every dispatch, buckets up to 1 MiB with a 64 KiB bound. | `sr-gateway/src/conn.rs`, `sr-api/src/observability.rs` |

Docs: doc 05 (§3 `SPOILER_`, §4 templates, §5.1 email routes, §6.4 READY and the metric), doc 06 (§4 email change and last-seen, §8 bootstrap), inventory LNG-GLD-003 and FND-CLI-001 ticked, LNG-GLD-005 caption, `answers-followups.md` ticked.

## Not done / needs a paired web change

- `apps/web` still creates template servers with sequential POSTs and still calls `GET /users/@me/channels` at boot (`shell/social.ts::loadDms`, which also feeds Message Requests). Both keep working. To finish F-11/F-12 on the web side: list `GET /guilds/templates/builtin` in the wizard, call `POST /guilds {name, template}`, delete `shell/serverTemplates.ts`; and drop the READY-time `loadDms` fetch for ordinary DMs (READY has no `is_message_request`, so keep it for the Message Requests page).
- F-24 (web email-change flow, `/email-change/cancel?token=` page) depends on F-03 and is not started.
- READY `private_channels` do not carry `is_message_request`.
- Passkeys: `check_second_factor` and `require_second_factor` only know TOTP and recovery codes.

## How to resume

```
cd C:\Users\braxt\Desktop\SIDEROOM_wt\p3-backend-lounge
export CARGO_TARGET_DIR=...\p3-backend-lounge-target CARGO_PROFILE_DEV_DEBUG=0
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db -p sr-cli -- --ignored --test-threads=1
cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json
npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/api-client generate
```

Ignored Postgres tests added: `sr-db/tests/guild_admin_repo.rs::prune_spares_members_seen_recently`, the session assertion in `sr-db/tests/auth_repo.rs::sessions_roundtrip_and_revoke`, `sr-cli` `guilds::tests::staff_roles_are_created_once_and_repaired_on_rerun`, and a template-from-snapshot test in `sr-db/tests/chat_repo.rs`.

## Owner decisions / notes

- F-13: confirm that the Admin settings catalog (not `sr-features`) is the intended "registry" for Admin -> Limits.
- Cancel-link semantics: with nothing pending, the link revokes all sessions (also when the pending change had merely expired).
- Built-in templates keep the default layout (Text Channels, Voice Channels, `#general`, `General`) plus one extra category, exactly what the client produced before; none of them defines roles.
