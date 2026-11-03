# Handoff: fix/fu-r2 (FU-20 to FU-24 implementation, builder r2)

Branch `fix/fu-r2` on main `0f82ab05`. Not pushed. Decisions: `docs/dev/handoff/fu3-fable-decisions.md` (committed), rows FU-20..FU-24 added to `docs/01-decisions-log.md`.

## Done (commits, in order)
1. **R2 (FU-20)** release DM asks `ReminderGate::check(watcher, None)`; skipped or unknown = not notified; audit gains `skipped`. `ArcadeDeps.gate` shares one `SocialGate` with the card deps in `app.rs`. Unit test with a stub gate (`routes/arcade.rs`, `release_tests`).
2. **H1 (FU-22)** `crates/sr-server/src/house_notes_md.rs` (in-house renderer, replaces `house_notes_html`), TS mirror `apps/web/vite/houseNotesMd.ts` used by `publicDev.ts`, shared fixtures `crates/sr-server/tests/fixtures/house-notes/{basic,links,headings}.{md,html}` read by the Rust test and `vite/publicDev.test.ts`, legal CSS in `scripts/render-legal.mjs`, new web_gate test, hint text fixed.
3. **R1 (FU-20)** final DM wording (`release_text`, 3-case test) and the "Released as" picker (`ArcadeAdmin.tsx`, `adminApi.ts`, `fu-rust-1.ts`, vitest incl. no-body default).
4. **B1 (FU-23)** `validate_tip_with` / `validate_tips_with` + `Sha256Fn` in `sr-core` (no sha2 there); `sr-activities` and `sr-packages` pass `sha2`. Wiki vectors (valid, one char changed, bc1) tested in both callers as unit tests (not in `tests/`, so they need no database); `sr-core` has the None/zeros-hash tests.
5. Decisions-log rows; OpenAPI + api-client regenerated (only the `ReleaseResult.notified` description changed), `mocks:check` green.

## Tests and gates run
- cargo: `fmt --check`, `clippy -D warnings --all-targets` on sr-core, sr-activities, sr-packages, sr-api, sr-server; tests: sr-core tips, sr-activities and sr-packages `--lib`, sr-api `--lib arcade`, sr-server `--lib house_notes` and `--test web_gate` (24 pass). Not run: the DB-backed integration tests of sr-activities/sr-packages and `authz_matrix`/`immutable_rules` (no new routes, so no matrix rows).
- web: tsc, eslint and prettier on changed files, vitest `vite/publicDev.test.ts` (13) and `src/arcade` (20), `check-css-tokens`, `i18n-audit` (0), web build, `check-entrance-isolation`, `check-bundle-budget`, `playwright --list` (902 tests).

## Visual check (`C:\Users\braxt\Desktop\SIDEROOM_wt\visual-sweep-shots\fu-r2`)
`admin-arcade-{desktop,phone}-{light,dark}.png` (picker), `admin-arcade-released.png` ("1 of 1 watchers were told"), `pub-house-notes-{desktop,phone}-{light,dark}.png` with seeded markdown staff copy (heading, bold/italic/code, link, list, quote, `<script>` shown as text), `dm-release.png` (the release DM in the member's SIDE ROOM DM). Read by eye: no defects. The house-notes page is dark-only by design, so light and dark are identical. House-notes was seen through the Vite dev mirror; the real sr-server path is proven by the unit, fixture and web_gate tests, not by a browser shot.
Stack stopped, DB `sr_fu_r2` dropped, scratch binaries in `SIDEROOM_wt\fu-r2-bin`.

## Not done
- The "one real WebM voice upload against non-memory storage" check (integrator step) was not part of this task.
- The release route itself has no DB-level test with real watchers (decision file suggested one); the gate loop is covered by the extracted `send_release_notices` unit test instead.

## Owner questions / defaults chosen
1. Heading levels: `#`..`####` give h2, h3, h4, h4 (spec listed three; a fourth mark is clamped, five or more is text). Heading ids are ASCII-only, so a non-ASCII-only heading gets no id.
2. Paragraph newline is `<br>`; blockquote lines join with a space; list is one level (an indented `- x` is a flat item).
3. `![alt](url)` stays literal text, `[x](javascript:...)` shows the link text only. Links carry `rel="noopener noreferrer"` (no `nofollow`, unlike the in-app renderer).
4. The picker lists every candidate the page already fetched, featured or not.
