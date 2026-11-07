# fix/p54-auth (wave 54: auth follow-ups, D-556 to D-563, audit MED theme 5)

Base: main `f581b538`. No migrations, no new dependencies. One route added (`GET /sr/auth/2fa/methods`); OpenAPI
regenerated.

## Done
- **SEC-AUD-016 (D-556).** `gateway.max_unidentified_per_ip` (default 32, must be > 0) is a `[gateway]` key next to
  `max_connections` (`sr-core` `GatewayLimits`, validated in `Config::validate`), wired into `GatewayConfig` in
  `sr-server/src/app.rs`. Documented in `docs/dev/server.md` (config table), `deploy/OPERATOR.md` (new "Gateway"
  table), `deploy/compose/prod.yml` + `.env.prod.example` (`SR_GATEWAY_MAX_UNIDENTIFIED_PER_IP`), doc 05 6.4. The
  re-check interval, IDENTIFY deadline and message cap stay constants. Test: `sr-core` `previous_keys_and_smtp_parse`
  (parses the key, default 32).
- **SEC-AUD-017 (D-557).** `GET /sr/auth/2fa/methods` (public, reads the pending `sr_mfa` cookie; 401 without one)
  returns `{methods}`. The Discord sign-in redirects to a bare `/login/2fa` (only `?next=` when there is one). The 2FA
  page uses sessionStorage (password path) and otherwise asks the route; a 401 falls back to the documented default
  set; `?methods=` is ignored. Route matrix row (`public none`), documented in OpenAPI and doc 05 5.1; test MSW default
  handler answers 401. The sweep row `pub-login-2fa-passkey` now mocks the route instead of using the URL; the plain
  row expects the 401. Tests: Rust `discord_sign_in_asks_a_passkey_only_account_for_its_second_factor` (bare
  redirect), `pending_second_factor_methods_come_from_the_cookie_not_the_url`; vitest `passkeys.test.tsx` (asks the
  route, ignores `?methods=`, does not ask when storage has them).
- **SEC-AUD-018 (D-558).** Removing a rung member from a group DM sends the remaining members `CALL_UPDATE` with the
  shorter `ringing` (`voice_call.rs` `call_recipient_removed`); test `removing_a_rung_member_tells_the_others_the_shorter_ring`.
  `SessionsRevoked` is published for every account of the Person after a Person ban (`ban_person_with_reason`, which
  every ban route uses), after self-deletion (`identity::delete_account`) and after a staff erasure run (also an
  account deletion). Tests: `sr-admin` `a_person_ban_tells_the_gateway`, `sr-api` `delete_account_tells_the_gateway`.
- **SEC-AUD-019 (D-561).** The staff email change (`POST /admin/accounts/{id}/change-email`) reads the old address
  first, then mails it a new `EmailChangedByStaff` notice ("This was a staff action") with the D-167 "This wasn't me"
  link (a fresh `CancelEmailChange` token; with no pending change the link signs every device out, as after a
  self-made change). No session revocation, no second approver. Mail failure is logged, the change stands. Test:
  `account_edits_over_http` (MemoryMailer: notice to the old address, staff wording, cancel link, nothing to the new).
- **SEC-AUD-020 (D-562).** `run_data_erasure` refuses with 403 "You verified this request, so another staff member
  has to run the erasure." when the caller verified it, unless the caller is the Owner and nobody else (Owner or a
  `MANAGE_TAKEDOWNS` holder) is staff. The route keeps its step-up. Test `the_verifier_of_an_erasure_does_not_run_it`
  (three cases); existing DSR tests, `sr-moderation/tests/deadline_windows.rs` and the e2e moderation gate now have a
  second person verify. Spec `moderation-compliance.md` section 6 updated.
- **SEC-AUD-021 (D-563).** The settings PATCH answers `ReasonRequired` when `moderation.audit_retention_days` or
  `invites.locked` is in the patch without a reason (`settings::REASON_REQUIRED_KEYS`); nothing is stored. The Limits
  screen already demanded a reason. `purge_audit_before` writes one `platform_audit_log` row `audit.purge` (actor the
  system user, note "guild audit-log rows created before <UTC> removed: <n>") per run that removed rows. Tests:
  `sr-admin` `retention_and_lock_need_a_reason_in_the_settings_patch`, `sr-db` `an_audit_purge_that_removed_rows_is_itself_audited`,
  `invite_controls` lock test updated. Doc 09 updated.
- **SEC-AUD-022 (theme 5, step-up gaps).** Confirmed in `audit-p53/` (api-routes-1, api-routes-2, gap-api-routes-1,
  gap-api-routes-2, gap-auth-routes) and in code:
  - Staff rows added to `SENSITIVE_ACTIONS` (route-layer gate, `reauth_list_tests` walks them): account rename and
    badges, mount delist/relist, shelf object delist/relist, Patron/perk grant (`POST /admin/donations/entitlements`),
    graduated bans (`/admin/moderation/members/{id}/ban`, `.../subtree-cases/{case}/nodes/{id}/ban`). Doc 07 section 6
    table updated. Web callers already go through `withReauth` (admin `send`, storage api, donations `send`,
    `graduated.ts`), so the existing re-auth dialog opens.
  - Guild 2FA level: switching `mfa_level` from 1 to 0 now needs the same fresh second factor as a moderator action
    on that server (`900004`/`900123`); the server settings call already uses `withReauth`. Test added to
    `lounge.rs` `elevated_routes_need_a_second_factor_on_a_2fa_server`.
  - TOTP enrolment on a bare session: `POST /sr/auth/totp/enroll` now takes `{password}` (re-checked with the
    shared re-auth lockout). The web `TotpWizard` (Settings and the first-run welcome) opens the existing
    `PasswordPrompt` first ("Continue"), then the QR step. New strings in `p54-auth-1.ts`. Tests: Rust `enrol_totp`
    helper (no body and a wrong password are refused), vitest `account.test.tsx`, `firstRun.test.tsx`; e2e specs that
    enrol (exit-gate, keyboard-2, moderation-gate, entrance-journey, first-run) send or type the password.

## Not done / notes
- Not gated (left for the owner, see questions): graduated freeze/clear/lift/timeout, the appeal decision (an
  overturn unbans), the entitlement revoke, the age tools' suspension (api-routes-2), Discord link on a bare session,
  TOTP/passkey removal with only the password, password change without a second factor (gap-auth-routes,
  gap-api-routes-1). No self-grant guard on entitlements (only the step-up).
- The old `settings.account.setupError` / `preparingKey` strings are now unused (left in `usrset-3.ts`).
- An owner who removed their own second factor while their server requires 2FA must enrol again before turning the
  requirement off (`900123`).
- The `audit.purge` audit row is skipped with a warning when the system user (id 1) does not exist yet.

## Commits
`cb5a4767` (016), `5681a9a2` (017), `26caac8b` (018), `2ee5ecaa` (019), `f0414fbb` (020), `17ce1301` (021),
`cd57fe56` (022), `5eb8e893` (sweep console filter), then this handoff. Shared files made a clean split impossible:
doc 05 rides with 016, the Person-ban test and the settings-patch rule share `sr-admin` hunks with 018/021, and
`sr-api/src/routes/auth/tests.rs` (the 017 and 018 tests) is in the 022 commit.

## Audit findings checked
Every theme-5 item was confirmed in code before the fix; none was false. The other step-up items in the same reports
are listed under "Not done".

## Tests run
Rust: `cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-core -p sr-gateway -p sr-admin -p sr-db -p sr-api
-p sr-server -p sr-moderation --all-targets -- -D warnings` clean; `cargo test -p sr-core -p sr-gateway -p sr-admin
--lib` (78/55/217), `-p sr-api --lib` (832), `--test lounge --test route_matrix` (31/6); `sr-db` `settings_repo` and
`invite_controls` ignored tests for purge and lock (4); `sr-moderation --test deadline_windows` (3). Web: tsc, eslint +
prettier on changed files, vitest `src/routes/entrance src/settings/user src/public firstRun admin.test` (53 files),
css-tokens, i18n-audit (0), `pnpm build`, `check-entrance-isolation`, `check-bundle-budget` (all ok), `playwright
test --list` (931 tests in 73 files), `install.test.mjs`/`upgrade-drill.test.mjs`, `check-doc-links`,
`check-ops-files`. Not run: the full authorization matrix and `immutable_rules` (integration; the new rows answer
900004, which the matrix counts as allowed).

## Shots and flows (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p54-auth/`)
Own stack (ports 8213/5213, DB `sr_p54_auth_ui`, dropped). `SR_SWEEP_ONLY='^(pub-login-2fa|pub-login-2fa-passkey|settings-user-account)$'`,
desktop and phone, Lounge Light and Lounge Dark, looked at: the 2FA pages look as before (code boxes; the passkey page
leads with "Your passkey."). The only flag was the browser's console line for the expected 401 of
`/sr/auth/2fa/methods` on `pub-login-2fa`; the sweep now drops that line for a row whose `expect4xx` covers it.
Flows clicked by Playwright on the same stack: `entrance-journey` (Settings: password prompt, QR, code, recovery
codes, TOTP sign-in), `first-run` ("Set it up here": password prompt, then QR), `keyboard-2-sign-in`; all pass.

## Owner questions (defaults chosen)
1. Staff email change notice: it does not name the new address (the self-made "changed" notice does not either). OK?
2. Erasure two-person rule counts the Owner and `MANAGE_TAKEDOWNS` holders as "somebody else". OK?
3. Should the appeal decision, graduated freeze/lift/timeout and the entitlement revoke also need the step-up, and
   should a staff member be refused a perk grant to their own account?
4. TOTP enrolment asks for the password; should a passkey-only account be asked for the passkey instead/as well?
