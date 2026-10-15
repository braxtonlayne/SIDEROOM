# Handoff: feat/p12-devplat-be (developer platform backend and portal)

Branch `feat/p12-devplat-be`, based on the Phase 8 to 11 answers (D-185 to D-216). Checklist: [answers-followups-p12.md](../answers-followups-p12.md).

## Done

- **G-01 App testers (D-204).** Migration `0770_app_testers.sql`. `sr-apps`: `service/testers.rs` (`invite_tester`, `respond_tester`, `list_testers`, `remove_tester`, `is_tester`), `perms::check_install` takes `installer_is_tester` (an accepted tester installs a `development` app, also a private one, in guilds of at most 25 members where they hold `MANAGE_GUILD`; testers stop counting once the app is `in_review` or `released`). Routes `GET|POST /applications/{id}/testers`, `DELETE .../{account_id}`, `POST /developers/tester-invites/{id}/respond` (invitee only). The invitation is a system DM with `sr:tester-accept:<id>` / `sr:tester-decline:<id>`; web `parseStaffButton` runs them. Web: Testers card on the Installation tab. The cap (25) counts invited and accepted rows together and is enforced under a row lock on the application. Account deletion removes the account's tester rows.
- **G-03 Interaction responses and `SEND_MESSAGES` (D-206).** Channel-visible responses (type 4, and follow-ups that resolve a deferred response) fail with `50013` and a message naming the channel when the bot lacks `SEND_MESSAGES`; ephemeral responses never reach the check. The permission itself was already enforced by the chat service, so the work was the clear error and the tests.
- **G-04 Seal the signing seed (D-185).** Migration `0771`. New `sr_db::sealed::SecretBox` (the TOTP envelope layout, now shared; `PgAuthRepo` delegates to it). `PgAppsRepo::new(pool, SecretBox)` seals on write and unseals on read, so `sr-apps` stays key-free (deviation from the checklist, which put unsealing in `signing.rs`/`endpoint.rs`). Startup job `apps.seal_seeds` (in `sr-server` `app.rs`) converts legacy rows and refuses to run without a configured `server.secret_key`. `sr-cli rotate-secret-key` re-encrypts seeds too. `secret_columns.rs` now treats `seed` as a credential name and `_sealed` as a safe suffix; the 0710 column is listed in `PLAINTEXT_OK` as legacy.
- **G-05 Endpoint auto-disable (D-186).** Same migration. `endpoint_health_step` (pure, unit-tested) is applied atomically in Postgres; 25 consecutive failures inside 10 minutes disable the endpoint (dispatch falls back to the gateway), the owner gets a system DM, and `POST /applications/{id}/endpoint/verify` runs the PING check and re-enables. The owner view of the app carries `interactions_endpoint_state`; the portal shows a notice with "Verify and re-enable". Outcomes are reported from `deliver_to_endpoint` in `sr-api`, not from `SafeEndpointClient` (the client stays a plain transport). A 200 reply that fails to apply as a callback is not counted as a failure.
- **G-43 Bot deny-list (D-189).** `auth/policy.rs` denies bots on `/users/@me/{reminders,scheduled-messages,bookmarks,birthday}*` and also on `POST /channels/{id}/scheduled-messages`; matrix `BOT` column updated.
- **G-20 `reqwest` ban (D-203).** `deny.toml` `wrappers` list: `sr-net`, `sr-unfurl`, `sr-push`, `sr-storage`, `sr-transcoder`, plus the transitive `oauth2` (Discord OAuth) and `object_store` (S3). A new crate depending on `reqwest` fails `cargo deny check bans`; verified by removing an entry. `cargo-deny` 0.20.2 is installed; `cargo deny --offline check bans` passes.
- **G-02 Porting notes.** Doc 05 section 7 item 6 (install permission cap, `SEND_MESSAGES`, privileged intents reviewed at release, testers, endpoint health), doc 11 section 2.1, `docs/specs/developer-platform.md`.
- New plumbing: `SystemDm` / `SystemCard` port in `sr-api` `routes/developer` (set after construction with `DevDeps::set_system_dm`; `StaffVoice` implements it).
- Docs: doc 12 stored-credentials table and rotation text mention the sealed seed. Inventory 17: DEV-APP-001 ticked, DEV-APP-004 and DEV-PORT-003 notes updated.

## Still open / follow-ups

- Drop the plaintext column `applications.interactions_signing_seed` in a later migration once every deployment has run `apps.seal_seeds` (it is NULL after the job; the number is not mine to pick).
- The "Testers" tab of DEV-PORT-003 lives on the Installation tab; DEV-PORT-003 stays unticked because Emojis, Webhooks, Rich Presence and Activities are not built.
- A development-time install of a private development app by a tester is allowed (testers exist to try the app); the checklist did not say. Owner may want testers limited to public apps.

## Tests run

- `cargo test -p sr-apps` (59), `-p sr-api` (all, including `route_matrix` and the new `tester_tests`, `endpoint_health_tests`, SEND_MESSAGES flow tests), `-p sr-cli`.
- Postgres (own TestDb clones, `--test-threads=1`): `sr-db` `apps_repo` (16, three new), `secret_columns`, `account_deletion`, `sr-server` `bot_pg` and `staff_feed`.
- `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on `sr-apps sr-db sr-api sr-server sr-cli`, `cargo deny --offline check bans`.
- Web: typecheck, eslint and prettier on touched files, vitest (`src/developers`, `staffCards`, `testerCards`, `src/i18n`: 130 passing), `vite build` and the bundle budget (developers 210.9 KB of 400 KB).
- OpenAPI, api-client schema and MSW handlers regenerated; route matrix rows reviewed (no `?` left).

## Decisions and owner questions

- Cap of 25 testers counts open invitations too (prevents invitation spam); the answers say "up to 25 accepted". Easy to change in `invite_tester` (repo) if the owner wants only accepted rows counted.
- Declining an invitation deletes the row; no "declined" state.
- Endpoint failure window starts at the first failure of a streak (a success resets the streak).
