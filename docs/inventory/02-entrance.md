# 02 — Entrance (`ENT`)

Phase 0. Spec: [entry-landing-and-login](../specs/entry-landing-and-login.md), doc 06.

## ENT-AUTH — Backend identity
- [ ] **ENT-AUTH-001** [BE] Account model + repository (create, get by username/email/id, state transitions)
- [x] **ENT-AUTH-002** [BE] Username rules + availability check endpoint (debounced)
- [x] **ENT-AUTH-003** [BE] argon2id hashing (tuned params, rehash on login if params changed)
- [x] **ENT-AUTH-004** [BE] Password strength validation (`zxcvbn` crate, min length 10)
- [x] **ENT-AUTH-005** [BE] Session create/list/revoke, rolling expiry, device naming from UA
- [x] **ENT-AUTH-006** [BE] CSRF token issuance + verification
- [x] **ENT-AUTH-007** [BE] Login endpoint (username or email) with generic error + throttling per account/IP
- [x] **ENT-AUTH-008** [BE] TOTP enrol/verify/disable (`totp-rs`), QR provisioning URI
- [x] **ENT-AUTH-009** [BE] Recovery codes generate/regenerate/consume
- [x] **ENT-AUTH-010** [BE] WebAuthn passkeys: register, list, rename, delete, login (discoverable), 2FA use (pure-Rust verifier in `sr-auth::webauthn` per D-161, migration 0180, `routes/auth/passkeys.rs`; the Playwright run with the CDP virtual authenticator passes; see `docs/dev/handoff/feat-p4-passkeys.md`)
- [ ] **ENT-AUTH-011** [BE] Discord OAuth2: authorize redirect, callback, state/PKCE, profile fetch, link/unlink
- [x] **ENT-AUTH-012** [BE] Email verification tokens + send (`lettre`), resend with cooldown
- [x] **ENT-AUTH-013** [BE] Forgot/reset password flow (30 min single-use token, no account oracle)
- [ ] **ENT-AUTH-014** [BE] Email templates (verify, reset, new login alert, 2FA changed) in Side Room style (MJML-like HTML + text)
- [ ] **ENT-AUTH-015** [BE] New-device login alert email (toggle)

## ENT-INV — Platform invites
- [x] **ENT-INV-001** [BE] Invite code generator (Crockford base32 10 chars, dash formatting, normalization)
- [x] **ENT-INV-002** [BE] `GET /sr/invites/{code}` validity check (valid/invalid/closed), per-IP rate limits, no issuer leak
- [ ] **ENT-INV-003** [BE] Create invite (eligibility: active, verified, age ≥ N days, open < credits, cap not reached, not frozen)
- [x] **ENT-INV-004** [BE] Revoke invite (race-safe vs redemption) _(`PgAuthStore::revoke_member_invite` revokes only an `open` code of the issuer in one guarded `UPDATE`, so a redemption that already locked the row wins; test `member_invites.rs`)_
- [x] **ENT-INV-005** [BE] Credits ledger + balance query + initial grant on registration
- [ ] **ENT-INV-006** [BE] Account cap service: counted accounts query, advisory lock, `is_closed()`
- [x] **ENT-INV-007** [BE] `POST /sr/register` transactional redemption (invite lock, cap check, issuer credit −1, Person matching, banned-Person rejection)
- [ ] **ENT-INV-008** [BE] Person matching rules (Discord id, verified email) + staff alert on banned Person attempt
- [ ] **ENT-INV-009** [GW] `SR_INVITE_CREDITS_UPDATE` dispatch to issuer on redemption
- [ ] **ENT-INV-010** [TEST] Concurrency test: 50 parallel redemptions at cap−1 → exactly one success

## ENT-UI — Pages
- [x] **ENT-UI-001** [FE] Landing header (logo, tagline, "Member access ↗")
- [x] **ENT-UI-002** [FE] Landing hero left: eyebrow, H1 with italic "closed.", subcopy
- [x] **ENT-UI-003** [FE] Invite input (key icon, auto-format, uppercase, validation, Enter submit)
- [x] **ENT-UI-004** [FE] "Check invitation ↗" button with loading/success/error states (shake animation)
- [x] **ENT-UI-005** [FE] Helper row: lock "Invitation required. Always." + "No invitation?" link
- [x] **ENT-UI-006** [FE] Hero right: door image, corner badge, vertical tagline, hero-foot strip + "A glimpse inside ↓" smooth scroll
- [x] **ENT-UI-007** [FE] Section strip "One door. *A few different worlds.*" with 3 columns (lounge/cinema/library), no Board
- [x] **ENT-UI-008** [FE] Footer (tagline, House notes, ©)
- [x] **ENT-UI-009** [FE] `/join/:code` prefill + auto-check
- [ ] **ENT-UI-010** [FE] `/no-invitation` and `/house-notes` static pages (operator markdown) _(Default pages now have rules, privacy, terms, image credits and a Help page; operator-editable markdown is ENT-UI-025.)_
   - p13 backend half built: `GET /sr/entrance/copy` (ETag, max-age 60) and `PUT|DELETE /admin/entrance/copy/{key}` (keys `no-invitation`, `house-notes`, `landing-note`) (`routes/admin/entrance.rs`, migration 0843, test `entrance_copy_is_public_to_read_and_staff_to_write`).
- [x] **ENT-UI-011** [FE] Register page layout (left editorial panel, right form)
- [x] **ENT-UI-012** [FE] Register form: email, username (live check), display name, password + strength meter, confirm, house-notes checkbox, submit
- [x] **ENT-UI-013** [FE] Register with Discord: OAuth start, return handling, linked chip with unlink, prefill
- [x] **ENT-UI-014** [FE] Login page layout per mockup (left "Good to have you *back.*", right form)
- [x] **ENT-UI-015** [FE] Login form: Continue with Discord, divider, username/email, password (eye), Forgot password link, "Step inside ↗"
- [x] **ENT-UI-016** [FE] "Use a passkey" button (feature-detect WebAuthn conditional UI)
- [x] **ENT-UI-017** [FE] "For people already on the list / New here? You'll need an invitation ↗" row
- [x] **ENT-UI-018** [FE] 2FA step page: 6-digit input auto-advance, passkey option, recovery code option, trust device
- [x] **ENT-UI-019** [FE] Forgot password + reset pages
- [ ] **ENT-UI-020** [FE] Verify-email landing + "verify your email" banner in app
- [x] **ENT-UI-021** [FE] Lockout / rate-limit / closed-door messages
- [ ] **ENT-UI-022** [FE] First-run onboarding modal (theme, avatar, 2FA nudge, suggested servers)
- [x] **ENT-UI-023** [FE] Logged-in redirect from `/` and `/login` to last section
- [x] **ENT-UI-024** [FE] Responsive layouts < 900px (stacked hero, door as background)
- [ ] **ENT-UI-025** [FE] Operator-editable copy loading (Admin → Entrance) with defaults
   - p13 backend half built: see ENT-UI-010; the Admin editor (ADM-PLT-003) and the client loader are not wired.
- [ ] **ENT-UI-026** [TEST] Playwright: invite → register → verify → logout → login with TOTP → passkey login
