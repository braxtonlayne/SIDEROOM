# Handoff: feat/p13-h-b (H-11, H-12, H-13, H-33 to H-41)

Branch `feat/p13-h-b`, base `integration/phase13`. Follow-ups of `docs/dev/answers-followups-p13.md` (D-225 to D-240). All twelve items are ticked there and in the inventory.

## Done

- **H-11** permission before the 2FA gate: `MfaGate::require_perm`, `require_owner` and `require_channel_perm` (`routes/guilds/safety.rs`) check the permission first and answer `50013`; kick, ban, unban, bulk ban, prune, role, channel, overwrite, server edit and delete use them (member edits split into `MANAGE_ROLES` and `MODERATE_MEMBERS`). Tests in `sr-api --test lounge`: no permission gives 50013 with and without a factor, permission gives 900123 then 900004.
- **H-12** recipient `CHANNEL_CREATE` carries `is_message_request` and `sr_request_reason`; the sender's objects carry neither; no Web Push for a conversation in the recipient's requests (`push_notify.rs`). READY and the web request list already did the rest (`applyDmEvent`).
- **H-13** the lounge test world mounts the thread router and `ThreadsDeps`; the gate is tested for a non-creator moderator (900004, then OK with a proved factor), a member without the permission (50013) and the creator (not gated). `thread_gate` now also checks `MANAGE_THREADS` first.
- **H-33** `sr_chat::sensitive` (`apply`, `apply_to_message`, `setting_for`, trait `Attachment`): the two D-230 signals, blur as the `SPOILER_` name, block as empty `url` plus `sr_filtered: true`, author and bots exempt. REST: `MessagesDeps::apply_sensitive` in `render_many` (list, get, search, pins, inbox) fed by `ViewerSafety` (implemented by `SocialDeps`). Gateway: `GuildDirectory::sensitive_view` plus a per-session cache (cleared on `SR_USER_UPDATE` and `RELATIONSHIP_*`), applied in `deliver_filtered` to `MESSAGE_CREATE`/`MESSAGE_UPDATE` (guild and DM paths). Web: `FilteredChip`. Doc 05 note replaced.
- **H-34** `UploadEntitlements::max_upload_bytes(user, guild)`; `sr-server/src/upload_limits.rs` adapts `Entitlements::limits(user, guild).upload_bytes` (it was a fixed 30 MiB before, so the Patron perk is now live too); slots, finalize, `GET /guilds/{id}` and the DM list pass the guild; web store refetches the guild(s) and DMs on `SR_ENTITLEMENT_UPDATE` of either shape. Doc 05 rows updated.
- **H-35** `PlatformPermissions::MANAGE_ENTRANCE = 1 << 24` (no collision; platform bits are separate from the guild bits), migration `0875_manage_entrance_bit.sql` (Senior Staff template and role), entrance copy routes, admin section and `permissions.ts` gate on it, doc 07 row. The wasm package only holds guild-level bits, so it needed no change (I rebuilt it anyway for the web gates).
- **H-36** `StaffNotifier::system_dm` (default no-op, `StaffVoice` delivers), `AdminService::system_dm` skips non-active accounts; staff transfer DMs the previous owner and the new owner, delete DMs the previous owner, with a standing "contact staff" line and never the reason.
- **H-37** migration `0874_guild_member_join_source.sql`; every join path sets the source (creator, invite, vanity, directory, application, staff appointment and Staff Server transfer, app install); `sr_join` and `sr_signals` on member list, search and get for `MANAGE_GUILD`; `?signals=true`; web columns "Joined via", "Signals" and the "Unusual activity only" filter. Postgres test `invite_join::every_join_path_records_its_source`.
- **H-38** upload slots are allowed on forum and media channels; `POST /channels/{forum}/threads` accepts `message.attachments` (own slots of that forum, `ATTACH_FILES`, foreign slot 50035), attaches them to the starter message; New Post form has the tray.
- **H-39** registry settings `limits.rate.ip|session|bot|messages|uploads` (floors and ceilings), `RateConfig` behind a `watch` channel in `RateLimitConfig::live`, `PATCH /admin/settings` updates it, `sr-server` seeds it from the stored values. Defaults reproduce the old behaviour (50/s per identity, 5 messages per 5 s, 40 uploads per 10 s).
- **H-40** `GET /admin/directory/queue/{guild_id}/preview`; the Review hub Directory tab is now a real queue (status tabs, Preview dialog with banner and no join, approve/reject/take down with note, feature). ADM-COM-001 ticked.
- **H-41** `PUT|DELETE /admin/entrance/hero`, public `GET /sr/entrance/hero` (ETag, max-age 3600, 404), copy key `hero-alt` (200 characters), admin `EntranceHero` block, landing door slot (no `t()` keys on the landing path).

OpenAPI, TS schema, MSW mocks and `route_matrix.tsv` regenerated (new rows use `staff` and `none`, no `other`).

## Decisions and caveats

- `join_ref` is the inviter (invite), application id (application) or installing application (bot); `inviter_id` is shown for invites only. `warn_new_account_days = 0` (the stored default, meaning off for DM requests) reads as 7 for the signal.
- `rejoined` reads audit actions kick, prune and unban for the person (there is no separate leave audit row).
- The `signals=true` list reads at most 5 pages of 200 to fill the limit.
- Forwarded-message snapshots are not run through the sensitive-media filter (they keep the original attachment objects).
- Attachments are re-parented to the starter message one row at a time after the send (not in one transaction); a failed send removes the thread and leaves the slots reusable.
- MANAGE_FEATURES-only staff lose the entrance editor; only Senior Staff gets MANAGE_ENTRANCE by migration (other roles need it granted).
- `limits.rate.*` change counters of the rebuilt limiter (a rare edit).

## Not done / open

- LNG-SSET-027 join-date filter (ticked per the follow-up, still not built).
- H-37 does not set a source for the dev CLI seed members or the system user in the Staff Server (null reads as unknown).
- No Playwright (shared ports).

## Tests run

fmt --check and clippy `-D warnings` (all targets) on sr-core, sr-chat, sr-admin, sr-db, sr-gateway, sr-api, sr-server; `cargo test`: sr-core, sr-chat, sr-admin, sr-gateway, sr-api (incl. route matrix), sr-server lib with `--include-ignored`, sr-db whole suite with `--include-ignored` on a scratch database (dropped afterwards). Web: typecheck, lint, prettier on changed files, full vitest (`--maxWorkers=2`), build and bundle budget.

## Owner questions

None blocking. Worth confirming: Senior Staff only for MANAGE_ENTRANCE by migration, and the `0 reads as 7` rule for the new-account signal.
