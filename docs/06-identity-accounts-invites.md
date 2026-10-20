# 06 — Identity, Accounts, and Invites

Decisions: D-013, D-014, D-020–D-032.

## 1. Concepts

| Term | Meaning |
|---|---|
| **Account** | A login (user). Exposed as `user` in the API |
| **Person** | Staff-only record of a human. One Person can own a chain of accounts over time (deleted → new) |
| **Platform invite** | A single-use code that lets one new account be created |
| **Invite credit** | The right to have one platform invite redeemed. 5 per account (lifetime), more by purchase |
| **Server invite** | A Discord-style guild invite link. Requires an existing account |
| **Counted account** | `state = active`. Counts toward the 20,000 cap |

## 2. Account states

```
            register (invite)            user deletes             (cannot return to this account)
   ───────────────────────────► active ─────────────► deleted
                                  │  ▲
            staff deactivates     │  │ staff reactivates
                                  ▼  │
                              deactivated
                                  │
            staff platform-bans   ▼
                           platform_banned ── staff unbans ──► active (if under cap)
```

- **active**: normal.
- **deleted** (user-initiated, D-029): login is impossible forever. The profile renders as "Deleted User" with a default avatar. The username is released after 30 days. Messages remain. Friendships, server memberships, and sessions are removed. Outstanding platform invites are revoked. Unredeemed invite credits are forfeited. Account deletion requires password + 2FA re-auth and a typed confirmation.
- **deactivated** (staff-initiated, temporary): cannot log in. The profile stays as-is. Doesn't count toward the cap. Reactivation requires a free cap slot.
- **platform_banned**: cannot log in. Doesn't count toward the cap. The ban is tied to the **Person**, so any later account linked to that Person is banned automatically on linking (staff may override).

## 3. Registration flow

1. Visitor enters a code on the landing page ("Check invitation").
   - `GET /sr/invites/{code}` → `{valid: bool}`. It does not reveal the issuer or anything else. Rate-limited per IP (10/min, 50/day).
   - If the cap is reached: response `{valid:false, reason:"closed"}`. Copy: "The door's closed for now."
2. Valid → registration form: username, display name, email, password (min 10 chars, zxcvbn score ≥ 3 via `zxcvbn` Rust crate), date-of-birth **not** collected. Two confirmations, each its own unticked, required checkbox (D-311): "I've read the house notes" and "I am at least {age} years old." (p17, M-06, D-294: `accounts.minimum_age` from the `platform.compliance` settings, default 13, 13 to 18; the landing and the form say "You must be at least {age} to join"; the affirmed value is recorded in `accounts.age_declared`; no identity check).
   - **Or** "Continue with Discord": OAuth → we receive the Discord id, username, email (if granted), and avatar, which prefill the form. The user still must set username, email, and password (D-027). The Discord identity is linked.
3. `POST /sr/register {code, …}` runs in **one transaction**:
   1. `SELECT … FOR UPDATE` the invite. Must be `open`.
   2. Take advisory lock `CAP_LOCK`. Count active accounts. Reject if ≥ cap.
   3. Check the issuer's credit balance ≥ 1. If they've hit 0, the invite is invalid (issuers can never have more open codes than credits, but double-check).
   4. Person matching: if an `external_identity` (discord id) or verified email matches a previous account, reuse that `person_id` (D-030). Otherwise create a new Person.
   5. If the Person is platform-banned → reject with a generic error, and alert staff.
   6. Create the account, mark the invite `redeemed`, add ledger entries (`issuer −1`, `new account +5 initial_grant`).
4. Send an email verification link. Unverified accounts can use the platform but cannot create invites until verified.
5. Land on the Lounge home with a first-run onboarding (pick theme, join the servers the inviter suggested, if any).

## 4. Login

- Username **or** email + password → if 2FA is enabled, a TOTP/passkey/recovery-code step.
- Passkey-only login (WebAuthn discoverable credentials, user verification required, so it counts as both factors) is offered when the browser supports it. An account with a passkey is asked for a second factor on password login: the passkey, a TOTP code (when enrolled) or a recovery code. Registering the first factor issues recovery codes when the account has none. Adding and removing passkeys needs the password. A signature counter that goes backwards (a cloned authenticator) fails the login and is recorded in the platform audit log as `passkey.counter_regression`.
- "Continue with Discord": logs in if a linked active account exists. Otherwise: "No SIDE ROOM account is linked to this Discord account. You'll need an invitation." Never auto-registers.
- Failed login throttling: per account (exponential back-off after 5) and per IP.
- Sessions list in User Settings → Devices, with revoke.
- Changing the email needs the password and, when 2FA is on, a fresh second factor (TOTP, recovery code or a passkey). A second-factor check on the same session in the last 15 minutes counts, so a login with 2FA covers it. The confirmation link goes to the new address; the old address is told at once and again after completion, each time with a "This wasn't me" link that cancels a pending change or, after completion, signs every device out (D-167).
- Every login and every gateway IDENTIFY/RESUME (then at most every 10 minutes per session) stamps `accounts.last_seen_at`. It feeds member prune only and is never part of a user object (D-166).
- Forgot password: email link (single use, 30 minutes). Accounts with 2FA still need 2FA after reset.
- Changing the email address (D-167) needs the current password plus, when 2FA is enabled (TOTP, recovery code, or a passkey), a fresh second-factor proof; a successful re-prompt within the last 15 minutes counts. The verification link goes to the new address, the old address gets a notice with a "This wasn't me" link that cancels a pending change, or revokes all sessions if the change already completed. The address only changes once the new one is verified.

## 5. Platform invites

### 5.1 Creating
- **Where:** User Settings → *Invites* (new section), and a quick action in the Lounge home ("Invite someone to SIDE ROOM").
- **Rules:**
  - Account must be active, email verified, and at least 7 days old (configurable).
  - `open_codes_count < credit_balance` (D-026).
  - Cap not reached (D-014).
  - Not flagged with a suspicion level ≥ the configured "freeze" threshold.
- Each code can carry a private note ("for Sam"). The note is visible only to the issuer and staff.
- Code format `XXXXX-XXXXX` (Crockford base32). Share link: `https://<host>/join/<code>` (the landing page with the code prefilled).

### 5.2 Revoking
An issuer can revoke any `open` code. Revocation is instant. A redemption in flight that has already locked the row wins (D-024 "until the moment the code is used").

### 5.3 Credits
- Initial grant: 5 (config `invites.initial_credits`).
- Consumed only on redemption.
- **Purchase:** "Buy an invite": USD 20 (config `invites.credit_price_usd`) → BTC invoice (doc 08) → on confirmation, ledger `+1 purchase`.
- Staff can grant or revoke credits (logged).

### 5.4 Invite graph and suspicion (D-023, D-031)
- Every account (except the bootstrap Owner) has exactly one inviting edge. The graph is permanent, even across deletion.
- Admin page: an interactive graph (force-directed, collapsible subtrees) plus search, "path to root," and per-node stats (invites issued, redeemed, banned descendants).
- **Suspicion rules** (configurable):
  - When an account is platform-banned with a **serious-cause** reason category, its inviter's Person gets `suspicion_level += 1` and a staff alert.
  - `suspicion_level ≥ 2` (configurable) → the inviter's invite creation is frozen pending staff review.
  - `≥ 3` → escalated alert to senior staff/Owner.
  - Staff can clear suspicion (logged).
- Ordinary bans (non-serious categories) do not propagate.

## 6. Server invites (Lounge)

Discord-identical, with these SIDE ROOM rules:
- `/invite/:code` while **logged out** shows only: server name, icon, and member count (as Discord does), plus "Log in to accept." There is no register button and no link to the landing page (D-021). Visitors must navigate to `/` themselves.
- After login, they're returned to the invite and auto-join (honouring membership screening/onboarding).
- Invite create dialog: expiry (30 min … never), max uses (1 … ∞), temporary membership. **Default max uses = 1** when the invite is copied for sharing outside the platform (the "Copy link" action). Invites sent through in-platform DMs default to the server's configured default (D-022).
- Server invites are unaffected by the account cap.

## 7. Age gate (D-032)

The first time a user opens an age-restricted channel, title, series, or Live TV channel, a modal asks "Are you 18 or older?" with **Yes / No**.
- Yes → `age_confirmed_at = now()`. Never asked again.
- No → `age_confirmed = false` stored. Restricted content stays closed. The user can change this later in Settings → Content & Social (that path asks again).

**Library (D-184).** Age-restricted Library titles are visible to every account (browse, search, detail, feeds, follows, covers, metadata). Only *opening* one is gated: `GET /library/chapters/{id}/pages` for an age-restricted series answers 403 with code `900011` (`AGE_CONFIRMATION_REQUIRED`) while `age_confirmed_at` is null. The client then shows the same "Are you 18 or older?" dialog the Lounge uses for age-restricted channels, sends `POST /sr/age-gate {over_18:true}` on Yes, and repeats the request. The server reads `age_confirmed_at` on every request, so the retry succeeds at once and the yes is never asked again. No (or dismissing the dialog) leaves the reader closed.

**For the future Library UI.** There is no shared age dialog component in the web app yet (the Lounge interstitial is LNG-CHN-022). Build it once as a hook plus modal, for example `useAgeGate()` returning `ensureAdult(): Promise<boolean>` that opens the dialog, posts `{over_18}` and resolves with the answer, and have the Lounge channel gate and the Library reader both use it. In the reader's data layer, catch an API error whose `code === 900011`, `await ensureAdult()`, and on `true` refetch the manifest once; on `false` return to the series page. Everything else in the Library renders without the gate. Members who prefer not to see restricted titles set `library.hide_age_restricted: true` in the user settings blob (`{"library": {"hide_age_restricted": true}}`, default false), which makes `hide` the default `age` filter and removes restricted series from search, updates and follows; a request with an explicit `age=include` or `age=only` still works.

## 8. Bootstrap

`sr-cli bootstrap --owner-username <u> --owner-email <e>` creates:
1. The Owner account (prompts for a password).
2. The Staff Server (`is_staff_guild = true`) owned by the Owner, with the default staff roles from `staff_role_templates` (see doc 07; the Owner holds the Owner role and an appointment). `sr-cli staff ensure-roles` repeats that step and is safe to run again: roles are matched by name, so only missing ones are created and edits are kept (D-162).
3. The system user "SIDE ROOM" (id = 1) used for system messages.
4. Prints a one-time Owner login link.
