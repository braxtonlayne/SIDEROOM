# fix/p55-signin (wave 55: sign-in changes take the second factor, D-573)

Base: `integration/phase54` `1480498f`. No migrations, no new dependencies, no new routes. Two request bodies
changed (`POST /sr/auth/discord/link` now takes `{password, mfa?}`; `mfa?` added to the others); OpenAPI, the
generated client and the MSW handlers regenerated.

## Done
- **SEC-AUD-041 (D-573), server.** Every self-service change to how an account signs in takes the password and,
  when the account has a second factor (TOTP or a passkey), the D-167 proof (`mfa {method, code}`, or a check on
  this session in the last 15 minutes; a recovery code counts; the shared `mfa-reauth` lockout applies):
  - `POST /sr/auth/password` (after the new password passed its checks, so nobody proves a factor for a weak one),
  - `DELETE /sr/auth/totp`, `DELETE /sr/auth/passkeys/{id}`, `POST /sr/auth/recovery-codes`,
  - `POST /sr/auth/discord/link`: new body `{password, mfa?}` (it had none); password, then "already linked", then
    the factor; no state cookie before both pass. 404 when Discord is not configured still comes first.
  - adding a second factor to an account that has one: `POST /sr/auth/totp/enroll` (after the "already enabled"
    check, so a TOTP account still gets that message) and `POST /sr/auth/passkeys/register/begin` (after the
    20-passkey limit).
  - Discord unlink unchanged (password only), as the row says.
  The helper is `mfa::reauthenticate_with_factor` (password, then `identity::require_second_factor`, the D-167
  function the email change and account delete already use). `PasswordBody` gained `mfa`.
- **SEC-AUD-041, web.** `admin/reauth.ts` `withSecondFactor(call)`: on 400 `errors.mfa.required` it asks the
  existing re-auth dialog (`ReauthHost`, mounted at the app root) and retries the same call once; the dialog now
  takes a reason and says "Changing how you sign in needs your second factor too..." for these (admin wording
  unchanged otherwise). Wrapped: `changePassword`, `totpEnroll`, `totpDisable`, `regenerateRecoveryCodes`,
  `discordLinkStart` (settings `api.ts`), and passkey add/remove in `Passkeys.tsx` (not in `api/passkeys.ts`, which
  the sign-in pages load: D-385). "Link Discord" now opens the existing `PasswordPrompt` first. No new component;
  strings in `p55-signin-1.ts`.
- **Found by the visual check, fixed:** the app-root `ReauthHost` sat outside every section's theme root, so the
  `--ui-*` tokens were undefined there and the dialog drew with no surface, backdrop or field boxes (text floating
  over the page and over the dialog under it). This also hit the Lounge staff cards and the server 2FA switch
  before this wave. The root host is now `RootReauthHost` (the same dialog inside `ThemedOverlayRoot`, the
  member's Lounge theme, like the player and the drawer), rendered on signed-in pages only; the admin shell keeps
  its own host in its own theme.
- Docs: doc 05 section 5.1 (rows plus a "Sign-in changes (D-573)" paragraph), doc 06 section 4, inventory row
  SEC-AUD-041 added and ticked.

## Tests (each fails without the change)
- Rust (`sr-api` lib, `routes::auth`): `disable_and_regenerate_require_the_password_and_the_second_factor`
  (regenerate and TOTP removal refuse the password alone with `errors.mfa.required`, accept a recovery code / TOTP,
  a wrong code is `invalid`, the 15-minute window), `change_password_needs_the_second_factor_when_the_account_has_one`
  (nothing changes without it; the window covers the next change), `discord_link_needs_the_password_and_the_second_factor`
  (bare body, wrong password, missing factor: no state cookie), passkeys: `totp_enrolment_needs_the_existing_passkey`,
  removal and a second registration refused without the first passkey's proof (`register_list_login_rename_and_remove`,
  `registration_needs_a_session_the_password_and_a_fresh_ceremony`), and the existing tests updated to prove
  (`an_account_holds_at_most_twenty_passkeys`, `removing_totp_keeps_recovery_codes_while_passkeys_remain`, Discord).
- vitest: `account.test.tsx` (password change through the dialog with a recovery code; TOTP removal cancelled keeps
  the authenticator), `passkeys.test.tsx` (second passkey: dialog before the browser prompt), `p15Discord.test.tsx`
  (password prompt first; dialog then retry).

## Gates run
Rust: `cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-api --all-targets -- -D warnings` clean;
`cargo test -j 4 -p sr-api --lib routes::auth` (110 passed). OpenAPI regenerated (a first attempt named the Discord
body `LinkBody`, which replaced the Cinema `LinkBody` component; renamed `DiscordLinkBody`), api-client `generate`
and `mocks:generate`, `mocks:check` ok. Web: tsc, eslint + prettier on changed files, vitest
`src/settings/user src/admin src/routes/entrance` + firstRun + p12safety (93 files, `--maxWorkers=2`: 91 green; the two red were the first test of `account.test.tsx` and one `p10.test.tsx` test timing out at 5 s while other builders loaded the machine; both files pass alone, and the `account.test.tsx` first test fails the same way with the pre-fix `router.tsx` under load, so it is a cold-start timing flake, not this change), `check-css-tokens`, `i18n-audit` (0), `check-doc-links`, `pnpm build`,
`check-entrance-isolation` and `check-bundle-budget` (ok), `playwright test --list` (937 tests in 75 files).
Not run: the full `sr-api` test suite, the authorization matrix and `immutable_rules` (integration). The matrix's
rightful actors for these routes have no second factor (`disp_pw` for the password; `/sr/auth/discord/link` is on the
skip list; the TOTP and recovery routes are undocumented), so no change is expected there.

## Visual check (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p55-signin/`)
Own stack (API 8313, Vite 5313, DB `sr_p55_signin_ui`, own sr-server build; stopped, DB dropped). Sweep
`SR_SWEEP_ONLY='^settings-user-account$'` (folder `sweep/`, desktop and phone, light and dark): unchanged page.
`e2e/tests/p55-signin-shots.spec.ts` (local, `SR_P55_SIGNIN=1`), Editorial, Lounge Light and Lounge Dark, desktop and
phone, a fresh member with TOTP each: Link Discord password prompt, the re-auth dialog over it, the refusal after
Cancel, the password card with the dialog and after Cancel, TOTP removal with the dialog, then a real code and the
"removed" state. All looked at: after the theme fix the dialog has its surface and the section's theme (Editorial
serif title), the dialog under it is dimmed, the phone layout fits. The Discord link is mocked to answer
`errors.mfa.required` (Discord is not configured on the stack); the password change and TOTP removal hit the real
server. Flows clicked on the same stack: `entrance-journey`, `passkeys`, `keyboard-2-sign-in`, `first-run` (8 passed).

## Not done / notes
- Recovery-code regeneration still needs TOTP on (`Enable two-factor authentication first.`): an account whose only
  factor is a passkey cannot regenerate its codes (it gets a batch with its first passkey). Unchanged; see question 3.
- The row says "Discord unlink stays password-free"; the route has always taken the password (D-030 era). Left as
  is (no new requirement), which matches the reasoning in the answers ("stays as is"). See question 1.
- The re-auth dialog's passkey button works the same way as for admin actions (`/sr/auth/passkeys/verify/begin`).

## Threat-model row (proposed; the commander numbers it)
"A session on a trusted device with the password alone changes how the account signs in (removes the owner's
second factor, adds the attacker's, links a Discord identity, changes the password)." Mitigation: D-573, the
second-factor proof on every such route (15-minute window per session, shared `mfa-reauth` lockout); tests above.

## Commits
`b222c881` (web), `f7c214cc` (server, docs, inventory), `78a79026` (schema rename), `5578a2ae` (OpenAPI and
client), `63cdc65f` (themed root re-auth dialog), then this handoff.

## Owner questions (defaults chosen)
1. D-573 says Discord unlink "stays password-free", but it takes the password today. I left the password check in
   place (no second factor added). OK, or should unlink drop the password?
2. The password change asks for the second factor only after the new password passed its strength checks, so
   nobody proves a factor for a password that is then refused. OK?
3. Should a passkey-only account be able to regenerate recovery codes (today: TOTP only)?
