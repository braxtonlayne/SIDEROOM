# Handoff: feat/p24-parity (wave 24: RDY-PAR-001 to 006, RDY-LIB-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p24-parity`, branch `feat/p24-parity` (base `integration/phase24` as of `d7ab66b2`). Local commits only; nothing pushed. Target dir `p24-parity-target`, scratch database `sr_p24_parity` (both `SR_TEST_DATABASE_URL` and `DATABASE_URL`), ports 8083 (sr-server, metrics on 9103) and 5176 (Vite). Migration used: `1310_dm_request_reason_spam.sql` (1311 to 1319 stay unused).

## Resume here

Everything below is built and its proof ran locally unless "Not run" says otherwise. No process of mine is running (the server and Vite of the Playwright run were stopped by PID). The scratch database `sr_p24_parity` was dropped at the end. The only item whose proof waits for CI is RDY-LIB-001 (see below).

## Method

Every box was audited against the code first; several were half built already, so the work is the real gap, not a rebuild.

## Done

### RDY-PAR-001 Group DM invite link

- **Already built, not rebuilt**: `POST|GET /channels/{id}/invites` on a group DM, `GET|POST|DELETE /invites/{code}` (G-47, `sr-api/src/routes/social/group_invites.rs`): owner only, 24 hours, unlimited uses, group size and group-count caps from `SocialLimits`, `CHANNEL_RECIPIENT_ADD`/`CHANNEL_CREATE` on join. Join by link is the invite page (`routes/entrance/ServerInvite.tsx`, handles `type: 1`). Their tests: `sr-chat` `group_invites_*` (owner-only, expiry, size cap), `sr-api/tests/lounge.rs` `group_dm_invites_create_resolve_join_and_revoke`, Postgres `sr-db/tests/p12_lounge_ch.rs` `group_dm_invites_round_trip`. I wrote a duplicate HTTP test, saw the existing one, and dropped mine.
- **New**: the copy-link in the Create DM picker. `lounge/shell/GroupInviteLink.tsx` (creates the link on demand, copies it, keeps it visible, announces the copy); `CreateDmDialog.tsx` keeps the dialog open after a group is created ("Group DM created" with the link row and Done) and shows the same row to the owner when adding people to an existing group. Fragment `p24-parity-4.ts`.
- **Tests**: `homeFlows.test.tsx` "offers the invite link of the new group and copies it" (also covers the clipboard write); the existing "creates a group DM" still passes.

### RDY-PAR-002 Emoji picker category sidebar

- `lounge/components/EmojiPicker.tsx` + new `emojiPicker.css`: a vertical toolbar beside the grid with one button per section that is on screen (favorites, frequently used, this server with its icon or initial letter, then each category with its first emoji as glyph). Click jumps; `onScroll` highlights the section whose top passed the edge (scroll sync); ArrowUp/Down/Left/Right, Home and End move focus and jump, with a roving tab stop; a search narrows the sidebar with the grid. Fragment `p24-parity-1.ts`.
- **Limit**: the picker only holds this server's custom emoji, so one server icon shows. Emoji from other servers ("use anywhere") would be a new section, not a styling change.
- **Tests**: `emojiPicker.test.tsx` (9 tests: toolbar, roving tabindex and keys, jump and scroll sync with a stubbed layout, server section with narrowing, axe).

### RDY-PAR-003 Profile theme, server tag display, markdown in About Me

- **Already built**: the two-colour theme (`theme_colors` in `sr-chat/src/social/profile.rs`, `lounge/profile/theme.ts`, the colour pickers in Settings > Profile) and the server tag picker (`ServerTagCard.tsx`, `ServerTagChip`, the tag badge on the profile). No new column or route was needed, so there is no migration for this item.
- **New**: `lounge/profile/profileMarkdown.tsx`: the profile subset of the shared `sr-markdown` parser (bold, italic, underline, strikethrough, spoilers, inline code, links, emoji, line breaks; headings, quotes, lists and code blocks flattened to text; mentions, `@everyone`, timestamps and the Lounge chips dropped) and `ProfileBio`. The popout, the full profile and the Settings preview all render About Me through it, and the preview now wears the profile theme (gradient, text tone) and shows the chosen server tag. Rendering only: what is stored and the 190-character limit are unchanged.
- **Tests**: `lounge/__tests__/p24ProfileCard.test.tsx` (subset rules; the real WASM parser when the bundle is built; popout and full profile give the same bio markup and the same theme variables), `settings/user/__tests__/p13profile.test.tsx` (preview shows markdown, theme and tag).

### RDY-PAR-004 DM spam filter and per-server override

- **Already built**: the three filter levels and the per-server "allow DMs from members" override with their routing (`open_dm`, `shared_guild_allows`), the settings cards (`SafetyCards.tsx`), tests in `sr-chat` and Postgres.
- **New (the heuristics)**: `sr_chat::social::safety::dm_spam_reason` (the automod `spam_reason` rules plus three different link hosts) and `SocialService::screen_dm_message`, called from the message hook for 1:1 DMs. When the recipient's filter is on, the sender is not their friend and the request is still pending, a message that trips a heuristic moves it to "Likely spam" (state `spam`, new request reason `spam`). Filing a request as spam by hand gives it the same reason. `GET /users/@me/channels` and READY return spam requests with `is_message_request: true` and `sr_request_reason: "spam"`. Migration `1310_dm_request_reason_spam.sql` widens the CHECK on `channel_recipients.request_reason`.
- **Web**: Message Requests gets a "Likely spam (n)" section; a settings paragraph says what the filter looks at. Fragment `p24-parity-5.ts`. OpenAPI and client regenerated (one description changed; no new route, so the route matrix has no new row).
- **Tests**: `sr-chat` `dm_spam_heuristics`, `dm_spam_filter_reads_a_strangers_first_message`, `filing_a_request_as_spam_gives_it_the_spam_reason_and_friends_are_never_screened`; `sr-api` `a_spammy_first_message_lands_under_likely_spam` (message route to list to accept); Postgres `sr-db/tests/p24_parity.rs`; Vitest `homeFlows.test.tsx` "lists what the spam filter flagged under Likely spam".

### RDY-PAR-005 View Server As Role as a Lounge mode

- `settings/server/viewAsMode.tsx`: a small store, `useViewAsChannels` (the channel sidebar of the previewed server shows only the channels and categories the chosen roles can see) and `ViewAsBanner` ("Previewing X as roles. Nothing is saved." with Exit preview, which returns to Server Settings > Roles). The dialog (`sections/ViewAsRole.tsx`) gets "Preview in the Lounge", which stores the result, closes, and opens the first readable text channel. The visibility is still the `sr-permissions` WASM through `viewAs.ts`. Two one-line hooks outside the owned files: `lounge/ChannelSidebar.tsx` (the filter) and `lounge/LoungeShell.tsx` (the banner). Fragment `p24-parity-3.ts`.
- **Tests**: `viewAsMode.test.tsx` (filter, banner, exit), `p14profile.test.tsx` (dialog to store, needs the wasm), Playwright `e2e/tests/view-as-role.spec.ts` (new file; green on Chromium locally against 8083/5176: the dialog hides a deny-overwritten channel, the Lounge shows the banner and omits it, Exit returns to settings, back in the Lounge it is listed again). Not run on Firefox/WebKit locally (Firefox does not launch here; the spec is in the default `playwright` matrix of `e2e.yml`).
- **Scope note**: the preview narrows the channel list; messages and members of a channel are not re-filtered. A channel the previewed role cannot see simply is not in the list (the first visible channel is opened on entry).

### RDY-PAR-006 GIF autoplay preference

- `gifAutoplay` in `settings/user/prefs.ts` (default on, validated, synced with the other preferences by the existing sync), a switch in Settings > Accessibility next to the sticker and emoji animation controls, `settings/user/gifAutoplay.ts` (`useGifAutoplay`: preference on and window focused and reduced motion off, Settings or system). Used by GIF attachments (`lounge/msg/Attachments.tsx`) and the GIF picker thumbnails (`GifPicker.tsx`); otherwise the first frame shows (`useAnimatedSrc`, as stickers do) and the GIF plays on hover or keyboard focus. Fragment `p24-parity-2.ts`.
- **Tests**: `settings/user/__tests__/gifAutoplay.test.tsx` (default, validation, focus and blur, reduced motion setting and system, the switch, attachment rendering).
- **Limit**: `emojiAnimation` still has no consumer (animated custom emoji in messages ignore it); not part of this item.

### RDY-LIB-001 Library exit gate: written, run only as far as memory storage allows

- `e2e/tests/library-gate.spec.ts`: (1) a test with no server dependency that builds the ten chapter ZIPs (a CRC32 and PNG encoder and a stored-method ZIP writer in the spec, no new package) and reads them back (names, CRCs, PNG headers, distinct pages): **runs and passes locally on Chromium**; (2) the gate, under `SR_E2E_LIVE=1`: a member creates a new (RTL) series submission; chapter 1 goes through the page (add chapter, choose the ZIP, in-browser archive check, presigned PUT) and chapters 2 to 10 use the routes the page calls; the member submits; the Owner reviews through the API (claim, approve nine items, reject one with a note, request changes with a reason); the member sees the banner with the reason and the item note; the member replaces the file and resends; the reviewer approves the rest, approves and publishes; then chapter 1 opens in the reader at 390 x 844, ArrowLeft goes forward (RTL), the saved page is read back by a second browser context of the same member, which lands on the page the first one turned to.
- `.github/workflows/e2e.yml`: a step in the `live` job (Garage, `SR_E2E_LIVE=1`, `SR_ADMIN_REQUIRE_REAUTH=0`) runs the file on `--project=chromium --project=firefox --project=webkit` with `--retries=0` in one server run, and uploads `server-library.log`.
- **What ran locally**: with `SR_E2E_LIVE=1` against `storage.endpoint = memory:` the run got through creating the series, opening the page, adding chapter 1, choosing the ZIP (archive check passes) and starting the upload, and stopped at the presigned PUT ("Failed to fetch": memory storage has no presigned URLs). That proves the selectors of the first half of the page path.
- **What waits for CI** (nothing below has run): the PUT and the ingest conversion of ten chapters; the exact shape of the review items (`items` for a series submission, written tolerant of a series-level item), claim/item/request-changes/approve/publish on a real submission; the banner text; republish; the RTL reader and progress sync (debounced save, polled up to 20 s); Firefox and WebKit (Firefox does not launch on this machine). **RDY-LIB-001 is therefore not ticked.** A failure in the first live run is most likely a selector or a payload shape in those steps, not the product.
- The device half (real phone, real screen reader) stays in `dev/owner-checklist.md`.

## Tests run

All against the scratch database `sr_p24_parity`, one cargo command at a time with `-j 2`.

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean on sr-chat, sr-db, sr-api, sr-server.
- `cargo test -p sr-chat -p sr-api -- --include-ignored --test-threads=1`: sr-chat 329 and sr-api 683 lib tests plus the integration binaries (route matrix, OpenAPI names, authz, lounge) all passed.
- `cargo test -p sr-db -p sr-server --no-fail-fast -- --include-ignored --test-threads=1`: everything passed except `sr-db/tests/p13_settings_admin.rs` `staff_people_views_link_unlink_notes_and_freeze`, which fails deterministically at its line 260 (`set_person_notes(1, ...)` is expected to answer `NotFound("person")`). It is about the Person link code of waves 20 to 22; this branch touches nothing under `sr-admin`, persons or that test file. Not investigated further and not verified against the base commit.
- `cargo test -p sr-db --test p24_parity -- --ignored --test-threads=1`: 1 passed (migration 1310 applied on a fresh child database).
- Web: `tsc --noEmit`, eslint, prettier (local copy) clean; i18n verify-diff "MISSING 0" and no new hits in `scripts/i18n-audit.mjs` (the 41 it lists were there before); targeted Vitest for every touched area; see the table for the full run.
- Playwright (Chromium, ports 8083/5176): `view-as-role.spec.ts` 1 passed; `library-gate.spec.ts` ZIP test passed, the gate stopped at the presigned PUT as described.

## Final gate table

| Gate | Result |
|---|---|
| `cargo fmt --check` (sr-chat, sr-db, sr-api, sr-server) | clean |
| `cargo clippy --all-targets -D warnings` (same four) | clean |
| sr-chat, sr-api tests incl. Postgres (`--include-ignored`) | all passed |
| sr-db, sr-server tests incl. Postgres | all passed except the one unrelated sr-db test above |
| `tsc --noEmit`, eslint, prettier (local copy) | clean |
| i18n | `i18n-verify-diff` "MISSING 0"; the audit's 41 hits were there before |
| wasm build (`packages/wasm/scripts/build.mjs`, `WASM_NO_OPT=1`, `CARGO_BUILD_JOBS=2`) | built all six crates |
| Full Vitest once, `--maxWorkers=2` | run twice: 2613 of 2617 passed (4 failed, `prefsEffects` first test and others), then 2615 of 2617 (`msgGaps` "loads the next page" and `p13WebUi2` "emoji popout"). The failing set changed between runs and every one of those files passes when run on its own (`prefsEffects`, `msgGaps`, `p13WebUi2`: 23 of 23). They are the load-sensitive cold-start timeouts of the lazy Lounge chunk (`prefsEffects` has no warm-up import; with two of my Lounge edits reverted it failed the same way). |
| `vite build` + `scripts/check-bundle-budget.mjs` | ok (entrance 232 of 300 KB; lounge 432 of 600; settings 305 of 600; all others under budget) |
| `scripts/check-doc-links.mjs` | ok |
| Playwright, Chromium, ports 8083/5176 | `view-as-role.spec.ts` passed; `library-gate.spec.ts`: ZIP test passed, gate step reached the presigned PUT |
| OpenAPI, api-client schema, MSW mocks | regenerated (one description changed, no new route; route matrix test passes, `KNOWN_CLASHES` still empty) |

## Decisions (conservative defaults, appended to `docs/01-decisions-log.md` as P24-PAR-1 to 5)

1. The spam filter reads only a pending request's messages from a non-friend, only when the recipient's filter is on, in memory, and files it under "Likely spam"; three different link hosts count as spam on top of the automod rules.
2. "Likely spam" is a section of Message Requests (reason `spam`), not a second inbox; user-filed spam shows there too.
3. About Me renders the profile subset of `sr-markdown`; storage and the 190-character limit are unchanged.
4. GIFs autoplay only while the preference is on, the window is focused and reduced motion is off; default on.
5. View Server As Role is client-side and narrows the channel list only.

## Open owner questions

- Is "three different link hosts in the messages of a still-pending request" the right strictness for the spam filter (it is deliberately lower than the automod rule's five)? It is one constant, `DM_SPAM_LINK_HOSTS`. *Answered (Fable, wave-24 integration): P24-PAR-1 to 5 kept; `DM_SPAM_LINK_HOSTS` stays 3. The other two questions are backlog rows in `dev/answers-followups-p16.md`.*
- Should the emoji picker also list emoji from the member's other servers ("use anywhere") with a server icon per section? Today it shows this server's emoji only.
- Should the Lounge preview also hide the members and messages the previewed role cannot see? Today only the channel list is narrowed.

## Files outside the Split row that were touched, and why

`lounge/ChannelSidebar.tsx`, `lounge/LoungeShell.tsx` (the preview filter and banner), `lounge/shell/CreateDmDialog.tsx`, new `lounge/shell/GroupInviteLink.tsx`, `lounge/shell/MessageRequestsPage.tsx`, `lounge/shell/api.ts`, `lounge/shell/requestReason.ts` (the spam section), `lounge/msg/Attachments.tsx`, `lounge/components/GifPicker.tsx` (GIF autoplay), `crates/sr-chat/src/social/{dm,safety,service}.rs`, `crates/sr-chat/src/message_hooks.rs`, `crates/sr-api/src/routes/social/{dms,model}.rs`, `crates/sr-server/src/gateway_pg.rs` (the READY flag), `crates/sr-db/migrations/1310_*`, new `e2e/tests/view-as-role.spec.ts`, `.github/workflows/e2e.yml` (the gate step). Each is the smallest change the item needs; none touches a file another wave-24 row owns.
