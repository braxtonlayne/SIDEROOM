# Handoff: feat/p12-admin-mod-be (phase 12, admin and moderation backend)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p12-admin-mod-be, branch feat/p12-admin-mod-be (base integration/phase12 plus the answers commits). Local commits only, nothing pushed. Migrations used: `0790_appointment_revoked_reason.sql`, `0793_staff_role_templates_cases.sql`. Checklist: [answers-followups-p12.md](../answers-followups-p12.md) (15 G items ticked, each with a note).

## Done

| Item | What |
|---|---|
| G-21 (owner override of D-197) | Direct appointment is **kept, Owner only, behind re-auth**: `POST /admin/staff` refuses every non-Owner (even with `APPOINT_STAFF`) with the usual missing-permission/forbidden error; route, OpenAPI, matrix row and client untouched. `sr-cli staff appoint <user> --role … [--issuer …]` now calls `AdminService::appoint_cli` (audit action `staff.appoint_cli`, actor = system user 1, target and appointer = the issuer, default the oldest account); idempotent for the same roles. D-197, the answers entry and the G-21 text are rewritten. Tests: `direct_appointment_is_owner_only`, `cli_appointment_is_audited_and_idempotent`. The Staff screen on this branch had the direct form and keeps it (Owner decides which button to show); nothing was removed. |
| G-22 | `POST /staff/offers/{id}/respond {accept:true}` answers `900014 SECOND_FACTOR_REQUIRED` (new `ErrorCode::SrSecondFactorRequired`) when the account has neither confirmed TOTP nor a passkey; the offer DM says so; the DM button shows an "Enrol a second factor first" toast with a Settings link. |
| G-23 | Migration 0790 adds `staff_appointments.revoked_reason` (`left`/`kicked`/`revoked`), backfills it, and the `sr_staff_member_left` trigger reads the transaction setting `sr.member_removal` that `PgChatRepo::remove_member_kicked` and `insert_ban` set. `GET /admin/staff/former` plus a "Former staff" list in Admin → Staff. |
| G-25 | `get_profile` hides the `staff` and `owner` badges when `staff.show_badge` is false (nested key in the settings blob, like `cinema.share_watching`). |
| G-33 | Guild audit action 200 `APPLICATION_DECISION` with `options {status, application_id}`, target = applicant; label in `auditText.ts`. |
| G-41 | `sr_features::scope_module`/`current_module` (task-local) wrap each module handler in the runtime; `AuditSink` writes user 1 and `options {module, module_id}`; the audit viewer prints "SIDE ROOM · <module>". |
| G-54 (backend) | `critical` (same as severity critical; always DM'd, cannot be dismissed) and `send_dm`; ordinary announcements skip accounts whose `notifications.platform_dm` is false. |
| G-55 | `POST /sr/auth/account/delete` refuses with field errors `staff` and `owner`. |
| G-57 | `GET|POST /sr/invites`, `DELETE /sr/invites/{code}` (session only, bots denied, 10 creates per hour). Rules re-checked in one transaction: verified email, `invites.min_account_age_days`, account cap (D-014), open codes below credits (D-026), suspicion freeze (reported as generic `frozen`). **The user-settings Invites section did not exist on this branch**, so I built it (`settings/user/sections/Invites.tsx`, nav entry, search entry). |
| G-60 | `submissions.daily_upload_bytes` (default 50 GiB, 0 = unlimited) in the admin settings catalogue (Admin → Limits renders it); `sr-review` reads it through a new `AllowanceSource` port, wired in `sr-server` over `platform_settings`. The default constant is now 50 GiB (was 50 GB). |
| G-61 | `CinemaKind::purge` (deletes the unpublished draft title or episodes, discards the uploaded assets: transcode output queued for GC, state `removed` so retention deletes original and proxy) and `credit_refs`. |
| G-64 | `MANAGE_CASES` (22), `MANAGE_TAKEDOWNS` (23); Moderator gains cases, Senior Staff both; migration also ORs the bits into existing Staff Server roles by name; doc 07 and `permissions.ts` updated. |
| G-65 | `BanReason` (D-211 presets) and optional `reason_category` on both ban routes; `category` still wins; the web ban dialog presets a "Serious ban" checkbox from the chosen reason. |
| G-66 | Data export README and Settings → Account removal state what is retained (D-124). |
| G-67 | `GET /sr/features/flags` (`Feature::client_visible`, only `moderation.reports` so far); web `ReportsFlagSync` loads it at boot; `VITE_FEATURE_MODERATION_REPORTS` is gone. |
| Dashboard | `GET /admin/stats` now returns `voice_rooms` and `egress_bytes` (null without the embedded SFU). `sr-voice` keeps `LiveStats` updated on its one-second tick (egress bytes are the egress rate integrated per tick, since SFU start). |

Docs: doc 04 (revoked_reason), doc 05 (`900014`, `/sr/invites`, `/sr/features/flags`), doc 07 (bits, role table), D-197, inventory ticks (LNG-USET-025/026/027/029, ENT-INV-004).

## Still open, with reasons

- **G-54 admin composer**: the compose form does not exist on this branch; the admin UI branch must add `critical` and `send_dm` to it (`canDismiss` already stays false for critical).
- **`#review-queue` Approve/Reject card action**: the review-queue feed does not exist in this tree (nothing posts review cards), so there is nothing to attach buttons to. Not started.
- **G-67 `SR_FEATURE_UPDATE`**: no gateway dispatch on flag change (would need a new gateway event); clients read the flag at boot.
- **G-66 `compliance.data_request_deadline_days`**: not declared; the data-request queue does not exist yet. Settings → Account removal still shows the "not available yet" notice (LNG-USET-006 stays open, no delete flow in the UI).
- **G-60**: reads `platform_settings` directly, not "through `sr-features`" (the admin Limits catalogue is the platform settings store; `sr-review` has no `sr-features` dependency).
- **G-25**: no user-settings schema registry exists, so `staff.show_badge` has no schema declaration and no toggle in Settings yet (the web "badge label exists already" but not the switch).
- **`sr_admin::BADGES`**, **G-53** and the other G items outside the brief were not touched.

## Integration notes

- `feat/p12-admin-ui` (Owner-only "Appoint directly" button): consistent with the owner override; nothing to remove. If that branch also deleted the direct form from `Staff.tsx`, keep its version.
- Web fragment: `apps/web/src/i18n/messages/en/p12-admin-mod-be-1.ts` (all keys new; the user Invites keys are `settings.userInvites.*` because `settings.invites.*` belongs to server settings).
- OpenAPI, generated client and msw handlers are regenerated (`/sr/invites*`, `/sr/features/flags`, `/admin/staff/former`, `StatsView`, ban bodies). Regenerate again after merging other API branches.
- Route matrix: four new rows hand-set to `self`/`other` (`GET|POST /sr/invites`, `DELETE /sr/invites/{code}`, `GET /sr/features/flags`, all bot-deny) plus `GET /admin/staff/former`. `GET /sr/invites/{code}` (public validity check) shares a path with the new DELETE, so its `#undocumented` line left the matrix; it is still public and rate-limited.

## Tests run

- `cargo fmt --check`, `cargo clippy --all-targets -D warnings` on sr-admin, sr-chat, sr-features, sr-review, sr-cinema, sr-voice, sr-core, sr-auth, sr-db, sr-api, sr-cli, sr-server: clean.
- `cargo test` (no Postgres): sr-admin, sr-chat, sr-features, sr-review, sr-cinema, sr-api (lib 500 tests and `route_matrix`): pass.
- Postgres (own scratch database, `--include-ignored --test-threads=1`): all of `sr-db --tests` (every test binary, incl. new `member_invites`, `appointments_record_how_they_ended`, account deletion for staff, profile badge, Cinema purge) and the sr-server tests for module audit and announcement DMs: pass.
- Web (`--maxWorkers=2`): typecheck, eslint, prettier, full vitest (142 files, 1639 tests), build and bundle budgets: pass.
- Not run: Playwright (shared ports), `cargo test -p sr-gateway`/`sr-voice` (only added an atomic counter in `sr-voice`; it compiles and clippy is clean), full sr-server Postgres suite.

## Owner questions

- Should `staff.show_badge` and `notifications.platform_dm` get Settings switches (and a user-settings schema) now, or wait for the settings registry?
- Should the Staff screen keep both "Send offer" and the Owner-only direct form visible to the Owner, or hide the direct form behind an "Emergency" fold?
