# Handoff: feat/p32-utl-cards (wave 32, UTL-CRD-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p32-utl-cards`, branch `feat/p32-utl-cards`, base `integration/phase32` plus a
merge of `feat/p32-utl-shell` at `cb33345e` (the tool registry). Migration **1670** only. Job id **1670**
(`cards.remind`). No new dependency, nothing pushed, no PR, no server started (8083/5176 unused; no Playwright spec).

Item: **UTL-CRD-001, Cinema ticket and game-night invite cards.** The tool id is `invite`; the message-component
custom-id prefix is `card:`; the message type is `201`.

## Done

Backend
- `crates/sr-db/migrations/1670_invite_cards.sql`: `invite_cards` (the card id is the message id; kind
  `cinema|live_tv|game_night`, title, note, `starts_at`, `duration_min`, picked `timezone`, `seats` 1..250, optional
  `title_id` and `link_path`, `canceled_at`/`started_at`/`closed_at`) and `invite_card_claims` (card, member, `remind`,
  `reminded_1h_at`, `reminded_10m_at`). Both cascade from the channel and the account.
- `crates/sr-chat/src/cards/` (new): `mod.rs` (types, `validate_new`, `card:<action>:<id>` ids, `CardStore` trait),
  `render.rs` (the embed and the button row, rebuilt from the rows every time), `reminders.rs` (the window rule, the DM
  privacy rule, `ReminderGate`/`SocialGate`), `ics.rs` (our RFC 5545 writer, no dependency) with
  `testdata/game_night.ics` (the fixed expected file; `.gitattributes` marks it `-text` so CRLF survives checkouts).
  `MessageType::InviteCard` = 201 added in `domain.rs` (two lines).
- `crates/sr-db/src/cards_repo.rs` (not in the Split row, needed for the store): `PgCards`.
  **Seat claims are atomic**: `claim` locks the card row `FOR UPDATE` in a transaction, then counts and inserts, so
  concurrent callers queue and the last seat goes to exactly one. A member who already has a seat keeps it (a double
  claim only updates the reminder choice). Card creation takes a per-channel advisory lock for the 25-open-cards cap.
- `crates/sr-api/src/routes/tools/cards.rs` + `tools/mod.rs`: `POST /channels/{id}/cards`,
  `GET|DELETE /channels/{id}/cards/{card_id}`, `POST|DELETE .../claim`, `GET .../ics` (`text/calendar`).
  Create needs `SEND_MESSAGES` (threads: `SEND_MESSAGES_IN_THREADS`); claim, unclaim, read and `.ics` need only
  `VIEW_CHANNEL` (a read-only announcement channel can still take seats); cancel is the host or `MANAGE_MESSAGES`.
  Full card = 409 "All the seats are taken." Bots and OAuth bearer tokens are refused (`auth/policy.rs`, one block).
  The card is a system-user message; **every change rewrites it** through the existing `rewrite_system_card`
  (`MESSAGE_UPDATE`), serialised per card in process, reading the whole state inside the lock. So there is **no new
  gateway event and nothing added to `sr-events`/`sr-gateway`** (no clash with utl-collab's `TOOL_ROOM_UPDATE`).
- `crates/sr-server/src/cards_jobs.rs`: singleton **`cards.remind`, job id 1670**, every 30 s. It (1) marks cards started
  and rewrites them (a cinema card with a title gains **Start watch party**), (2) closes cards 6 h after the start (the
  buttons go), (3) sends reminders. Wired in `app.rs` (deps extension, runner, schedule) with `lib.rs` (`cards_jobs`
  and the test module).
- **Reminders** are a DM from the system user through `Poster::dm`, the same path Live TV reminders use (so a
  notification and a push like any DM). One at about 1 h and one at about 10 min before the start, only for claimants who
  asked. Rules: a claim made after a window never gets it; if both are due only the 10-minute one goes; nothing after
  the start; each window is marked before sending (two nodes cannot both send) and given back if the send fails.
  **DM privacy** (`dm_decision`): no DM if the member blocked the system user, moved the system user's conversation to
  ignored or spam, turned off "allow direct messages from server members" for the card's server (the per-server
  override wins over the account default), or can no longer see the channel. A DM-channel card has no server, so the
  server setting does not apply there. Skipped members are not retried. `GET`/claim responses carry
  `mine.reminder_deliverable`, so the web can tell the member the reminder cannot reach them.
- Authz: six rows in `crates/sr-api/tests/route_matrix.tsv` (`authenticated`, `membership`, BOT `deny`); OpenAPI,
  `schema.d.ts` and the MSW handlers regenerated (additions only, +6 operations; the body type is `CardClaimBody`
  because `ClaimBody` already exists in review).

Web (`apps/web/src/tools/cards/`, registry entry `invite` in group `collab`, strings in `i18n/messages/en/p32-utl-cards-1.ts`)
- `InviteTool.tsx`: channel (the one you are in), kind, title, note, start (a `datetime-local` plus a time zone select,
  default the device zone), seats, length, and an optional in-app link. A Cinema title page address
  (`/cinema/t/<id>`) also sets `title_id` for the watch-party hand-off. Client checks first, server checks again.
- `time.ts`: wall time in a zone to a UTC instant with `Intl` only (summer/winter, the repeated hour resolves to the
  earlier one). The server stores the instant in UTC and the zone name only as a label ("Time picked in ...").
- `press.tsx` (`useCardPress`): the four buttons call the card routes; claim says "reminders on" with a "Turn off
  reminders" link (a second claim with `remind:false`) or warns when the DM settings block them; Add to calendar
  downloads the `.ics`; **Start watch party** calls the existing `POST /watch-parties` (the server checks
  `START_WATCH_PARTY`, 403 shows "no permission"; a 409 joins the party already running in the channel).
- Two small edits outside the new directory: `lounge/msg/Components.tsx` (a `card:` branch beside the `sr:` one) and
  `lounge/msg/SystemMessage.tsx` (type 201 draws as a message, like 200 and 24).

## Tests run

- `cargo test -p sr-chat` (361 passed; 28 are the new `cards::` tests: validation, ids, renderer, windows, the DM privacy
  rule and the gate over the in-memory social repo, the iCalendar writer including the fold at 75 octets, escaping, CRLF
  and **the fixed expected file** `testdata/game_night.ics`; `SR_UPDATE_FIXTURES=1` rewrites it on purpose).
- `cargo test -p sr-db --test cards_repo -- --include-ignored --test-threads=1` on my scratch database `sr_p32_cards`:
  9 passed (round trip, cap + double claim + unclaim + order, **38 callers racing for the last seat: exactly one wins**,
  **30 callers for 7 seats: exactly 7 rows**, cancel, cascade on channel delete, the open-card cap, reminder marks and
  candidates, started/closed once).
- `cargo test -p sr-server --lib cards_pg_tests -- --include-ignored --test-threads=1`: 11 passed through the real
  handlers (card message and `card:` ids, claims and the message rewritten after each, **10 members racing for the last
  seat through the handlers with the final message matching the table**, permissions and bearer refusal, cancel,
  the `.ics` download, reminders at 1 h and 10 min once only, a late claim, a failed send retried, **DM privacy / block /
  muted conversation / per-server override / lost channel view**, the start pass with the watch-party button and the
  close pass, a group-DM card).
- `cargo test -p sr-api --test route_matrix --test openapi_names` (6 + 2 passed), `--lib tools:: policy` passed.
- `cargo fmt --check` and `cargo clippy --all-targets -D warnings` on sr-chat, sr-db, sr-api, sr-server: clean.
- Web: `vitest run src/tools src/i18n src/lounge/__tests__/p14AlertCards.test.tsx msgActions.test.tsx` (114 passed, 16 of
  them new in `tools/__tests__/cards.test.tsx`: time zones incl. DST, ids, link parsing, type 201, each button against
  MSW, the tool form), `tsc --noEmit`, eslint and prettier on the touched files, `check-css-tokens.mjs` clean.
- `node scripts/build-web.mjs`, `check-entrance-isolation.mjs` (every public surface clean), `check-bundle-budget.mjs`
  (all ok). The tool loads lazily; the only addition to an existing chunk is the small `card:` press hook in `Components.tsx`.
- Not run: the full web vitest suite, Playwright (no spec written; the e2e needs the stack on 8083/5176), the
  `authz_matrix` Postgres suite (prf-profiles owns it this wave), `immutable_rules.rs`.

## Not done / open

- **The timezone tool's pick** (utl-calc): the tool is not merged here, so the form has its own zone select and `time.ts`.
  When utl-calc exposes a picker the form can use it; the card only needs a UTC instant and a zone name.
- No **edit/reschedule** of a card (cancel and post a new one); no PATCH route. Seats cannot be raised later either.
- **Data subject requests:** claims are personal data. They are deleted by the account FK (`ON DELETE CASCADE`) and with the
  channel, but `sr-admin/src/data_requests.rs` (not mine) does not list them in the export yet. One line there for the
  integrator or wave 40: "invite card seats" with the card title, time and the member's `remind` choice.
- **Retention (docs/09, for wave 40):** a card and its claims stay as long as the channel; cards are not purged after
  they close. Say so in the row or add a purge.
- Server-side card text is English (like the watch party card); the buttons' labels and the embed field names are not
  localised. The web strings are.
- The watch-party hand-off is a button, not an automatic start: starting a party needs a person (the host, with
  `START_WATCH_PARTY`), so the job only offers the button at the start time.
- Reminder DMs for a card in a guild come from the system user, which joins the guild for DMs the way the other feature
  modules do (`SystemPoster`); nothing new there.

## Integration notes

- `apps/web/src/tools/registry.ts`: I appended `invite` after `guide` (conflict likely with every tool agent; keep all).
- `crates/sr-api/src/routes/tools/mod.rs` is created by me with the same shape as utl-collab's: keep every `pub mod`, every
  `.merge(...)` and `doc.merge(...)`. Same for the `pub mod tools;`, `.merge(tools::router())` and `doc.merge(tools::openapi())`
  lines in `routes/mod.rs`, and the `auth/policy.rs` block (mine is `"/cards"`, collab's `"/tool-rooms"`).
- `crates/sr-server/src/app.rs`, `lib.rs`: append-only; `lib.rs` gets `pub mod cards_jobs;` and `mod cards_pg_tests;`.
- `packages/api-client/openapi.json`, `schema.d.ts`, `handlers.generated.ts`: regenerate after merging both branches rather
  than resolving by hand.
- Job ids in use: 1660 `tool_rooms.sweep`, **1670 `cards.remind`**. Message type 201 is new.
- Add to `deploy/OPERATOR.md` jobs list if the integrator keeps one: `cards.remind` (30 s).

## Decisions

- Card = system-user message (type 201) rewritten from the database, like the watch party card (not a member-authored
  message with components, which users cannot send). The host is shown as "Hosted by @name" and owns the cancel right.
- Claiming needs `VIEW_CHANNEL` only; making a card needs `SEND_MESSAGES`.
- A double claim is idempotent (200), a full card is 409, so a double click on the button is harmless.
- Reminders are opt-in per claim (default on at claim time, one click to turn off) and are skipped, not queued, when the
  member's DM settings say no.
- The `.ics` uses UTC instants, a 10-minute display alarm, `STATUS:CANCELLED` and `SEQUENCE:1` for a canceled card, UID
  `card-<id>@<public_url host>` and `URL:<public_url>/channels/<guild or @me>/<channel>/<card>`.
- Card life: opens at creation, "started" at the start time, closed 6 h later (buttons removed, seats frozen).

## Open owner questions

- Should a member who turned off "allow DMs from server members" still get card reminders (they claimed on purpose)?
  The conservative default implemented here is no, with a warning at claim time. A per-card exception would be a one-line
  change in `dm_decision`.
- Should a finished or canceled card (and its claimant list) be purged after some time? Default: kept with the channel.

## CHANGELOG lines (Phase 32)

- Invite cards: post a Cinema ticket, a Live TV slot or a game night as a card with a seat count, a "Claim a seat" button,
  the people going, an "Add to calendar" download and optional reminders by direct message an hour and ten minutes before
  (they follow your direct message settings). A Cinema card offers a watch party when it starts. Migration 1670, job
  `cards.remind`, routes under `/channels/{id}/cards`.
