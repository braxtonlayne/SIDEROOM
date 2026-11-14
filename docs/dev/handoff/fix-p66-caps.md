# Handoff: wave 66 caps (SEC-AUD-183..186; D-659, D-660, D-661, D-662)

Branch `fix/p66-caps` (worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p66-caps`), base `integration/phase66`. Nothing pushed.

Items 1 to 3 are done and verified (tests below, clippy clean on the touched crates, visual check done). Item 4 (`tv.footer_note`) is not done: it needs a new field in the Live TV guide response, which the task says to stop on. Commits: `SEC-AUD-183` (the limits, with two small follow-up commits for test fixes), `SEC-AUD-184` (client, spec for shots), `SEC-AUD-185` (settings, docs). The first 183 and 185 commits were made before the heavy lock was free, so they were not compiled when committed; the later commits fix what the first compile found (tests only) and everything on the branch head passes.

## Done

- **183 the two limits, enforced.**
  - `crates/sr-core/src/guild_caps.rs`: the constants (`GUILDS_PER_USER_SETTING`/`DEFAULT_GUILDS_PER_USER`/range, `GUILD_MEMBERS_SETTING`/`DEFAULT_GUILD_MEMBERS`/range), `limit_in_force` (an unreadable value gives the default, never zero) and `CapHit`. `ErrorCode::MaxGuildMembers = 30019`.
  - `crates/sr-admin/src/settings.rs`: the two settings in group `limits` after `accounts.cap`, with the descriptions of the task; the table `READERS` and its tests (below).
  - `crates/sr-db/src/guild_caps.rs`: `check_user_cap`, `check_guild_cap` (advisory transaction lock `guild-cap:user:<id>` / `guild-cap:guild:<id>`, setting read in the same transaction, count, compare), `repo_err`, `guild_members_limit_now` (for `max_members`).
  - Call sites: `chat_repo.rs` `insert_guild` (user cap on the owner, skipped for `is_staff_guild`), `invite_join.rs` `join_via_invite` and `join_listed` (both caps after the already-member, staff, pause and ban checks, before the insert), `settings_repo.rs` application approval (both caps, only when the member row does not exist yet; at a limit the transaction is dropped, so the application stays pending), `apps_repo.rs` bot install (server cap only, only when the bot is not a member yet).
  - Errors: `ChatError::{MaxGuilds(n), MaxGuildMembers}`, `SettingsError::{MaxGuilds(n), MaxGuildMembers, ApplicantAtLimit}`, `AppsError::GuildFull` (new variant in `sr-apps`, mapped to 30019 in `routes/developer/mod.rs`). `RepoResult` repositories (`insert_guild`, `decide_application`) return a marker `RepoError` (`sr_chat::error::cap_hit_to_repo` / `cap_hit_from_repo`) that the service turns back into the typed error. The API answers `400` `30001` `Maximum number of guilds reached (N)` and `400` `30019` `Maximum number of server members reached`; an approval at the applicant's limit answers `30001` with "This member is in the most servers one account can be in."
  - In-memory repositories: `sr_chat::limits::MemoryCaps` (limits a test sets; default the constants) in the chat memory repos (`insert_guild`), and in `MemorySettingsRepo` for `join_via_invite` and `join_listed`. The settings memory repo cannot see members, so approvals are covered by `Store::approval_cap` (a test sets the limit the next approval meets).
  - `max_members` in the guild object (`GET /guilds/{id}` only, next to `sr_max_upload_bytes`): `Option<Option<u64>>`, `null` for the Staff Server. It is read through `routes/guilds/member_limit.rs` (`MemberLimitSource`, registered in `sr-server/src/app.rs` over `sr_db::guild_caps::guild_members_limit_now`; without a source the default).
  - Deviation to know: the server count leaves out the system user (`NOT_SYSTEM_MEMBER`, D-188), as `count_members` does; the plan said "every row". Memberships of bots count (D-660). The in-memory repositories know no bot flag, so there every account counts.
- **184 the member sees why.** `apps/web/src/lounge/limitErrors.ts` (`limitErrorText`) and the fragment `apps/web/src/i18n/messages/en/p66-caps-1.ts` (`lounge.limits.maxServers`, `maxServersNoCount`, `serverFull`, `applicantAtLimit`, `serverFullBot`). Surfaces: see "Client surfaces". No pre-check, no counter.
- **185 every listed setting does something (D-662).**
  - `SizePolicy::from_settings`, `STANDARD_LIMIT_RANGE`, `LARGE_FILE_DAYS_RANGE`; `effective_limit` can no longer panic when the standard limit is above the system maximum.
  - A live `watch` channel carries the policy: started at the defaults and set from the stored settings once the admin service exists (`app.rs`), followed on every `PATCH /admin/settings` (`AdminDeps::with_upload_policy`, `routes/admin/system.rs`). `MessagesDeps::policy()` (used by the upload slot, the completion check and the strip rule) and `PaymentsUploadLimits` (the free size is the operator's standard limit; a perk above the free tier keeps its own number) follow it. `app.rs:487` (`FixedEntitlements`) and `app.rs:1279` (`PaymentsUploadLimits`) read the channel.
  - `uploads.standard_limit_bytes`: range now 1 MiB to 500 MiB. `retention.large_file_days` unchanged range (1 to 365).
  - Removed from the catalog: `messages.max_length`, `invites.credit_price_usd` (confirmed: nothing read either; the price is `[payments] invite_credit_price_cents`; `docs/06` line 85 said the old key, now fixed). Stored rows stay; the list shows only catalog keys, so an old row neither shows nor fails (test).
  - `staff_feed.rs:1161` (ops's file) still builds its CLI `MessagesDeps` with `SizePolicy::default().standard_limit`. That deps object is only used by the CLI poster for staff messages (no uploads), so I left it; if you want it to follow the setting too, the one-line change is to read `sr_admin` `setting_value_i64(UPLOADS_STANDARD_LIMIT)` there and pass `SizePolicy::from_settings(..).standard_limit`.
- Docs: `docs/specs/lounge-chat.md` section 14 (rows, the paragraph at line 154), `docs/05-api-and-gateway.md` (new "Server limits" bullet: 30001, 30019, where answered, `max_members`), `docs/09-retention-and-privacy.md` (retention text), `docs/06-identity-accounts-invites.md` (the credit price key), `deploy/OPERATOR.md` (settings table), `deploy/RUNBOOK.md`, `deploy/UPGRADE.md` (wave 66 release notes), `CHANGELOG.md` (a Phase 66 section with my two lines; the other builders' sentences go there at integration). `docs/specs/admin-staff-server.md` and `docs/specs/moderation-compliance.md` had no "does nothing" sentence about `retention.large_file_days` (nothing to change).

## Not done

- **186 `tv.footer_note` (TV-UI-009): not done, stopped as the task says.** The Live TV guide gets its data from `GET /tv/guide` (the client `normaliseGuide` sets `footer_note: null`); showing a stored setting there needs a new field in that response (`footer_note`), which changes the generated client that ops owns. Adding the setting without a reader would break D-662. Proposal: a text setting `tv.footer_note` in group `tv` (max 200 characters, empty by default), a `footer_note: Option<String>` in the guide response, `normaliseGuide` reading it, plus one test each side. TV-UI-009 stays open.
- Visual check shots: see "Visual check".

## Every server-insert and membership-insert site found

| Site | Gated? |
|---|---|
| `sr-db/src/chat_repo.rs` `insert_guild` (used by `POST /guilds`, `POST /guilds/templates/{code}` and the package "create a server" route, all through `ChatService::create_guild[_from_snapshot]`) | Yes: user cap on the owner (not for the Staff Server) |
| `sr-cli/src/guilds.rs:85` `create_staff_guild` | Passes through `insert_guild`, skipped (`is_staff_guild`) |
| `sr-cli/src/guilds.rs:267` `seed_dev_guild` (insert + `:275` member rows) | Owner create is gated by `insert_guild` (one server, harmless); the extra member rows are not (dev seeding, D-661) |
| `sr-loadtest/src/seed.rs:203,208,225` | Create gated (one owner per guild); member rows not (D-661) |
| `sr-db/src/invite_join.rs` `join_via_invite` (invite, vanity) | Yes: both caps |
| `sr-db/src/invite_join.rs` `join_listed` (Server Directory) | Yes: both caps |
| `sr-db/src/settings_repo.rs` application approval | Yes: both caps, only if the row does not exist yet |
| `sr-db/src/apps_repo.rs` `install` (bot) | Yes: server cap, only if the bot is not a member yet |
| `sr-db/src/admin_repo.rs:550,689` (staff appointment, Staff Server ownership) | Not gated, on purpose (D-661) |
| `sr-server/src/userfeat.rs:180` (system user), `staff_feed.rs:574` (system user into the Staff Server) | Not gated, on purpose |
| `sr-cli/src/seed_scale.rs` (bulk seed with `COPY`) | Not gated (operator tool) |
| Test-only inserts (`gateway_pg.rs`, `inbox_pg.rs`, `push_pg.rs`, `arcade_jobs.rs`, `jobs.rs` tests) | n/a |

Arcade rooms, packages, the Discord bridge and Activities do not insert guilds or members of their own (the package route above goes through `create_guild_from_snapshot`).

## Client surfaces (item 2)

1. Create-server dialog: `apps/web/src/lounge/AddServerDialog.tsx` (create, 30001).
2. Join a Server (by code), same file (30001, 30019).
3. Invite page: `apps/web/src/app/AppServerInvite.tsx` (first-paint file: the limit sentences are loaded with a dynamic `import()` of `lounge/limitErrors.ts`, so only the `import()` call is added to the first-paint chunk).
4. In-chat invite card Join: `apps/web/src/lounge/msg/InviteCard.tsx`.
5. Server Directory join: `apps/web/src/lounge/shell/DirectoryDialog.tsx`.
6. Server Settings > Community > Access, approving an application: `apps/web/src/settings/server/sections/CommunityMore.tsx` (`useAction` in `settings/server/ui.tsx` takes an optional `describe` function; 30019 says the server is full, 30001 names the applicant).
7. Add an app to a server (OAuth consent page, `bot` scope): `apps/web/src/developers/Authorize.tsx`.

## Tests

All run on the head of the branch. Postgres tests: `--ignored`, scratch DB `sr_p66_caps` (each test of `guild_caps.rs` has its own database through `TestDb`, because the limits are platform settings).

- **Watched fail without the change:** `the_201st_server_is_refused_with_30001_and_the_number`, `fifty_parallel_joins_fill_a_server_to_the_limit_and_no_further`, `parallel_creates_cannot_pass_the_user_limit` (`crates/sr-db/tests/guild_caps.rs`). There is no base build of the new test file (it uses `sr_db::guild_caps`), so I made both checks a no-op in a throwaway build (an env flag in the two check functions, then restored from the committed file): the three failed (0 of 3), and all 13 pass with the checks.
- `crates/sr-db/tests/guild_caps.rs` (13): the three above, `a_join_at_the_member_limit_is_30019_and_uses_no_invite_use`, `the_directory_join_obeys_both_limits`, `approving_an_application_at_a_limit_changes_nothing`, `the_staff_server_is_never_counted_and_never_full`, `a_bot_account_is_not_limited_in_servers`, `deleted_servers_do_not_count`, `lowering_the_limit_removes_nobody_and_blocks_the_next_join`, `raising_it_applies_to_the_next_request_without_a_restart`, `an_unreadable_setting_gives_the_default` (stored `"many"`, `0`, `-1`, `null`, `1.5`, and one value out of range for each setting), `an_account_already_a_member_gets_already_member_not_a_limit_error`.
- `crates/sr-db/tests/apps_repo.rs`: `a_bot_install_into_a_full_server_is_30019`. Regression: `invite_join` (8), `settings_repo` application tests, `chat_repo` (5) pass.
- `sr-chat`: `error::cap_tests` (the two error mappings, the marker round trip), `settings::service_tests::approving_at_a_limit_leaves_the_application_pending_and_says_which_limit`, `the_memory_joins_enforce_both_limits_and_skip_the_staff_server`; the whole lib (402) passes.
- `sr-api`: `tests/guilds.rs` `creating_a_server_at_the_limit_is_30001_with_the_number` (plain and template create), `the_guild_object_carries_max_members`; `routes/settings/tests.rs` `accepting_at_a_limit_answers_30001_with_the_number_and_30019`, `approving_an_application_at_a_limit_names_which_limit`; `routes/developer/tests.rs` `a_full_server_refuses_a_bot_install_with_30019` (the arm; the Postgres test above does the install); `routes/messages/tests.rs` `a_large_attachment_expires_after_the_configured_days`, `changing_the_days_leaves_stored_attachments_alone`. The routes of `routes::developer`, `messages`, `settings`, `admin` pass (280). The Server Directory route has no API test of its own: it goes through the same `MemberJoiner` mapping as the invite accept (tested) and the repository test covers its limits.
- `sr-admin`: `every_listed_setting_names_its_reader`, `the_removed_settings_are_not_listed_and_the_wired_ones_have_their_new_range`, `the_two_server_limits_are_catalog_rows_with_the_core_numbers`, `the_removed_settings_are_not_listed_and_a_stored_row_of_one_is_ignored` (service test: a stored row of a removed key neither shows nor fails the list, and a patch naming it is refused); the whole lib (89) passes.
- `sr-storage`: `a_standard_limit_above_the_system_maximum_cannot_panic` (also: a stored value above the new maximum reads as the default), `the_large_file_days_set_the_lifetime_and_unreadable_ones_read_as_seven`; the whole lib (187) passes. `sr-core`: `guild_caps::tests::unreadable_values_give_the_default`.
- `sr-server`: `upload_limits::tests::the_standard_limit_follows_the_setting_without_a_restart` (the free size follows the live policy; a perk above it keeps its number).
- `cargo clippy -j 4 -p sr-core -p sr-db -p sr-chat -p sr-admin -p sr-storage -p sr-apps -p sr-api -p sr-server --all-targets -- -D warnings`: clean. `rustfmt --check` on every changed Rust file: clean.
- Web (single files, `--maxWorkers=1`): `lounge/__tests__/limitErrors.test.ts` (5), `serverFlows.test.tsx` (create, join by code, the invite page; 30), `p11ShellDirectory.test.tsx` (5), `settings/server/__tests__/p14communityMore.test.tsx` (approval; 25), `developers/__tests__/authorize.test.tsx` (add an app; 15): all pass. `tsc --noEmit` and eslint on the changed files clean; prettier on the code. Not run (CI): the rest of the web suite.
- Bundle check after `pnpm --filter @sideroom/web build`: `check-bundle-budget.mjs` ok (first-paint `app-*.js`: room 41,358 bytes to its limit, needs 8,192) and `check-entrance-isolation.mjs` ok. `AppServerInvite.tsx` is in the first-paint chunk; it only gained a dynamic `import()` of `lounge/limitErrors.ts`.
- `node scripts/check-doc-links.mjs`: ok.

## Visual check

Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p66-caps` (60 PNGs: desktop 1280x800 and phone 375x812, Editorial, Lounge Light, Lounge Dark), by `e2e/tests/p66-caps-shots.spec.ts` (local only: `SR_P66_CAPS=1`) on my own stack (ports 8441/5441, database `sr_p66_caps_ui`). Per theme and viewport: `create-at-limit`, `join-server-full`, `join-at-limit`, `invite-page`, `directory-full`, `application-approve-at-limit`, `application-approve-full`, `add-app-full` (the server's answer is faked with `page.route`; the community settings, the Directory listing and the OAuth consent preview are faked too, because the live stack has none), and `admin-settings` / `admin-settings-saved` (Admin > Settings with the two new fields; `guilds.max_per_user` set to 150, saved with a reason, page reloaded: the value is still 150; the spec resets it to 200 afterwards). I looked at a spread of them across both sizes and all three themes (create, join, invite page, directory, approval and add-app dialogs, and the Admin page on desktop light and phone dark): the sentence sits in each dialog's own error slot (red under the field, or the notice in the settings card), nothing overflows on the phone. Both specs of the file passed on the last run (the Admin one on an earlier run of the same code).

## What you can look at first

- `crates/sr-db/src/guild_caps.rs` is the whole enforcement; `crates/sr-core/src/guild_caps.rs` holds the numbers.
- `SizePolicy::from_settings` and `routes/messages/deps.rs` (`policy()`) for the live upload policy.

## Settings reader table (D-662)

`READERS` in `crates/sr-admin/src/settings.rs` (key, reader file, the text that names the key there); the test `every_listed_setting_names_its_reader` fails when a catalog key has no row, a row names an unlisted key, or the file does not contain the text. All 75 listed settings have a reader; I found none without one. One line each (file under `crates/`):

- `accounts.cap`: `sr-db/src/member_invites.rs`
- `limits.rate.ip`: `sr-api/src/rate_limit.rs`
- `limits.rate.session`: `sr-api/src/rate_limit.rs`
- `limits.rate.bot`: `sr-api/src/rate_limit.rs`
- `limits.rate.messages`: `sr-api/src/rate_limit.rs`
- `limits.rate.uploads`: `sr-api/src/rate_limit.rs`
- `abuse.friend_requests_per_hour`: `sr-api/src/rate_limit.rs`
- `abuse.dm_opens_per_hour`: `sr-api/src/rate_limit.rs`
- `abuse.webhook_execute_per_minute`: `sr-api/src/rate_limit.rs`
- `abuse.oauth_authorize_per_minute`: `sr-api/src/rate_limit.rs`
- `abuse.interaction_callbacks_per_minute`: `sr-api/src/rate_limit.rs`
- `abuse.watch_party_joins_per_minute`: `sr-api/src/rate_limit.rs`
- `abuse.emoji_uploads_per_hour`: `sr-api/src/rate_limit.rs`
- `abuse.sticker_uploads_per_hour`: `sr-api/src/rate_limit.rs`
- `abuse.appeal_attempts_per_hour`: `sr-api/src/rate_limit.rs`
- `abuse.storage_tests_per_hour`: `sr-api/src/rate_limit.rs`
- `abuse.counter_notices_per_day`: `sr-moderation/src/service.rs`
- `abuse.data_export_cooldown_hours`: `sr-db/src/data_export.rs`
- `abuse.bridge_messages_per_minute`: `sr-server/src/bridge_inbound.rs`
- `bridge.max_bridges`: `sr-server/src/bridge_inbound.rs`
- `bridge.delete_hold_hours`: `sr-server/src/bridge_sync_wiring.rs`
- `bridge.reconcile_minutes`: `sr-server/src/bridge_sync_wiring.rs`
- `bridge.struct_writes_per_sec`: `sr-server/src/bridge_sync_wiring.rs`
- `byos.presign_ttl_secs`: `sr-api/src/routes/storage/objects.rs`
- `byos.presign_ttl_cap_secs`: `sr-api/src/routes/storage/objects.rs`
- `byos.endpoint_allow`: `sr-api/src/routes/storage/flags.rs`
- `byos.worker_for_members`: `sr-api/src/routes/storage/transcode.rs`
- `byos.require_client_remux`: `sr-api/src/routes/storage/scopes.rs`
- `byos.client_convert_max_mib`: `sr-api/src/routes/storage/scopes.rs`
- `byos.who_may_mount`: `sr-api/src/routes/storage/flags.rs`
- `profiles.access`: `sr-api/src/routes/packages/profiles.rs`
- `byos.max_mounts_per_user`: `sr-api/src/routes/storage/mounts.rs`
- `byos.max_mounts_per_guild`: `sr-api/src/routes/storage/mounts.rs`
- `music.max_playlists`: `sr-api/src/routes/music/mod.rs`
- `music.max_items`: `sr-api/src/routes/music/mod.rs`
- `jukebox.vote_skip_percent`: `sr-api/src/routes/voice/jukebox.rs`
- `byos.upload_parts_per_hour`: `sr-api/src/routes/storage/uploads.rs`
- `invites.locked`: `sr-db/src/auth_store.rs`
- `invites.default_credits`: `sr-admin/src/invite_controls.rs`
- `invites.max_outstanding`: `sr-db/src/member_invites.rs`
- `invites.unused_code_days`: `sr-admin/src/invite_controls.rs`
- `invites.card_claimants_retention_days`: `sr-server/src/cards_jobs.rs`
- `invites.min_account_age_days`: `sr-db/src/member_invites.rs`
- `invites.suspicion_freeze_threshold`: `sr-db/src/member_invites.rs`
- `invites.suspicion_escalate_threshold`: `sr-admin/src/service.rs`
- `uploads.standard_limit_bytes`: `sr-api/src/routes/messages/deps.rs`
- `submissions.daily_upload_bytes`: `sr-server/src/review_jobs.rs`
- `submissions.max_open_per_member`: `sr-server/src/review_jobs.rs`
- `moderation.audit_retention_days`: `sr-chat/src/settings/audit.rs`
- `moderation.reports_per_day`: `sr-moderation/src/service.rs`
- `moderation.hold_review_days`: `sr-moderation/src/service.rs`
- `moderation.shared_ip_max_persons`: `sr-moderation/src/service.rs`
- `moderation.new_account_after_ban_days`: `sr-moderation/src/service.rs`
- `moderation.case_reopen_days`: `sr-moderation/src/service.rs`
- `retention.deleted_message_days`: `sr-moderation/src/service.rs`
- `library.ingest.rar_enabled`: `sr-server/src/library_jobs.rs`
- `library.ingest.max_upload_mb`: `sr-server/src/library_jobs.rs`
- `activities.package.max_mb`: `sr-activities/src/package.rs`
- `activities.package.reviewed_max_mb`: `sr-activities/src/package.rs`
- `activities.package.platform_max_mb`: `sr-activities/src/package.rs`
- `activities.package.max_files`: `sr-activities/src/package.rs`
- `review.lookup.enabled`: `sr-server/src/lookup_wiring.rs`
- `cinema.upload.max_gb`: `sr-server/src/media_jobs.rs`
- `retention.large_file_days`: `sr-api/src/routes/messages/deps.rs`
- `alerts.email_to_owner`: `sr-admin/src/service_ext.rs`
- `alerts.disk_percent`: `sr-admin/src/service_ext.rs`
- `alerts.backup_max_age_hours`: `sr-admin/src/service_ext.rs`
- `ops.storage_measure_max_objects`: `sr-server/src/storage_jobs.rs`
- `ops.disk_path`: `sr-admin/src/service_ext.rs`
- `ops.backup_dir`: `sr-admin/src/service_ext.rs`
- `tv.slate_asset`: `sr-cli/src/commands.rs`
- `tools.currency_rates`: `sr-server/src/fx_jobs.rs`
- `tools.currency_stale_hours`: `sr-server/src/fx_jobs.rs`
- `guilds.max_per_user`: `sr-db/src/guild_caps.rs`
- `guilds.max_members`: `sr-db/src/guild_caps.rs`

## Needs the commander

- **`packages/api-client/openapi.json` and the generated client must be regenerated at integration** (ops owns them this wave): new error code 30019, `max_members` on the guild object (`GuildObject`), and `30001` now documented with the number. No route was added or removed (`route_matrix.tsv`, the authz matrix: unchanged; `authz_matrix.rs:7188` names `retention.large_file_days`, which stays, so nothing in that file changed).
- Inventory rows (proposed, after SEC-AUD-182):
  - SEC-AUD-183 (done): the two server limits are operator settings and enforced atomically on every create and join path; 30001 with the number, new 30019, `max_members`.
  - SEC-AUD-184 (done): each create or join surface shows its own sentence for 30001 and 30019.
  - SEC-AUD-185 (done): `retention.large_file_days` and `uploads.standard_limit_bytes` are read (live); `messages.max_length` and `invites.credit_price_usd` removed; a test names the reader of every listed setting.
  - SEC-AUD-186 (not done): `tv.footer_note`, TV-UI-009 stays open (needs a field in the guide response).
- Proposed threat-model rows: (1) a join race past a limit (fifty parallel joins, parallel creates): the check and the insert share one transaction under a per-account/per-server advisory lock, tests `fifty_parallel_joins_fill_a_server_to_the_limit_and_no_further` and `parallel_creates_cannot_pass_the_user_limit`; (2) a settings value that disables a limit (`0`, a negative number, text, out of range): an unreadable value reads as the default, never zero (`an_unreadable_setting_gives_the_default`; the catalog range excludes 0 as well); (3) lowering a limit removes nobody (D-661): residual risk is an account or a server staying over the limit until members leave.
- Changelog sentences: in `CHANGELOG.md` (Phase 66 section, two lines).
- Owner questions (for Fable): (a) do 200 servers per account and 25,000 members per server suit this installation (`docs/dev/owner-checklist.md` row for ops: "do 200 and 25,000 suit this installation?"); (b) does the Owner want a per-server override of the member limit (not built, D-660); (c) is it acceptable that the Staff Server is the only exemption, so a staff account at its server limit cannot join another community; (d) `uploads.standard_limit_bytes` replaces the payments free tier as the free upload size (a perk above the free tier keeps its own number); lowering it below 30 MiB also lowers what free members may upload, and a file above it expires after `retention.large_file_days`.
- Docs outside my files: none made wrong that I know of. `docs/dev/owner-checklist.md` is ops's (row above).

## Notes

- The in-memory `MemoryCaps` knows no bot flag; Postgres exempts `accounts.is_bot` and the system user (flag bit 0).
- Scratch DBs `sr_p66_caps` and `sr_p66_caps_ui` and the target `p66-caps-target`: dropped (`sr_p66_caps_ui` at the end of the stack run, `sr_p66_caps` afterwards); the target `p66-caps-target` is deleted. The shared `sr_tpl_*` template databases that `TestDb` leaves on the server are the suite's and were not touched. The scratch folder `scratch-caps` (scripts and logs, outside the worktree) is removed.
