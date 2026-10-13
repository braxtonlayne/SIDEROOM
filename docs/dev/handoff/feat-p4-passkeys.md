# Handoff: feat/p4-passkeys (COMPLETE)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p4-passkeys`, base `integration/phase3`. Local commits only, nothing pushed.

## Done

ENT-AUTH-010 and F-14 (ticked), end to end on the pure-Rust verifier from the spike.

Backend
- Migration `0180_passkey_credentials.sql`: the unused `webauthn_credentials` table from 0001 is renamed to `passkey_credentials` and reshaped (`cose_key`, `alg`, `backup_eligible`, `backup_state`, `aaguid`, `name` 1 to 64 chars); `accounts.user_handle` (32 random bytes, unique, never the account id). `sr-db/src/accounts.rs::reset_2fa_in` follows the rename.
- `AuthStore` (`sr-auth::store`) gains `PasskeyRecord` and `passkey_user_handle / insert / list / find / rename / delete / record_use` (counter compare-and-set). Implemented in `PgAuthStore` and `MemoryAuthStore`.
- `sr-api/src/routes/auth/passkeys.rs`: register begin/finish, login begin/finish, `verify/begin` (step-up for signed-in sessions), list, rename, delete; utoipa docs in `PasskeysApiDoc`, merged into `auth::openapi()`.
- Passkeys plug into `check_second_factor` as `method: "passkey"` (`code` = JSON text of `{ceremony_id, credential}`); `require_second_factor` (email change) now fires for accounts with a passkey; password login returns `methods` including `passkey` and asks for a second factor when the account has a passkey or TOTP; trusted devices still skip it.
- `webauthn` feature enabled via the `sr-api` and `sr-server` dependency lines on `sr-auth`.
- Rate limits: register begin 10 per 10 min and finish 20 per 10 min per account, login begin 60/min and finish 30/min per IP plus the per-IP lockout, verify begin 30 per 10 min per account; password re-auth shares the login lockout. Max 20 passkeys per account.
- Regenerated `packages/api-client/openapi.json` and `src/generated/schema.d.ts`.

Web
- `src/lib/webauthn.ts` (base64url, option decoding, create/get helpers), `src/api/passkeys.ts`.
- `PasskeyButton` now uses the new endpoints (passkey-only login, and second step with `secondFactor`). The 2FA page leads with the passkey when it is the only authenticator, and offers it beside TOTP. `MfaMethod` is `passkey` (was `webauthn`).
- Settings > My Account > Passkeys card (list, add with password + name, rename, remove with password; recovery codes shown once when the first factor creates them). `PasswordPrompt` and `RecoveryCodes` in `Account.tsx` are now exported (additive).
- The "Use a passkey" button was already rendered whenever the browser supports WebAuthn (it was not hidden by CSS); it stays, and the Playwright test now covers it.

Tests
- Rust: 13 sr-api route tests with a software ES256 authenticator (`routes/auth/tests/passkeys.rs`), 1 mutation-robustness test in sr-auth (stand-in for a fuzz target), 2 Postgres-backed ignored tests (`sr-db/tests/auth_repo.rs`).
- Web: 11 new vitest tests (helpers, login button, second step, settings card).
- Playwright `e2e/tests/passkeys.spec.ts` (chromium only, CDP virtual authenticator) passes against a real sr-server on a throwaway DB: register in Settings (wrong password refused before the authenticator is touched), rename, passkey-only login, password login with the passkey as second step, counter rewind refused then recovered, remove, removed key refused. The full chromium e2e suite (13 tests) passed together with it.

## Decisions

- Ceremonies are in-memory, single-use, 5 minutes (the spike report allows this for one node). They share the single-node assumption of the pending-2FA table and lockouts; moving to Postgres is the same job for all three. Documented in `passkeys.rs` and doc 05.
- The `webauthn_credentials` table was renamed rather than adding a second table (nothing used it).
- Primary passkey login and registration require user verification; as a second factor user verification is only preferred.
- Counter regression: the attempt fails like any bad assertion and writes `passkey.counter_regression` (actor = owner, target = credential row). The credential is not disabled automatically (a synced passkey can legitimately report 0); staff or the owner can remove it.
- A passkey makes password logins ask for a second factor. To avoid lock-out, the first passkey issues recovery codes when the account has none, TOTP removal keeps recovery codes while passkeys remain, and deleting the last factor drops them.
- RP id is the host of `public_url` (override `AuthConfig.webauthn_rp_id`), allowed origins are `public_url` plus `AuthConfig.webauthn_origins`. sr-server does not expose these in config yet; a dev setup on another origin needs a config key.
- No cargo-fuzz target: `cargo fuzz` is not installed and I did not install tools.

## Not done / open

- Firefox and WebKit have no virtual authenticator, so the browser proof is Chromium only. Real authenticators (Safari/iCloud BE/BS, `credProps`) are still unverified.
- Conditional UI (autofill passkey prompt) is not implemented; the explicit button is.
- LNG-USET-005 (passkey list inside the full Password & Authentication spec) is left unticked: the passkey part is done, the item covers more. ENT-UI-026 left unticked (its TOTP step is covered by exit-gate, the passkey step by passkeys.spec.ts, but the verify-email step is not in either).
- Staff-side surfaces (admin passkey list or reset button) are untouched; `reset_2fa` already deletes passkeys.
- Pending ceremonies are lost on restart (users just retry).
