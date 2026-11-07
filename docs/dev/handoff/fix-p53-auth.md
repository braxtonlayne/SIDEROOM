# fix/p53-auth (wave 53: auth, sessions, gateway)

Audit fixes from `audit-p53/` (SUMMARY rows 1, 2, 10 and the unverified group-DM HIGH). Base: main `491aad85`.

## Done
- **SEC-AUD-001: Discord sign-in skipped a passkey-only second factor.** The password login's rule is now one function,
  `login::second_factor_step` (TOTP or passkey, unless the device is trusted), and the Discord callback's sign-in
  (`discord::sign_in_linked`) uses it. A Discord sign-in that needs the second factor redirects to
  `/login/2fa?methods=passkey,recovery[&next=...]` with the pending `sr_mfa` cookie; the 2FA page reads `methods` from the
  URL (a server redirect has no sessionStorage hop), so a passkey-only account gets the passkey step. "Trust this device"
  now also skips the step after Discord, like after a password. Tests: `crates/sr-api/src/routes/auth/tests.rs`
  `discord_sign_in_asks_a_passkey_only_account_for_its_second_factor`; vitest `routes/entrance/passkeys.test.tsx`
  "offers the passkey after a Discord sign-in". Both fail without the fix.
- **SEC-AUD-002: revocation never reached a live gateway socket.** New bus event `DomainEvent::SessionsRevoked {user}`
  (user topic, never sent to clients). Published after logout, logout-all, `DELETE /sr/auth/sessions/{id}`, password reset
  by mail, password change with "sign out other sessions", the email-change revoke link (sr-api), and staff suspend,
  force-logout, reset-2FA and force-password-reset (sr-admin). The gateway asks every socket of that user to re-check its
  credential at once (`Out::Recheck`); a socket whose token/cookie no longer authenticates (revoked or expired session,
  account not active, reset bot token) closes with **4004**; the account's other live sessions and every other account
  stay. Backstop: each identified socket re-checks every 60 s (`GatewayConfig::session_recheck`), which also covers the
  revocations that publish nothing (Person ban, the moderation sweep, account deletion). A backend error during a
  re-check keeps the socket. Web: the gateway's 4004 now calls the app's unauthorized handler (the same exit as a REST
  401: it asks `/users/@me`, then leaves for `/login`). Tests: `crates/sr-gateway/src/session_revoke_tests.rs` (revoked
  session closes, the others stay; logout-all closes all of the account only; periodic re-check without an event; a
  revoked session cannot RESUME), `tests.rs::ending_sessions_tells_the_gateway` (sr-api),
  `suspend_and_force_logout_tell_the_gateway` (sr-admin), vitest `store/connect.test.ts`.
- **SEC-AUD-010: `/gateway` had no message size limit, and unauthenticated sockets were uncapped.** Confirmed both: the
  cap counted sessions only, and a pre-IDENTIFY socket could heartbeat forever (op 1 reset the deadline without a
  session). Now: `max_message_size` and `max_frame_size` = `MAX_MESSAGE` = 256 KiB (largest client op is an op 101
  commit: `data` + `welcome` of up to 96 KiB each + up to 100 recipients, under 200 KiB; the constant's comment says
  so). A socket must IDENTIFY/RESUME within 30 s (`identify_timeout`; heartbeats do not extend it; 4009). At most 32
  unidentified sockets per client address (`max_unidentified_per_ip`; HTTP 429 at the upgrade). The sr-server cap counts
  `session_count() + unidentified_count()`, and its middleware hands the gateway the client address
  (`sr_gateway::PeerIp`, from `sr_api::rate_limit::client_ip`, so X-Forwarded-For when the proxy is trusted). Tests:
  `crates/sr-gateway/src/limits_tests.rs` (a real axum listener over TCP: a 200 KiB message is answered, one byte over
  256 KiB closes the socket unanswered; heartbeats do not keep an unidentified socket; per-address cap).
- **Group-DM removal kept the member in the call (verified, real; ID `SEC-AUD-GDM` until the commander numbers it).**
  `remove_recipient` only sent `CHANNEL_DELETE` / `CHANNEL_RECIPIENT_REMOVE`; the gateway had no handler, `call_members`
  stayed stale, and the removed member stayed connected with a live SFU session and MLS leaf. Now the gateway sees the
  `CHANNEL_DELETE` on the removed user's topic, checks membership (`dm_call_members`), and if they are no longer in the DM:
  `voice_leave` (SFU `Disconnect`, `VOICE_STATE_UPDATE` leave, the MLS Remove proposal through `e2ee_member_left`, call
  end if they were last) and drops them from the call's member list and ring. Closing a one-to-one DM keeps the
  membership, so nothing happens then. Tests: `voice_ext_tests.rs`
  `removing_a_member_from_a_group_dm_drops_them_from_its_call`, `closing_a_dm_keeps_its_member_in_the_call`.
- Docs: `docs/15-threat-model.md` T2 (revocation reaches the socket; Discord second factor), T16 (group-DM removal),
  T17 (gateway size and unidentified caps); `docs/05-api-and-gateway.md` 6.4 "Limits and revocation". No new T row.
- Sweep: new row `pub-login-2fa-passkey` (`/login/2fa?methods=passkey,recovery`).

## Not done / notes
- Commits: `c0984af4` (SEC-AUD-001), `c20cfc64` (SEC-AUD-002 and SEC-AUD-010 together: they share the `GatewayConfig`
  and `conn.rs` hunks), `46749b51` (group DM), `171cf3f9` (docs), and this handoff.
- No migrations, no route or OpenAPI changes, no new dependencies (sr-gateway dev-dependencies only gained features:
  tokio `net`/`io-util`, axum `http1`/`tokio`, for the real-socket size test).
- Not covered by the bus event (the 60 s re-check catches them): a Person ban (its outcome has no account ids), the
  moderation 30-day sweep (`sr-moderation/src/subtree.rs`), account deletion, `sr-cli user reset-2fa`.
- The group-DM fix trusts `dm_call_members`; on a database error that returns `None`, so the member would be dropped
  from the call (only when a `CHANNEL_DELETE` for that DM arrives).
- The 4008 close in `run_connection` for the per-address cap is the in-process path; over HTTP it is a 429 before the
  upgrade.

## Tests run
Rust: `cargo fmt --all -- --check`; `cargo test -p sr-events -p sr-gateway -p sr-admin` (16 / 216 / 75 passed);
`cargo test -p sr-api --lib routes::auth` (105 passed); `cargo clippy -j 4 -p sr-events -p sr-gateway -p sr-admin -p sr-api
-p sr-server --all-targets -- -D warnings` clean. Web: tsc, eslint + prettier on changed files, vitest
`src/routes/entrance src/store src/gateway src/public` (26 files, 233 tests), css-tokens, i18n-audit (0).
`node --test scripts/threat-model.test.mjs`, `check-doc-links`. `playwright test --list` loads all 72 files. The new web
passkey test was run against the old `TwoFactor.tsx` and fails there; the Rust tests assert behaviour that did not exist.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p53-auth/`)
Sweep with `SR_SWEEP_ONLY='^pub-login-2fa'` against the prebuilt main server and this worktree's Vite (ports 8283/5283,
DB `sr_p53_auth`, dropped afterwards): `pub-login-2fa` and the new `pub-login-2fa-passkey`, desktop and phone, light and
dark account themes (the entrance has one look). Looked at: the passkey page leads with "Your passkey." and "Use your
passkey" and offers only the recovery code; the plain page is unchanged (code boxes). No layout change.

## Owner questions (defaults chosen, Discord-like)
1. Gateway re-check interval 60 s, IDENTIFY timeout 30 s, 32 unidentified sockets per address, 256 KiB message cap. Are
   these the right figures? (They are `GatewayConfig` fields, not yet in `sr.toml`.)
2. A revoked socket closes with 4004 (fatal for the web client, which then signs out). Discord uses 4004 for
   "authentication failed"; fine to reuse, or a SIDE ROOM-specific 4xxx "session revoked"?
3. After a Discord sign-in the 2FA methods ride in the URL (`?methods=passkey,totp,recovery`): it says which factors the
   account has to whoever sees that URL in the browser history. The password path keeps them in sessionStorage. OK, or
   add a `GET /sr/auth/2fa/methods` that reads the pending `sr_mfa` cookie instead?
4. A removed group-DM member is dropped from the call; should the others get a `CALL_UPDATE` with the shortened ring list
   (Discord sends one)? Not sent now.
