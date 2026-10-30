# Spec — Entrance: Landing, Login, Registration

Phase 0 · Theme `sideroom` · Mockups: `assets/mockups/entrance-landing.jpg`, `entrance-login.jpg` · Decisions: D-020–D-028, D-032.

## 1. Routes

The public routes below are served as separate small bundles, one per surface (D-385): `landing` (`/`, `/join/:code`), `auth` (login, second factor, login link, register, forgot, reset, verify-email, email-change cancel, the Discord callback), `invite` (`/invite/:code`, `/template/:code`, `/no-invitation`), and plain HTML with no script for `/house-notes`. `apps/web/public-surfaces.json` is the one list; this table is its prose. Every other path, `/help` and the handbook included, is the app and answers `302 /login?next=` to a visitor. The landing names no section of the app (O-7). See [dev/entrance-isolation.md](../dev/entrance-isolation.md).

| Route | Auth | Purpose |
|---|---|---|
| `/` | public | Landing ("The internet, with the door closed.") |
| `/join/:code` | public | Landing with the invite code prefilled and auto-checked |
| `/register` | public, requires a valid code in state | Account creation |
| `/login` | public | Member login ("Come on in.") |
| `/login/2fa` | partial session | TOTP / passkey / recovery code |
| `/forgot` and `/reset/:token` | public | Password reset |
| `/verify-email/:token` | any | Email verification |
| `/oauth/discord/callback` | public | Discord OAuth return |
| `/invite/:code` | public shell; join requires login | Server invite (see §6) |
| `/house-notes` | public, plain HTML (no script) | Short static page: what SIDE ROOM is and its house rules (operator-editable markdown) |
| `/help` | signed in (D-385; was public) | The handbook, its legal pages and What's new. A visitor's help is the collapsible "Can't sign in?" section on `/login` and the house notes; `/support` is the donation page (D-270) |
| `/no-invitation` | public | Static: "SIDE ROOM is invitation-only. Ask someone you know who's a member." No form, no waitlist |

Logged-in users who visit `/` or `/login` are redirected to their last section (default: the Lounge).

## 2. Landing page (`/`)

Layout per the mockup, with these changes: **remove "The board"** (the section strip becomes 3 columns) and drop all demo labels.

| Region | Content |
|---|---|
| Header | Door logo + `SIDE ROOM.` wordmark (left) · `A PRIVATE CORNER OF THE INTERNET` letter-spaced (center) · `Member access ↗` → `/login` (right) |
| Hero left | Eyebrow `— GOOD THINGS STAY BETWEEN US.` · H1 "The internet, / with the door / *closed.*" (italic gold on the last word) · two-line subcopy · label `YOUR INVITATION` · invite input with key icon, placeholder "Enter your invite code", button `Check invitation ↗` · helper row: lock icon "Invitation required. Always." and link `No invitation?` → `/no-invitation` |
| Hero right | Door-ajar image (operator-replaceable asset), corner tag "EST. AFTER HOURS / [door icon] / BY INVITATION ONLY", vertical text "Some doors are worth finding." |
| Hero foot | Dot + "The outside is quiet. The inside is yours." · `A glimpse inside ↓` scrolls to the section strip |
| Section strip | "One door. *A few different worlds.*" · `For members, together.` · three columns: 01 The lounge "Good company. Open conversations." · 02 The cinema "Something worth staying up for." · 03 The library "One more chapter. Then another." Columns are **not links** for logged-out visitors |
| Footer | "Not everywhere. *Just here.*" · `House notes` · `SIDE ROOM © <year>` |

All copy is operator-editable (Admin → Entrance), with defaults as above.

### Invite check behavior
- Input normalizes case, strips spaces, and auto-inserts the dash. The button is disabled until 10 valid characters are entered.
- Submit → `GET /api/v1/sr/invites/{code}`.
  - `valid` → the input turns gold-outlined, with the message "You're on the list." → navigate to `/register` after 600 ms. The code is held in memory and sessionStorage, and is never put in the URL after `/join`.
  - `invalid` → shake animation + "That invitation doesn't open this door." (one message for unknown, revoked, and used codes: no oracle).
  - `closed` (cap reached) → "The door's closed for now. We're at capacity."
  - Rate-limited → "Too many tries. Take a breath and try again later."
- Keyboard: Enter submits.

## 3. Registration (`/register`)

Same two-panel layout as login. Left panel: eyebrow `YOUR INVITATION WORKED.` and H1 "Make yourself *at home.*". Right panel form:

1. `Continue with Discord` button (prefills fields).
2. Divider "or start fresh".
3. Fields: Email · Username (live availability check, rules hint) · Display name (optional) · Password (strength meter) · Confirm password.
4. Checkbox: "I've read the house notes." (link)
5. Button `Open the door ↗`.
6. Footnote: "Your invitation is single-use. Once you're in, it's spent."

Errors appear inline. On success: auto-login → email verification banner → first-run onboarding (choose Lounge theme light/dark, optional avatar, "Want 2FA now?").

**Discord path:** after OAuth, the form shows a "Linked to Discord as @name" chip (unlink ×). Email is prefilled if Discord provided a verified one. Username is suggested from the Discord username. A password is **still required** (D-027).

## 4. Login (`/login`)

Per the mockup, with demo items removed:

| Region | Content |
|---|---|
| Header | Logo · tagline · `↖ Back to the entrance` → `/` |
| Left | Eyebrow `— YOUR USUAL PLACE.` · H1 "Good to / have you / *back.*" · "Same people. New stories. / Pick up where you left off." · footer dot "A little less noise. A little more belonging." · `MEMBER ENTRANCE / 01` |
| Right | Eyebrow `ON THE OTHER SIDE` · H1 "Come on in." · "Sign in to your corner of the internet." · `Continue with Discord ↗` · divider "or make yourself at home" · Username or email · Password (eye toggle) + `Forgot password?` · gold button `Step inside ↗` · lock row "For people already on the list. / New here? You'll need an invitation. ↗" → `/` · **Passkey** button "Use a passkey" (when the browser supports WebAuthn) |
| Removed | "Enter cinema preview" row, "Design preview · Authentication isn't connected…" footnote |
| Footer | "Not everywhere. *Just here.*" · `By invitation only` / `SIDE ROOM © <year>` |

Errors: "That doesn't match our list." (generic for both unknown user and wrong password). After lockout: "Too many attempts. Try again in N minutes."

2FA step (`/login/2fa`): six-digit input with auto-advance, "Use a passkey," "Use a recovery code," and "Trust this device for 30 days."

Discord login with no linked account → message: "No SIDE ROOM account is linked to that Discord account. You'll need an invitation."

## 5. Password reset & email verification
Standard, using Side Room styling. Reset doesn't reveal whether an email exists ("If that address is on the list, a link is on its way.").

## 6. Server invite page (`/invite/:code`)

Discord-style card on a Lounge-styled background: server icon, name, online/member counts, inviter avatar and name.
- **Logged out:** button "Log in to accept" → `/login?next=/invite/:code`. **Nothing else.** No register button and no landing link (D-021). If the code is invalid: "This invite is invalid or has expired."
- **Logged in:** "Accept invite" → join → open the server (honouring rules screening and onboarding).

## 7. Acceptance criteria
- [ ] Landing matches the mockup at 1440px and 1704px widths within the design tolerance. It reflows to a single column under 900px with the door image behind the hero.
- [ ] No route or link to "The board" exists anywhere.
- [ ] The invite check never reveals who issued a code, and gives the same message for unknown, used, and revoked codes.
- [ ] Registration is atomic with cap enforcement. 50 parallel redemptions of different codes at cap−1 produce exactly one success (integration test).
- [ ] A Discord sign-up cannot complete without a password and email.
- [ ] Logged-out `/invite/:code` offers only "Log in to accept."
- [ ] All Entrance pages score ≥ 95 on Lighthouse accessibility and work with JavaScript-driven forms only (no SSR required), with LCP < 2.5s on a 4G profile.
