# Spec — Entrance: Landing, Login, Registration

Phase 0 · Theme `sideroom` · Mockups: `docs/assets/mockups/entrance-landing.jpg`, `entrance-login.jpg` · Decisions: D-020–D-028, D-032, D-385, D-557.

## 1. Routes

The public routes below are served as separate small bundles, one per surface (D-385): `landing` (`/`, `/join/:code`), `auth` (login, second factor, login link, register, forgot, reset, verify-email, email-change cancel, the Discord callback), `invite` (`/invite/:code`, `/template/:code`, `/no-invitation`), `profile` (`/~handle`, public profiles), `guest-play` (`/play/<token>`, D-397), and plain HTML with no script for `/house-notes`, `/fonts/README.md` and `/THIRD_PARTY_NOTICES.md`, plus the error pages. `apps/web/public-surfaces.json` is the one list; this table is its prose for the entrance surfaces. Every other path, `/help` and the handbook included, is the app and answers `302 /login?next=` to a visitor. The landing names no section of the app (O-7). See [dev/entrance-isolation.md](../dev/entrance-isolation.md).

| Route | Auth | Purpose |
|---|---|---|
| `/` | public | Landing ("The internet, with the door closed.") |
| `/join/:code` | public | Landing with the invite code prefilled and auto-checked |
| `/register` | public, requires a valid code in state | Account creation |
| `/login` | public | Member login ("Come on in.") |
| `/login/2fa` | partial session | TOTP / passkey / recovery code |
| `/login/link` | public | The Owner's one-time login link (`?token=`; `sr-cli bootstrap` prints it) |
| `/forgot` and `/reset/:token` | public | Password reset |
| `/verify-email/:token` | any | Email verification |
| `/email-change/cancel` | public | The "This wasn't me" link of an email change (`?token=`) |
| `/oauth/discord/callback` | public | Discord OAuth return |
| `/invite/:code` | public shell; join requires login | Server invite (see §6) |
| `/template/:code` | public shell; preview requires login | Server template: a visitor is sent to log in and comes back; a signed-in member is handed to the app, which shows the preview |
| `/house-notes` | public, plain HTML (no script) | Short static page: what SIDE ROOM is and its house rules (operator-editable markdown) |
| `/help` | signed in (D-385; was public) | The handbook, its legal pages and What's new. A visitor's help is the collapsible "Can't sign in?" section on `/login` and the house notes; `/support` is the donation page (D-270) |
| `/no-invitation` | public | Static: "SIDE ROOM is invitation-only. Ask someone you know who's a member." No form, no waitlist |

Logged-in users who visit `/` or `/login` are redirected to the app (ENT-UI-023: a document load into `/app`, which sends them to their last section; a public surface knows no section path).

## 2. Landing page (`/`)

Layout per the mockup, with these changes: **no "The board" and no section strip at all** (the landing names no section of the app, O-7, D-385) and no demo labels.

| Region | Content |
|---|---|
| Header | Door logo + `SIDE ROOM.` wordmark (left) · `A PRIVATE CORNER OF THE INTERNET` letter-spaced (center) · `Member access ↗` → `/login` (right) |
| Hero left | Eyebrow `— GOOD THINGS STAY BETWEEN US.` · H1 "The internet, / with the door / *closed.*" (italic gold on the last word) · two-line subcopy ("A little less noise. A little more belonging." / "A place for your people, and everything you're into.") · the "doors are closed" notice while Lock the Doors is on (doc 06 §5.3a) · label `YOUR INVITATION` · invite input with key icon, placeholder "Enter your invite code", button `Check invitation ↗` · helper row: lock icon "Invitation required. Always." and link `No invitation?` → `/no-invitation` · "You must be at least {age} to join." (the platform minimum age, doc 06 §3) · the operator's landing note, if one is set |
| Hero right | Door-ajar image (operator-replaceable asset), corner tag "EST. AFTER HOURS / [door icon] / BY INVITATION ONLY", vertical text "Some doors are worth finding." |
| Hero foot | Dot + "The outside is quiet. The inside is yours." |
| Footer | "Not everywhere. *Just here.*" · `House notes` · `SIDE ROOM © <year>` |

The operator can edit (Admin → Entrance pages, `MANAGE_ENTRANCE`, behind a fresh second factor): the `/no-invitation` text, the house notes, the landing note and the hero image with its alt text. The rest of the copy is the built-in text above.

### Invite check behavior
- Input normalizes case, strips spaces and characters outside the Crockford alphabet (reading I and L as 1, O as 0), and auto-inserts the dash. The button is disabled until 10 valid characters are entered.
- Submit → `GET /api/v1/sr/invites/{code}`.
  - `valid` → the input turns gold-outlined, with the message "You're on the list." → navigate to `/register` after 600 ms. The code is held in memory and sessionStorage, and is never put in the URL after `/join`.
  - `invalid` → shake animation + "That invitation doesn't open this door." (one message for unknown, revoked, and used codes: no oracle).
  - `closed` (cap reached) → "The door's closed for now. We're at capacity."
  - `locked` (Lock the Doors) → "The doors are closed right now. Invitations are not being accepted; please check back later."
  - Rate-limited → "Too many tries. Take a breath and try again later."
- Keyboard: Enter submits.

## 3. Registration (`/register`)

Same two-panel layout as login. Left panel: eyebrow `YOUR INVITATION WORKED.` and H1 "Make yourself *at home.*". Right panel form:

1. `Continue with Discord` button (prefills fields).
2. Divider "or start fresh".
3. Fields: Email · Username (live availability check, rules hint) · Display name (optional) · Password (hint "At least 10 characters."; the server also refuses a zxcvbn score below 3) · Confirm password.
4. Two required, unticked checkboxes (D-311): "I've read the house notes." (link) and "I am at least {age} years old."
5. Button `Open the door ↗`.
6. Footnotes: "You must be at least {age} to join." and "Your invitation is single-use. Once you're in, it's spent."

Errors appear inline. On success: auto-login → email verification banner → the first-run welcome in the app (introduction, theme, optional avatar, an authenticator app, install, notifications, a server to join; doc 06 §3).

**Discord path:** after OAuth, the form shows a "Linked to Discord as @name" chip (unlink ×). Email is prefilled if Discord provided a verified one. Username is suggested from the Discord username. A password is **still required** (D-027).

## 4. Login (`/login`)

Per the mockup, with demo items removed:

| Region | Content |
|---|---|
| Header | Logo · tagline · `↖ Back to the entrance` → `/` |
| Left | Eyebrow `— YOUR USUAL PLACE.` · H1 "Good to / have you / *back.*" · "Same people. New stories. / Pick up where you left off." · footer dot "A little less noise. A little more belonging." · `MEMBER ENTRANCE / 01` |
| Right | Eyebrow `ON THE OTHER SIDE` · H1 "Come on in." · "Sign in to your corner of the internet." · `Continue with Discord ↗` · divider "or make yourself at home" · Username or email · Password (eye toggle) + `Forgot password?` · gold button `Step inside ↗` · lock row "For people already on the list. / New here? You'll need an invitation. ↗" → `/` · **Passkey** button "Use a passkey" (when the browser supports WebAuthn) · a collapsible "Can't sign in?" help section (reset the password, lost second factor, no invitation) |
| Removed | "Enter cinema preview" row, "Design preview · Authentication isn't connected…" footnote |
| Footer | "Not everywhere. *Just here.*" · `By invitation only` / `SIDE ROOM © <year>` |

Errors: "That doesn't match our list." (generic for both unknown user and wrong password). After lockout: "Too many attempts. Try again in N minutes."

2FA step (`/login/2fa`): six-digit input with auto-advance, "Use a passkey," "Use a recovery code," and "Trust this device for 30 days." Only the methods the account has are offered; they are read from the pending login (`GET /sr/auth/2fa/methods`) and never ride in a URL (D-557). Wrong answers count against the account (doc 06 §4).

A platform-banned account that signs in sees the ban notice and an appeal form (one appeal per decision, MOD-APL-004, D-215).

Discord login with no linked account → message: "No SIDE ROOM account is linked to that Discord account. You'll need an invitation." With a linked account the same second-factor rule as a password login applies (SEC-AUD-001): a bare `/login/2fa` and the pending cookie.

## 5. Password reset & email verification
Standard, using Side Room styling. Reset doesn't reveal whether an email exists ("If that address is on the list, a link is on its way. It works once, for 30 minutes.").

## 6. Server invite page (`/invite/:code`)

The preview of a server needs a session (D-385), so the public page shows a visitor nothing about it.
- **Logged out:** a generic card ("You've been invited to join a server") and the button "Log in to accept" → `/login?next=/invite/:code`. **Nothing else.** No server name, icon or counts, no register button and no landing link (D-021).
- **Logged in:** the public page hands the member to the app (`/app?goto=/invite/:code`), which shows a Discord-style card on a Lounge-styled background: server icon (or its initial), name and "{online} online · {members} members", with "Accept invite" → join → open the server (honouring rules screening and onboarding). If the code is invalid: "This invite is invalid or has expired."

## 7. Acceptance criteria
- [ ] Landing matches the mockup at 1440px and 1704px widths within the design tolerance. It reflows to a single column under 900px with the door image behind the hero. _(built: the 900px reflow in `apps/web/src/components/entrance.css` and the snapshot `entrance-landing` in `e2e/tests/visual.spec.ts`; left: no recorded comparison with the mockup at 1440px and 1704px, and the mockup's section strip was removed on purpose)_
- [x] No route or link to "The board" exists anywhere. _(no `/board` route in `apps/web/src`; `apps/web/src/routes/entrance/Landing.tsx` has no section strip)_
- [x] The invite check never reveals who issued a code, and gives the same message for unknown, used, and revoked codes. _(`crates/sr-api/src/routes/auth/invites.rs`, test `invite_check_valid_invalid_and_no_oracle` in `routes/auth/tests.rs:313`)_
- [x] Registration is atomic with cap enforcement. 50 parallel redemptions of different codes at cap−1 produce exactly one success (integration test). _(`crates/sr-db/tests/registration.rs:180`)_
- [x] A Discord sign-up cannot complete without a password and email. _(`POST /sr/register` validates both whether or not a Discord link is pending: `crates/sr-api/src/routes/auth/register.rs`)_
- [x] Logged-out `/invite/:code` offers only "Log in to accept." _(`apps/web/src/routes/entrance/ServerInvite.tsx`)_
- [ ] All Entrance pages score ≥ 95 on Lighthouse accessibility and work with JavaScript-driven forms only (no SSR required), with LCP < 2.5s on a 4G profile. _(built: the focus check after a refused code in `apps/web/src/routes/entrance/landing.a11y.test.tsx`, `e2e/tests/keyboard-1-landing.spec.ts` and the surface budgets in `apps/web/public-surfaces.json`; left: no Lighthouse score, axe run on the entrance pages or LCP measurement is recorded)_
