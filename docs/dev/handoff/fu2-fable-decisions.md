# Follow-up wave 2 decisions (Fable, owner-delegated, 2026-11-02), for the follow-up integrator

Scope: every question in `fu2-owner-questions.md` (the defaults already applied on `integration/fu`, worktree `fu-ui`). Rules applied: `docs/01-decisions-log.md` wins where it already decides (FU-1 to FU-12 and P30-VIS-1 included); Discord parity by default; WCAG AA; D-385 (nothing app-side before sign-in); E2EE, authorization and the audit logs are never toggleable; no download or new dependency (those are owner-only); the arena3d replay hash `6fdea5719b5383d6` / `e35977c2a4cddeed` stays unchanged, and no decision below accepts a protocol or replay change. Code read at `integration/fu` (`f45a36de`), nothing edited.

Key findings that change the picture:
- The crash list already fetches the card: `SELECT_ROWS_LIST` in `crates/sr-db/src/crash_reports.rs` returns `card` (only `replay` is `'[]'`), and `CrashRow.card` reaches `summary()` in `crates/sr-api/src/routes/crash/mod.rs`. So `top_frames` is a few lines in sr-api with no database change, and the UI column (`CrashReportsSection.tsx` already reads `r.top_frames`) lights up by itself.
- The app learns the account id only on the gateway `READY` (`store/reducer.ts` sets `currentUserId`); `AppDoor` redirects in its first effect. A `lastSection` key per account id would make `/app` wait on the socket. The sign-out clear (`f45a36de`) covers the shared-browser case; what it misses is the session that ends without a sign-out (`setUnauthorizedHandler` in `apps/web/src/app/main.tsx` leaves through `exitToLogin` and clears nothing).
- `packages/profile-blocks` is shared with the public `/~handle` bundle (`apps/web/src/public/profile/ALLOWED.ts` lists `@sideroom/profile-blocks`), so a QR import inside it would land in the public build and fail the entrance-isolation check. The package already has a host-filled seam (`BlocksEnv` in `context.tsx`, `BlockSlots` in `types.ts`); a QR renderer passed through it keeps the public surface text-only with no new import.
- The only `/games/*` routes are `/games` and `/games/emulator` (`apps/web/src/emu/routes.tsx`); Activity instances live under `/channels/...`. Remembering the emulator page risks nothing.
- The arena3d GPU notice is already `role="status"` (`client/index.html`), so the English literals meet WCAG 4.1.3 as they are.

Format per question: id, the decision, why, and for a change what the implementing agent does.

## 1. Crash reports admin (OPS-CRASH-002)

### C1: a top-frames column without an audited read
**Change: add `top_frames` to `CrashSummary` (Rust, S, no migration).** A triage list that shows a fingerprint hash and no code location is unusable without opening every row, and opening a row is the audited replay read (D-433); the frames are already in the Staff Server card every staff member reads, and the list query already carries the card, so nothing new is read from the database and nothing is audited that was not.

Implementation:
- `crates/sr-api/src/routes/crash/mod.rs`: `CrashSummary` gains `pub top_frames: Vec<String>` (ToSchema); `summary()` builds it from `r.card["frames"]`, the first 3 entries, each `"{fn} {file}:{line}"` (`"{file}:{line}"` when `fn` is absent), the same strings `allow_list.rs` already produced (closed character sets, no free text). An entry that is not an object is skipped.
- Unit tests beside the two `#[test]`s at the bottom of `mod.rs`: a card with five frames gives three strings in order; a card without `frames` gives an empty vector; a frame without `fn` gives `file:line`.
- `crates/sr-db` unchanged; `route_matrix.tsv` unchanged (same route, same guard). Regenerate OpenAPI, the api-client and the MSW handlers; the web type `top_frames?: string[]` in `apps/web/src/admin/crash/api.ts` becomes required (`string[]`).
- Vitest `crashReports.test.tsx`: the seeded rows carry frames, so assert the column renders and the search matches a frame string.
- Re-shoot `admin-crash-reports-{desktop,phone}-{light,dark}` after the merge (the column changes the layout; check the phone width still fits, see `crash.css`).

### C2: search and filter client-side over the 200 most recent groups
**Keep.** The table keeps 2000 groups (D-431) but triage works on what was seen last; the `#crash-triage` channel holds every card and is searchable like any channel. Server-side query parameters are a later item if an instance ever has more than 200 live groups; the "showing 200" note stays.

### C3: a 429 keeps the queued card
**Keep.** A 429 is the per-account or platform window (D-432) and says nothing about the card; the retry is bounded by the queue (10 cards, 7 days) and the 5-per-page-load limit, so a crash loop cannot amplify. Dropping it would lose the one report of a rare crash because another crash was busy.

### C4: the page in Admin > Overview for every `STAFF_BASIC` holder
**Keep.** D-433 lets any appointed staff member read the two routes, and the Audit group's entries all need a stronger permission (`VIEW_ACCESS_LOG`, `READ_PRIVATE_CONTENT`); putting a `STAFF_BASIC` page among them would be the one entry in that group with a different rule. The Overview group is the "anyone appointed" group.

### C5: cards dropped from the queue (full, expired) are dropped silently
**Keep.** A card drops only when ten newer crashes are queued or it is seven days old, and the server dedupes by fingerprint anyway, so the lost information is a count, not a crash. Telling staff would need a count field in the report, and the allow-list on both ends (D-430) is the reason the format is closed; the browser's crash ring buffer still holds the card for the member.

## 2. Arena 3D (ACT-A3D-002)

All ten defaults stand; the replay hash and `PROTOCOL_VERSION` are untouched, and nothing below accepts a protocol or replay change.

### A1: Crouch toggles, Jump is held
**Keep.** With one thumb on the stick and the other looking, a held crouch would cost the look thumb; mobile shooters (Call of Duty Mobile, PUBG Mobile) use a crouch toggle for the same reason. `aria-pressed` on the button says which state it is in.

### A2: the 8-way stick
**Keep.** An analogue move vector changes `sim/input.js`, the wire format and the replay, which the binding rule forbids without an explicit decision; nothing today needs it (walking speed is the same for everyone, so a phone player is not slower than a keyboard). Analogue movement, if ever wanted, is listed below as owner-only because it is a protocol and replay change.

### A3: touch look = mouse sensitivity x 1.8, no separate slider
**Keep.** One sensitivity setting with a fixed scale is one number for the player to tune; a second slider for the same thing is a settings page nobody asked for. If real-phone testing finds 1.8 wrong, `TOUCH_LOOK_SCALE` in `client/touch.js` is the one constant.

### A4: no pause on touch; Menu opens over the live match
**Keep.** A match with other players never pauses anywhere: on desktop Esc only drops pointer lock and the player stands still. Touch does the same (`toMenu` clears the held buttons and the stick), so the two inputs behave alike, which is what the replay tests rely on.

### A5: the new host is whoever the platform names
**Keep.** D-444 decides the successor (longest-connected, ties by user id); a game re-checking the platform's choice would be a second authority on one question.

### A6: the new host loses its last few ticks; the old host's body vanishes
**Keep.** The rewind is at most three ticks plus one trip (`net/handoff.js`), the same rewind every peer already takes on a correction. A grace-period body would be an un-owned statue that blocks a doorway and collects hits.

### A7: a hidden host with nobody sending catches up at most 5 s per throttled call
**Keep.** A hidden host with nobody else connected has nobody who can see it run behind; the cap is what stops a returning tab from spiralling (300 ticks a burst). A pause rule would add a state the handoff and pacer tests do not cover, for a case with no witness.

### A8: WebGPU retried once, then WebGL2; a second loss within 30 s goes straight to WebGL2
**Keep.** One retry tells a transient loss from a flaky driver; `REPEAT_LOSS_MS` is the single knob if a real device argues.

### A9: `host:seats` as JSON beside the binary roster
**Keep.** It is 300 bytes read defensively (`decodeSeats` refuses anything over 4 KiB or malformed), leaves `PROTOCOL_VERSION` and the replay as they are, and a binary roster v2 is the protocol bump the binding rule says not to take by default.

### A10: English literals for the GPU notice
**Keep.** The game has no i18n at all; one translated string in a page of English would be stranger than none. The element is `role="status"` already, so the notice is announced (WCAG 4.1.3).

## 3. Decisions builder (FU-ENT-4, FU-BETA-1, FU-STU-5, FU-ARC-5)

### F1: `lastSection` cleared on sign-out; also key it by account id?
**Keep per browser with the sign-out clear, and also clear it when a session ends without a sign-out (S, web only).** The account id is known only after the gateway `READY`, so a per-account key would make `/app` wait on the socket or add a fetch for a case the clear already covers; the gap is a session that expires or is revoked while the app is open (`main.tsx`'s unauthorized handler leaves through `exitToLogin` and keeps the path), after which the next account on that browser inherits it. FU-10's "per account" is met once both exits clear.

Implementation:
- `apps/web/src/app/main.tsx`: in `setUnauthorizedHandler`, call `forgetSection()` (from `../state/session`) before `exitToLogin()`. Not in `lib/enterApp.ts`: that module is shared with the public surfaces and must not import the app store.
- Vitest: a test beside `session.test.ts` (or in the app boot tests) that the unauthorized path forgets the section; the existing sign-out test stays.

### F2: `/games/emulator` and other `/games/*` subpaths not remembered
**Change: remember `/games/emulator` (S, web only).** It is a page like `/study`, not an Activity instance (those live under `/channels`), and a member who left in the emulator expects it back (Discord parity: the last place). No other `/games/*` route exists, so the rule stays an allow-list.

Implementation: `apps/web/src/state/session.ts`, `SECTION` regex: `games` becomes `games(?:\/emulator)?`; `session.test.ts`: `/games/emulator` is remembered, `/games/anything-else` and `/games/emulator?rom=x` are not. `rememberSection` and `AppDoor` unchanged.

### F3: Caddy proxies `/~*` only with a session cookie
**Keep.** Anonymous visitors get the static public shell from Caddy and never reach `sr-server`, which is the privacy-preserving and cheaper shape (no app-server log line per crawler hit) and exactly D-385's spirit; a signed-in member gets the full page from `sr-server`, and a stale cookie falls back to the shell (`handle_response` on 404). FU-11's wording ("`sr-server` serves the public page without a session") is met in effect: the public page is the same shell either way. A row below records the cookie gate so the two never read as a contradiction. The integrator still runs `caddy validate` on `Caddyfile.prod` (the fu-fable handoff could not).

### F4: Bitcoin and Lightning QR in the signed-in profile view
**Change: draw them in the signed-in profile view through a host-filled slot, never inside `profile-blocks` (S, web only).** FU-4 decides "the signed-in profile page", and the editor's Inspector alone leaves the page a visitor actually reads without the QR; a tipper scans the view, not the owner's editor. The encoder must not enter `profile-blocks` (it is in the public bundle), so the package only gains an optional renderer the app supplies.

Implementation:
- `packages/profile-blocks/src/context.tsx`: `Env` gains `tipQr?: (uri: string, label: string) => ReactNode`; `BlocksEnv` passes it through. `blocks.tsx` `Linktree`: for a `pay` entry, render `tipQr(l.href, l.label || l.href)` above the `<code className="pp-tipcode">` when the function is given; nothing else changes. `profile-blocks.css`: a `.pp-tipqr` block (max 160 px, centred, `margin-block: 8px`).
- `apps/web/src/profilepage/ProfileRenderer.tsx`: pass `tipQr={compact ? undefined : (uri, label) => <QrBox value={uri} label={label} />}` (`QrBox` from `apps/web/src/tips/QrBox.tsx`, size 160, ecc L, omitted when the payload exceeds the encoder, as it already does). The mini popout (`compact`) keeps text only (space).
- `apps/web/src/public/profile/PublicProfile.tsx`: unchanged, passes no `tipQr`; `ALLOWED.ts` unchanged. `allowed.test.ts` and `check-entrance-isolation.mjs` prove `util/qr` is still absent from the public closure.
- Tests: `packages/profile-blocks/src/blocks.test.tsx` (tip entries): without `tipQr` the markup is as today; with a recording `tipQr` it is called once per tip entry with the URI and not for the Ko-fi or https entries. `apps/web/src/profilepage/__tests__/prf-pro.test.tsx`: `ProfilePageView` with a `bitcoin:` link renders one `svg` with the QR label; `MiniProfile` renders none.
- Re-shoot `profile-member` and `profile-u` (desktop/phone, light/dark) and look at the Links block at phone width.

### F5: the PS1 HLE notice only while a game runs; the BIOS panel always hints when no BIOS is loaded
**Keep.** FU-1 asks the shell to say when a game is running on HLE and to offer the panel; a notice with no game running would describe nothing, and the panel hint is where a member looks for the BIOS. The first step of FU-1 (confirm the pinned `pcsx_rearmed` starts with no BIOS file) is still open and stays with whoever fetches the cores (owner-only download).

## 4. UI leftovers (XC-VIS-004)

### U1: a hidden profile is a missing one to a signed-in viewer
**Keep.** `PackageService::page` returns one 404 for missing, blocked and hidden on purpose; "this profile is private" would tell a blocked member they were blocked, which is the oracle D-382's privacy tiers exist to avoid. The body already says "or you cannot see their page".

### U2: a server accent recolour no longer tints Lounge Light links
**Keep.** P30-VIS-1 made `--text-link` a theme token set for AA on every Lounge surface (#4752c4 on Light, 6.4:1 on white); a server accent is an arbitrary colour and tinting links with it would hand the contrast back to whoever picked the accent. A theme pack that wants another link colour sets the token (P30-VIS-1 registered it for that).

### U3: the hover toolbar overlaps the first line of the top message
**Keep.** Everywhere else the toolbar hangs 16 px above and covers the last line of the previous message, so covering a line while hovering is already the design (and Discord's); it only happens on the one message within 18 px of the scroller top, and it is hover content a pointer move dismisses (WCAG 1.4.13: dismissible, hoverable, persistent). Reserving the first line's width would make the text reflow on every hover, which is worse.

### U4: a TV guide slot label at the right edge is hidden until it fits
**Keep.** A cut "6:" reads as a wrong time; a hidden label is a missing one, and the slot's time is still in the cell's `aria-label` (`cellLabel`) and in the label as soon as the scroller moves. It is the same rule as the sticky start label.

## (a) Implementation list, by user impact

| # | Item | Size | Rust | Migration |
|---|---|---|---|---|
| 1 | C1 `top_frames` in `CrashSummary` (the triage list shows where each crash is without an audited read) | S | yes | no |
| 2 | F4 Bitcoin/Lightning QR in the signed-in profile view through a `tipQr` slot; public surface text-only | S | no | no |
| 3 | F2 `/games/emulator` remembered as a section | S | no | no |
| 4 | F1 `forgetSection()` on the expired/revoked-session exit in `main.tsx` | S | no | no |

Everything else keeps its default and needs no code. Deploy step for the integrator (no decision): `caddy validate` on the rendered `Caddyfile.prod` (F3), and the arena3d replay check (`node replay/run-node.mjs --json`) after the merge, expecting `6fdea5719b5383d6` / `e35977c2a4cddeed`.

## (b) Owner-only

- Analogue movement for arena3d touch (A2): a `sim/input.js` vector changes the replay hash and the wire format; only an explicit owner decision accepting a protocol/replay change opens it.
- A binary `ROSTER` v2 with member ids (A9): a `PROTOCOL_VERSION` bump, same rule.
- Confirming the pinned `pcsx_rearmed` core starts on its HLE BIOS with no BIOS file (FU-1 first step): the core is a download and is not on the machine.
- Unchanged from `fu-fable-decisions.md` section (b): `sr_p42_arcade_room`, the `lofty` bump, KTX2/Basis, a self-built Jolt, hls.js, `platform.public_profiles`, the ambience mixer, base58check if `sha2` is not in the workspace, the node agent notice and the CurseForge EULA, section 4a and section 5 of the owner checklist.

## (c) Decisions-log rows

Append under `## Follow-up (2026-11-02, owner-delegated)` after FU-12, same 3-column header.

| ID | Decision | Status |
|---|---|---|
| FU-13 | **`GET /admin/crash-reports` carries `top_frames` (the first three frames of the stored card as `fn file:line`) so the triage list shows where a crash is; reading them is not the audited replay read.** The list query already fetched the card and the same frames are on every Staff Server card; D-433's audit entry stays on `GET /admin/crash-reports/{id}` alone. (OPS-CRASH-002 question 1) | Adopted (owner-delegated, Fable) |
| FU-14 | **The crash admin page's other defaults stand: search and filter run in the browser over the 200 most recent groups; a 429 keeps a queued card for a later event; the page sits in Admin > Overview for every `STAFF_BASIC` holder; cards the offline queue drops (10 cards, 7 days) are dropped silently.** The queue is bounded on both axes, the server dedupes by fingerprint, and a drop count would widen the closed card format of D-430. (OPS-CRASH-002 questions 2 to 4, FU-CRS-1 queue) | Adopted (owner-delegated, Fable) |
| FU-15 | **arena3d's follow-up defaults stand with the replay (`6fdea5719b5383d6` / `e35977c2a4cddeed`) and `PROTOCOL_VERSION` unchanged: touch Crouch toggles and Jump is held; the stick is 8-way on the existing button bits; touch look is the mouse sensitivity x 1.8; the match never pauses on touch; the successor is whoever D-444 names; the new host rewinds to its newest snapshot and the old host's body is dropped; a hidden host catches up at most 5 s (300 ticks) per call; a lost WebGPU device is retried once, then WebGL2, and a second loss within 30 s goes to WebGL2 directly; `host:seats` is a 4 KiB-bounded JSON key beside the binary roster; the GPU notice is English `role="status"` text like the rest of the game.** An analogue move vector or a binary roster v2 is a protocol and replay change and waits for an explicit owner decision. (ACT-A3D-002 questions 1 to 10) | Adopted (owner-delegated, Fable) |
| FU-16 | **`lastSection` stays one key per browser, forgotten on sign-out, on sign-out-everywhere and when a session ends without a sign-out (the unauthorized exit in `main.tsx`); `/games/emulator` is a section `/app` reopens, other `/games/*` paths are not.** The account id is only known after the gateway `READY`, so a per-account key would hold the door on the socket; clearing on every exit gives FU-10's "per account" result without it. (FU-ENT-4 questions 1 and 2) | Adopted (owner-delegated, Fable) |
| FU-17 | **FU-11 as rendered: Caddy proxies `/~*` to `sr-server` only when the request carries an `sr_session` cookie; a request without one, or one `sr-server` refuses (404), gets the static public profile shell from Caddy.** Anonymous views never reach the app server (D-385 spirit, no log line per crawler), and the public page is the same shell either way, so FU-11's outcome holds. (FU-BETA-1 question 3) | Adopted (owner-delegated, Fable) |
| FU-18 | **The signed-in profile view draws Bitcoin and Lightning tip links as QR codes through a host-filled `tipQr` renderer on `BlocksEnv`; `packages/profile-blocks` imports no encoder, the public `/~handle` surface passes none and stays text-only, the mini popout stays text-only.** FU-4 named the profile page, and the editor's Inspector alone left the page a tipper reads without the code; the shared package is in the public bundle, so the encoder enters only from the app. (FU-ARC-5 question 4) | Adopted (owner-delegated, Fable) |
| FU-19 | **The XC-VIS-004 and FU-STU-5 leftovers stand: one 404 for a missing, blocked or hidden profile (no oracle); `--text-link` is the fixed AA token of P30-VIS-1 and a server accent does not tint it; the hover toolbar sits 2 px inside the one message at the scroller top; a TV guide label the right edge would cut is hidden until it fits (the cell's `aria-label` keeps the time); the PS1 HLE notice shows only while a game runs and the BIOS panel hints whenever no BIOS is loaded.** (XC-VIS-004 questions 1 to 4, FU-STU-5 question 5) | Adopted (owner-delegated, Fable) |
