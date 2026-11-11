# Handoff: fix/p56-auth (wave 56, SEC-AUD-050 and SEC-AUD-051, D-580 and D-581)

Branch `fix/p56-auth` (base `integration/phase55`). Not pushed, no PR.

## Done
- **SEC-AUD-050 (D-580)**: `POST /sr/auth/recovery-codes` accepts a confirmed TOTP or at least one passkey as the
  second factor (password and the D-573 proof still required; 400 when there is no factor). The web has one
  `RegenerateCodes` component (`settings/user/sections/Account.tsx`): in the two-factor card while TOTP is on, in the
  passkeys card while TOTP is off and a passkey exists (`PasskeysCard` takes `totpOn`). The D-573 row now reads
  "Discord unlink stays as it is (password only, no second factor)"; docs/05 row updated.
- **SEC-AUD-051 (D-581)**: `SENSITIVE_ACTIONS` gained `restrict.case_warn`, `restrict.data_hide`, `app.suspend`,
  `app.unsuspend` (the subtree case close stays ungated). `POST /admin/donations/entitlements` with
  `kind: server_pool` to a guild the caller owns: 403 `50001`, Owner included, logged `denied`
  (`MoneyCtx::refuse_own_pool_grant`, `PaymentService::guild_access`); a grant to a server the caller only belongs to
  still works. D-574 wording, doc 07 prose and table, doc 15 T58 text, the OpenAPI document and generated client
  (doc comment only) and inventory rows SEC-AUD-050/051 are updated.
- Commits: bb282d94 (050), c68443ac (051; its subject says WIP, superseded by later commits, no history rewrite),
  b3fb553f (T58), then the test-filter fix and 1ea08c41 (OpenAPI, doc 07 table, clippy fix), plus this handoff.

## Tests run
- `cargo test -j 4 -p sr-api -p sr-payments`: 841 passed in sr-api, all other targets ok.
- `cargo clippy -j 4 -p sr-api -p sr-payments --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: ok.
- Authz matrix with Postgres (`every_route_enforces_its_authz_row`, `--ignored`): passed; `SHAPE_CEILING` unchanged.
- New Rust tests: `recovery_codes_regenerate_for_a_passkey_only_account` (passkey-only succeeds with the proof; no factor
  refused; wrong password refused; bare password refused), `a_staff_member_never_grants_a_pool_level_to_a_server_they_own`
  (Owner and Treasurer refused, belongs-only allowed, two `denied` log rows), the family test now lists the four new routes.
- `every_listed_staff_route_demands_the_second_factor` skips `/admin/developers/*` rows: the developer portal is not
  mounted in that environment; the family test and the authz matrix (full router) cover them.
- Web: vitest `passkeys.test.tsx` (10, three new for button placement), `p55staffReauth.test.ts` (17, two new: case
  warn and data-request restriction prompt once and retry once); `tsc`, eslint, prettier on changed files,
  `check-css-tokens`, `i18n-audit` (0), `check-doc-links`, `node --test scripts/threat-model.test.mjs` (5 pass),
  `playwright test --list --project chromium` loads 942 tests.

## Visual check
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p56-auth/`: `a-no-passkey` and `b-passkey` (sweep, top of
the page only), `c-passkey-totp-off` (security cards with a passkey and TOTP off, desktop and phone, plus the
regenerate dialog) and `d-no-factor` (no passkey: no regenerate button). Passkey rows were inserted by SQL into the
scratch DB (no WebAuthn). Read by me: the button sits next to "Add a passkey" in the passkeys card and wraps cleanly on
the phone; the two-factor card keeps only "Enable authenticator app". Captured in the dark Lounge theme only; the
shots in `a`/`b` cover light at the top of the page. Not checked: Editorial light for the passkeys card (no component
or token changes). Seen but not mine: the account-removal text shows a literal `{years}`.
- The visual run used the wave 55 `sr-server` binary (no Rust changes needed for the page).

## Not done / notes
- No staff screen exists for developer app suspend/unsuspend, so there is no step-up prompt to wire; only the API is gated.
- The web's staff prompt is `withReauth` on the shared `send` helpers; both warn-from-case and the data-request restriction already used it.

## Owner questions
- Should app suspend and unsuspend get a staff screen? There is none today.
- The recovery-codes button keeps the label "Regenerate recovery codes" (D-580 says "Regenerate codes").

## Proposed threat-model rows
T58 was edited in place (D-581 added; residual: a pool grant to a server the caller merely belongs to is allowed on
purpose). No new row proposed.
