# Handoff: feat/p10-sset-be (server settings backend)

Branch `feat/p10-sset-be`, migrations 0630 to 0634. Scope: the `[BE]`/`[GW]` items of
`docs/inventory/06-lounge-server-settings.md`.

## Done

| ID | What |
|---|---|
| LNG-SSET-073 | Guild deletion is now a soft delete (`0630_guild_soft_delete.sql`, `guild_admin_repo.rs`). Messages go to `user_deleted` with `purge_after = now + 30 days` (held messages get none, D-122), pins are dropped, members, invites, webhooks and app installs are removed, `GUILD_DELETE` was already emitted. The guild row stays (`guilds.deleted_at`, hidden from `get_guild`) and the hourly purge job hard-deletes it once no message is left (`REAP_GUILDS_SQL` in `sr-server/src/jobs.rs`). Before this, `DELETE FROM guilds` cascaded and ignored moderation holds. |
| LNG-SSET-056 | `X-Audit-Log-Reason` now reaches every audit write: a task-local scoped per request (`router.rs` `audit_reason_scope`, `AuditEntry::new` falls back to it). New entries: moderator pin, unpin, message delete (only when not the author) and bulk delete (`MessagesDeps::with_audit`), thread create, update and delete (`ThreadsDeps::with_audit`), onboarding questions one entry each (163 to 165) with 167 only for real settings changes, tag, guide and applications-form changes as `GUILD_UPDATE` with key diffs. `AuditSink::record_with_options` is a new defaulted trait method (Discord `options`). |
| LNG-SSET-060 | Verified already built (SettingsEvent to DomainEvent, gateway passthrough and intents); ticked. |
| LNG-SSET-012 | Server tags and adoption (`0631_guild_tags.sql`): `GET/PUT/DELETE /guilds/{id}/tag`, `GET/PUT /users/@me/primary-guild`. `primary_guild` (`identity_guild_id`, `identity_enabled`, `tag`, `badge`, `color`) appears on message author objects and `UserObject` while the tag exists, "members can display" is on and the user is still a member (checked in the `UserLookup` query). Bots cannot use the user routes (`auth/policy.rs`). |

Backend halves of FE-tagged items (the FE boxes stay unticked, noted in the inventory):

* LNG-SSET-042 applications (`0634`): `GET/PUT /guilds/{id}/sr/applications/form`, `POST /guilds/{id}/sr/applications`, `GET .../applications/@me`, `GET .../applications` (queue, `status`, `before`, `limit`), `POST .../applications/{id}/decision`. Approving inserts the member in the same transaction and publishes `GUILD_MEMBER_ADD` (`SettingsEvent::MemberJoined`). One pending application per person; banned users and members cannot apply.
* LNG-SSET-066 guide (`0633`): `GET/PUT /guilds/{id}/sr/guide` (welcome message 300, 5 to-dos, 10 resource pages). Members read it only while `guide_enabled` is on.
* LNG-SSET-068 insights (`0632`): daily job `insights.daily` (singleton id 630, 00:20 UTC, recomputes the last 2 completed days) and `GET /guilds/{id}/sr/insights?days=` (`MANAGE_GUILD` or `VIEW_GUILD_INSIGHTS`): members at end of day, joins, messages, top channels, 90 days kept.
* LNG-SSET-065 onboarding backend already existed (`GET/PUT /guilds/{id}/onboarding`); nothing to add.

## Still open

* **LNG-SSET-026 (sound trim/transcode to Opus)**: not built. There is no soundboard store or API (VOX-SB-001), so nothing to transcode, and the only Opus encoder in the ecosystem is the `opus` crate (a binding to C libopus); there is no production pure-Rust encoder. `symphonia` decoding alone is pure Rust. This needs an owner decision (ADR for libopus, as docs/03 marks it with a warning) and the soundboard tables first. Left unticked.
* Audit actions with no feature behind them yet: soundboard (130 to 132), scheduled events (100 to 102), integrations (80 to 82), `INVITE_UPDATE`. `CHANNEL_OVERWRITE_CREATE` is logged as update because `set_overwrite` is an upsert (that code is in `routes/guilds/backend.rs`, owned by the LNG-GLD/CHN agent). Application decisions are not audited (Discord has no action for it).
* Insights `voice_minutes` is always 0: voice sessions are not persisted.
* No `USER_UPDATE` is sent when someone adopts or clears a tag; clients see it on the next fetch of the user or message. The guild member and ban `UserRef` (`routes/guilds/dto.rs`, other agent's file) does not carry `primary_guild` yet.
* Applications: no gateway event for a new application (a moderator UI would poll), and the "apply to join" access mode (LNG-SSET-039) is just `enabled` on the form; how an outsider finds the form (invite landing, Directory) is FE work.
* Web: nothing in `apps/web` was touched (no UI strings, no bundle impact). The generated api-client and MSW handlers were regenerated.

## Tests run

* `cargo fmt --check` and `cargo clippy --all-targets -D warnings` on sr-chat, sr-db, sr-api, sr-server: clean.
* `cargo test -p sr-chat -p sr-api`: all pass (route matrix included).
* `cargo test -p sr-db -p sr-server -- --include-ignored --test-threads=1` against a scratch Postgres database (dropped afterwards): all pass. New Postgres tests: `delete_guild_soft_deletes_messages_and_removes_members`, `reap_removes_deleted_guilds_only_once_their_messages_are_gone`, `server_tag_adoption_shows_only_for_members_while_allowed`, `insights_are_computed_per_day_and_read_back`, `guide_round_trips`, `applications_queue_and_approval_adds_the_member`.
* New unit/API tests: audit ambient reason (sr-chat and router), `moderator_message_actions_are_audited`, `thread_create_edit_and_delete_are_audited`, onboarding audit split, tag/guide/applications/insights endpoint tests.
* `pnpm --filter @sideroom/api-client mocks:check` and `typecheck`: pass.

## Decisions

* Tag text is 2 to 4 letters or digits (inventory says at most 4; Discord needs 2). Badge ids 0 to 15, colour 24-bit.
* Soft delete keeps the guild row for the whole window (docs/09: "Server metadata is kept for staff"); the staff guild still cannot be deleted.
* New SR-only routes live under `/sr/` like the other community routes; tags use `/guilds/{id}/tag` and `/users/@me/primary-guild` to stay near Discord.
* `sr-chat` gained a direct `tokio` dependency (`rt`, for the task-local).

## Owner questions

1. Allow the `opus` crate (libopus) for LNG-SSET-026 via an ADR, or wait for a pure-Rust encoder?
2. Should an approved application also write an audit entry (no Discord action type exists; `MEMBER_UPDATE` with an `application` key would be the closest)?
3. Should the tag also be shown on guild member lists (needs a change in `routes/guilds/dto.rs`)?
