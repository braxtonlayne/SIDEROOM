# Handoff: feat/p10-admin-be (phase 10, admin backend)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p10-admin-be, branch feat/p10-admin-be (base integration/phase10). Local commits only, nothing pushed. Migration range used: 0650 (one file).

## Done (inventory ticked in docs/inventory/10-admin.md)

| ID | What |
|---|---|
| ADM-STF-001 | Migration `0650_admin_backend.sql`: `appointment_delegations`, `staff_appointment_offers`, `staff_alerts`, `platform_announcements`, trigger `sr_staff_member_left`. The rest of the item (is_staff_guild index, roles.platform_permissions, staff_appointments, staff_access_log, platform_audit_log) already existed in 0010, 0042, 0130. |
| ADM-STF-003 | Creating and redeeming a guild invite to the Staff Server is refused (`SettingsError::StaffServerInvites`, 403 `FeatureDisabled`). Checked in the create route, the Postgres join transaction and the memory repo. "Hide Invite to Server" is client work. |
| ADM-STF-004 | Offer flow: `POST/GET /admin/staff/offers`, `DELETE /admin/staff/offers/{id}`, appointee answers `POST /staff/offers/{id}/respond {accept}`. The offer is a system DM (embed + Accept/Decline buttons with `custom_id` `sr:offer-accept:<id>` / `sr:offer-decline:<id>`); only an accepted offer creates the appointment. Accepting re-checks the offerer's standing and the grant rules. Offers expire after 7 days (hourly sweep job). Delegations: `GET /admin/staff/delegations`, `PUT/DELETE /admin/staff/{id}/delegation` (Owner only). |
| ADM-STF-005 | A DB trigger closes the appointment (and delegation) when the Staff Server member row is deleted (leave, kick). Standing is evaluated on every request, so `/admin` access ends on the next call. The removed member gets `SR_STAFF_ALERT {kind: staff_revoked, revoked: true}` on their own user topic (`AdminService::push_revoked`, also called by the revoke endpoint and by the feed on `MemberLeft`). |
| ADM-STF-007 | `crates/sr-server/src/staff_feed.rs`: `StaffVoice` posts cards as the system user (id 1) through the normal message stack (becomes a Staff Server member on first use), sends system DMs, and implements `sr_admin::StaffNotifier`. `POST /admin/card-actions {channel_id, message_id, custom_id}` runs a button: the message must be a system-user card in a Staff Server channel the caller can see; actions `sr:ack:<alert>`, `sr:clear-suspicion:<person>`, `sr:retry-job:<job>` run with the presser's own permissions. |
| ADM-STF-009 | `GET/POST /admin/reauth` (TOTP, recovery code or passkey proof, reuses the existing 15-minute `sessions.mfa_verified_at` window). `StaffCtx::require_reauth()` guards: private content routes, `change-email`, `force-password-reset`, `reset-2fa`, direct appoint, set roles, revoke, offers, delegation, announcements. A refusal is 403 `SrReauthRequired` (900004); private-route refusals are also logged as `denied`. Off in tests; ON in the real server unless `SR_ADMIN_REQUIRE_REAUTH=0`. Staff with no second factor enrolled cannot use these actions at all. |
| ADM-DASH-002 | `GET /admin/stats` is cached 30 s (`AdminService::cached_counts`, permission checked before the cache) and now also returns `accounts_cap`, `review_pending`, `invites_open`, `alerts_unacked`, `storage_bytes`. `GET /admin/alerts` + `POST /admin/alerts/{id}/ack` for "recent alerts". |
| ADM-PPL-003 | New: `rename` (username/display name), `change-email` (unverified afterwards, mailed links to the old address die, log keeps it masked), `badges` (catalog in `sr_admin::BADGES`, bits 8 to 11 of `accounts.flags`), `force-password-reset` (unusable hash + sessions and trusted devices ended; the user uses "Forgot password"). Suspend/unsuspend with cap check, reset-2fa, force-logout already existed. |
| ADM-COM-004 | `GET /admin/private/channels/{id}/messages` (deleted messages within `retention.deleted_message_days`, edit history, paging), `/admin/private/accounts/{id}/dms`, `/admin/private/guilds/{id}/channels`. Reason mandatory (query `reason` or `X-Audit-Log-Reason`), fresh 2FA, `READ_PRIVATE_CONTENT`. The `private_read` row is written to the hash chain BEFORE content is read, also for unknown channels; message content never enters the log. |
| ADM-PLT-009 | `POST/GET /admin/announcements`, `DELETE /admin/announcements/{id}`, `GET /announcements/active` (any signed-in human). `send_dm` sends a system DM to every active account in a background task (40 ms pace). |
| ADM-ALR-001 | `staff_feed::spawn` consumes `Topic::Platform`: every `StaffAlert` is stored (`staff_alerts`) and posted to `#alerts` with Acknowledge + Open in Admin. New alert kinds `DiskLow`, `BackupFailed`, `TranscodeFailed`, produced by the 15-minute health job (`admin_jobs.rs`: disk %, newest backup age, failed `transcode_jobs`; each kind at most once per day, transcode once per job). Suspicion, payment, chain-break and banned-person alerts already published events and now reach the channel. |
| ADM-ALR-002 | New `DomainEvent::StaffFeed`: registration posts "@new joined, invited by @issuer" to `#new-members` (usernames only), a credited invoice posts to `#donations` (no donor name, link to the invoice). |
| ADM-ALR-003 | Setting `alerts.email_to_owner`; one mail per alert kind per 30 minutes to the Staff Server owner (existing `Mailer`). |
| ADM-GRAPH-002 | `crates/sr-graph-wasm` (Barnes-Hut quadtree + Fruchterman-Reingold, deterministic per seed, `layoutGraph(n, Uint32Array edges, iterations, seed, springLength) -> Float32Array`), `loadGraph()` in `@sideroom/wasm` (not part of `initWasm`, so nobody loads it unless the graph page does). |

Also fixed: the nine `/sr/auth/*` routes (passkeys, password, username, email change) answered 500 instead of 401 without an auth service because they extracted `Svc` before the principal. Extractor order swapped in all handlers that take both, and `NEEDS_DEPS_FIRST` removed from `route_matrix.rs`.

New alert/limit settings (Admin Limits catalog, group `alerts`): `alerts.disk_percent` (80), `alerts.backup_max_age_hours` (36), `alerts.email_to_owner` (off), `ops.disk_path`, `ops.backup_dir` (empty = backup alert off). Documented in `deploy/RUNBOOK.md`.

## Still open (not ticked or only partly done)

- Client work that these APIs need: hide "Invite to Server" in the Staff Server; handle the Accept/Decline buttons of the appointment DM (call `POST /staff/offers/{id}/respond`) and the `sr:ack` etc. buttons of staff cards (call `POST /admin/card-actions`); close `/admin` on `SR_STAFF_ALERT {revoked}`; show the announcement banner (poll `GET /announcements/active` at start; there is no gateway push for announcements yet); the re-auth modal (`GET/POST /admin/reauth`); the Private Content Viewer UI.
- `#review-queue` Approve/Request changes/Reject buttons (sr-review feed) are REV work; the card-action registry only knows ack, clear-suspicion and retry-job. Alert cards for suspicion carry no "Clear suspicion" button because `StaffAlert` carries no person id; the action itself works.
- Announcement DMs go to everyone at once. There is no per-user opt-out or schedule.
- No Playwright or web gates were run (shared ports; the web app was not touched except regenerated `packages/api-client`).
- `ADM-GRAPH-003/004/005` (WebGL renderer etc.) not started: FE and the WebGL renderer needs an owner decision.

## Decisions I made (please confirm)

- Direct `POST /admin/staff` (appoint without acceptance) is still there, now behind re-auth. Doc 07 says the appointee accepts; the offer flow is the documented path. Remove the direct route or restrict it to the Owner?
- A delegate (open `appointment_delegations` row) gets `APPOINT_STAFF` added to their standing and may grant only bits inside the ceiling the Owner set and that they hold themselves. Note doc 07 D-162 gives the default Senior Staff template `APPOINT_STAFF` outright, which is not the same thing as a delegation; both work side by side.
- Badge names and bits (`early_member`, `contributor`, `bug_hunter`, `honorary` at bits 8 to 11 of `accounts.flags`) are my proposal; staff cannot touch bits 0/1 (system, app developer).
- Re-auth is ON by default in production and locks staff without TOTP/passkey out of the sensitive actions (the Owner included). `SR_ADMIN_REQUIRE_REAUTH=0` disables it for development and e2e. Keep this default?
- `revoked_by` of an appointment closed by the leave trigger is the member themselves (the database does not know who kicked); the feed posts the alert and the audit log of the kick lives in the guild audit.
- Email alerts go to the Staff Server owner's account email only.

## Tests run

- `cargo fmt --check`; `cargo clippy --all-targets -D warnings` on sr-events, sr-admin, sr-db, sr-chat, sr-gateway, sr-api, sr-server, sr-graph-wasm: clean.
- `cargo test`: sr-events, sr-admin (40, incl. 15 new in `tests_ext.rs`), sr-chat, sr-gateway, sr-graph-wasm (8), sr-api (route matrix incl. the 401 fix, 11 new admin endpoint tests, staff-invite test), sr-server, sr-db.
- Postgres (`--include-ignored --test-threads=1`): sr-db `admin_repo` (9), `admin_ext_repo` (6 new), `invite_join` (6 incl. new staff test); sr-server `staff_feed_pg_tests` (5 new: cards, appointment DM, events/mail/revocation push, announcement DMs, health check).
- `packages/wasm` vitest `graph.test.ts` (4) against a dev wasm build; api-client regenerated (openapi.json, schema.d.ts, MSW handlers: 560 operations).

## Notes for the merge

- `Cargo.lock` gains `sr-graph-wasm` and an `fs4` edge on sr-server (already in the tree via tantivy).
- `sr_admin::Standing` has a new field `grant_ceiling`; `Caller` is unchanged. `SystemCounts` has five new fields. `StaffAlertKind` has three new variants and `DomainEvent` one (`StaffFeed`); the gateway ignores it.
- New routes are in `route_matrix.tsv` (reviewed: all `staff`, except `/staff/offers/{id}/respond` = self and `/announcements/active` = other; bots denied for both, `policy.rs`).
- Reserved job ids: 131 (health), 132 (offer expiry).
