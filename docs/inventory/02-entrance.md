# 02 — Entrance (`ENT`)

Phase 0. Spec: [entry-landing-and-login](../specs/entry-landing-and-login.md), doc 06.

## ENT-AUTH — Backend identity
- [ ] **ENT-AUTH-001** [BE] Account model + repository (create, get by username/email/id, state transitions)
- [ ] **ENT-AUTH-002** [BE] Username rules + availability check endpoint (debounced)
- [ ] **ENT-AUTH-003** [BE] argon2id hashing (tuned params, rehash on login if params changed)
- [ ] **ENT-AUTH-004** [BE] Password strength validation (`zxcvbn` crate, min length 10)
- [ ] **ENT-AUTH-005** [BE] Session create/list/revoke, rolling expiry, device naming from UA
- [ ] **ENT-AUTH-006** [BE] CSRF token issuance + verification
- [ ] **ENT-AUTH-007** [BE] Login endpoint (username or email) with generic error + throttling per account/IP
- [ ] **ENT-AUTH-008** [BE] TOTP enrol/verify/disable (`totp-rs`), QR provisioning URI
- [ ] **ENT-AUTH-009** [BE] Recovery codes generate/regenerate/consume
- [ ] **ENT-AUTH-010** [BE] WebAuthn passkeys: register, list, rename, delete, login (discoverable), 2FA use (`webauthn-rs`)
- [ ] **ENT-AUTH-011** [BE] Discord OAuth2: authorize redirect, callback, state/PKCE, profile fetch, link/unlink
- [ ] **ENT-AUTH-012** [BE] Email verification tokens + send (`lettre`), resend with cooldown
- [ ] **ENT-AUTH-013** [BE] Forgot/reset password flow (30 min single-use token, no account oracle)
- [ ] **ENT-AUTH-014** [BE] Email templates (verify, reset, new login alert, 2FA changed) in Side Room style (MJML-like HTML + text)
- [ ] **ENT-AUTH-015** [BE] New-device login alert email (toggle)

## ENT-INV — Platform invites
- [ ] **ENT-INV-001** [BE] Invite code generator (Crockford base32 10 chars, dash formatting, normalization)
- [ ] **ENT-INV-002** [BE] `GET /sr/invites/{code}` validity check (valid/invalid/closed), per-IP rate limits, no issuer leak
- [ ] **ENT-INV-003** [BE] Create invite (eligibility: active, verified, age ≥ N days, open < credits, cap not reached, not frozen)
- [ ] **ENT-INV-004** [BE] Revoke invite (race-safe vs redemption)
- [ ] **ENT-INV-005** [BE] Credits ledger + balance query + initial grant on registration
- [ ] **ENT-INV-006** [BE] Account cap service: counted accounts query, advisory lock, `is_closed()`
- [ ] **ENT-INV-007** [BE] `POST /sr/register` transactional redemption (invite lock, cap check, issuer credit −1, Person matching, banned-Person rejection)
- [ ] **ENT-INV-008** [BE] Person matching rules (Discord id, verified email) + staff alert on banned Person attempt
- [ ] **ENT-INV-009** [GW] `SR_INVITE_CREDITS_UPDATE` dispatch to issuer on redemption
- [ ] **ENT-INV-010** [TEST] Concurrency test: 50 parallel redemptions at cap−1 → exactly one success

## ENT-UI — Pages
- [ ] **ENT-UI-001** [FE] Landing header (logo, tagline, "Member access ↗")
- [ ] **ENT-UI-002** [FE] Landing hero left: eyebrow, H1 with italic "closed.", subcopy
- [ ] **ENT-UI-003** [FE] Invite input (key icon, auto-format, uppercase, validation, Enter submit)
- [ ] **ENT-UI-004** [FE] "Check invitation ↗" button with loading/success/error states (shake animation)
- [ ] **ENT-UI-005** [FE] Helper row: lock "Invitation required. Always." + "No invitation?" link
- [ ] **ENT-UI-006** [FE] Hero right: door image, corner badge, vertical tagline, hero-foot strip + "A glimpse inside ↓" smooth scroll
- [ ] **ENT-UI-007** [FE] Section strip "One door. *A few different worlds.*" with 3 columns (lounge/cinema/library), no Board
- [ ] **ENT-UI-008** [FE] Footer (tagline, House notes, ©)
- [ ] **ENT-UI-009** [FE] `/join/:code` prefill + auto-check
- [ ] **ENT-UI-010** [FE] `/no-invitation` and `/house-notes` static pages (operator markdown)
- [ ] **ENT-UI-011** [FE] Register page layout (left editorial panel, right form)
- [ ] **ENT-UI-012** [FE] Register form: email, username (live check), display name, password + strength meter, confirm, house-notes checkbox, submit
- [ ] **ENT-UI-013** [FE] Register with Discord: OAuth start, return handling, linked chip with unlink, prefill
- [ ] **ENT-UI-014** [FE] Login page layout per mockup (left "Good to have you *back.*", right form)
- [ ] **ENT-UI-015** [FE] Login form: Continue with Discord, divider, username/email, password (eye), Forgot password link, "Step inside ↗"
- [ ] **ENT-UI-016** [FE] "Use a passkey" button (feature-detect WebAuthn conditional UI)
- [ ] **ENT-UI-017** [FE] "For people already on the list / New here? You'll need an invitation ↗" row
- [ ] **ENT-UI-018** [FE] 2FA step page: 6-digit input auto-advance, passkey option, recovery code option, trust device
- [ ] **ENT-UI-019** [FE] Forgot password + reset pages
- [ ] **ENT-UI-020** [FE] Verify-email landing + "verify your email" banner in app
- [ ] **ENT-UI-021** [FE] Lockout / rate-limit / closed-door messages
- [ ] **ENT-UI-022** [FE] First-run onboarding modal (theme, avatar, 2FA nudge, suggested servers)
- [ ] **ENT-UI-023** [FE] Logged-in redirect from `/` and `/login` to last section
- [ ] **ENT-UI-024** [FE] Responsive layouts < 900px (stacked hero, door as background)
- [ ] **ENT-UI-025** [FE] Operator-editable copy loading (Admin → Entrance) with defaults
- [ ] **ENT-UI-026** [TEST] Playwright: invite → register → verify → logout → login with TOTP → passkey login
