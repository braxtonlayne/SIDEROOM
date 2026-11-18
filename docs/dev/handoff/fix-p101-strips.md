# Wave 101, strips: edge strips from a shared helper (SEC-AUD-447, D-957)

## Times (2026-11-18, local)
Round 1 (lock 06:20 to 06:25), a diagnosis round (06:31 to 06:36), round 2 (06:53 to 06:57). Checks after the last edit, 06:57: `npx prettier --check` on the three files exit 0; e2e `npx tsc --noEmit -p tsconfig.json` exit 0; `node scripts/check-doc-links.mjs` exit 0. Not run: web typecheck, eslint on app files, i18n audit, any build (no app file changed).

## Done
- Commit `20d5b30a` (SEC-AUD-447): `e2e/support/edge-strips.ts` (new); `p98-cue-shots.spec.ts` and `p100-rtl-shots.spec.ts` call it in place of their `boundingBox` code.
- Diff in words: the helper scrolls the box into view (`block/inline: 'nearest'`, instant, one animation frame), reads its rect and the viewport in one `evaluate`, intersects them, throws `edge-strips: <selector> has only <n> px in view` when under 40 px of height or under 2 x width of width is visible, and cuts the two strips from the visible part. In each spec the four `boundingBox`/`clip` lines became one call (`maxHeight` 200 in cue, 120 in rtl, width 16); each spec also records the clip (`y`, height, viewport height) in a new `REPORT strips [...]` line. Nothing else changed: no comparison, no print, no threshold.
- Helper: `edgeStrips(page, target: string | Locator, { width = 16, maxHeight }) -> { l, r, box, clip }` (`clip` = left strip rect plus `viewportHeight`, `viewportWidth`, `visible`). Does not scroll the box's own content, does not scroll back, no dependency.
- The three lines a later spec writes (also in the helper's header):
  `const off = await edgeStrips(page, WRAP, { maxHeight: 200 });` ... switch the rule ... `const on = await edgeStrips(page, WRAP, { maxHeight: 200 });` then `on.l.equals(off.l)` / `on.r.equals(off.r)`.

## Round 1: the unedited specs, one world (fresh DB, seed with no FAIL)
| run | result | failing line | invoices in table |
|---|---|---|---|
| p99-reach | pass (`problems []`) | - | 5 (its own) |
| p98-cue (1st) | **fail**: `problems` "light/dark: scroll end: shade Ltrue Rtrue" (exit 1) | `totals.json` | 10 (wrapper h 712, y 28) |
| p100-rtl | pass, but `findings ["invoices ltr end: shade Ltrue Rtrue, more table beyond Ltrue Rfalse"]` (the same wrong right strip at the end) | - | 15 |
| p98-cue (2nd) | **fail**: `page.screenshot: Clipped area is either empty or outside the resulting image` | `p98-cue-shots.spec.ts:343` (`edges`), wrapper h 1324, y -278: clip wholly above the viewport | 20 |
Row counts are inferred from the wrapper heights and the seeds (5 per spec run); I did not count rows in the DB. The first cue log was overwritten by the second (same out tag); the first run's failure is from its `totals.json` and its shots.

Honest note: the first cue run's wrapper (712 high at y 28, bottom 740 of 768) was fully in view, yet the right strip at scroll end differed from rule-off. I could not reproduce that exact case afterwards: a second fresh-world run of the unedited spec at 10 rows (wrapper 678, y 45) passed (`problems []`), and at 15 rows (h 1018, y -125, clip half above the screen) it also passed; at 20+ rows it failed with the empty clip (h 1358) as in wave 100. So the reproduced fault is the empty/off-screen clip; the 712 case at the viewport's bottom edge is unexplained (the table's rows were taller that run, the dates wrapped differently, and the strip at x 896-912 touches the end of the "Created" text, which may differ by antialiasing between the two screenshots; unproven). With the helper that case cuts the same pixels as before, so it is not claimed fixed; it did not recur in round 2 (10-row run: h 678, passed).

## Round 2: edited specs, fresh DB and world, same order (reach, cue, rtl, cue)
| run | result | invoices (wrapper h) | strip `y` / h / viewport h |
|---|---|---|---|
| reach | pass | 5 | - |
| cue 1 | pass, `problems []` | 10 (h 678) | y 45, h 200, vh 768 (wrapper fully in view) |
| rtl | pass, `findings []`, `specErrors []` | 15 | compare table y 269.4 h 120 vh 812; Invoices wrapper y 0 h 120 vh 768 (wrapper started above the viewport; strips from the visible part) |
| cue 2 | pass, `problems []` | 20 (h 1290) | y 0, h 200, vh 768 (wrapper ran past the viewport) |
Earlier same-day, on a kept 20-row world: edited cue and rtl both passed (strips y 0, h 200 and 120, vh 768; wrapper h 1358), where the unedited cue failed with the empty clip. A further cue run on that DB hit `POST /sr/register -> 429` (register rate limit from too many runs on one DB): a stack/world artefact, not a fault.
Figures unchanged: cue's shade line in every round-2 run: `start Lfalse Rtrue; mid Ltrue Rtrue; end Ltrue Rfalse` (light and dark), same as the passing wave 98/100 reading. rtl: LTR start L no / R yes, end L yes / R no; RTL start L yes / R no, end L no / R yes (equal to `fix-p100-rtl.md`'s table, with 83 px of table beyond the edge instead of 48 because the world has wider rows).
Shots opened (4): `visual-sweep-shots/p101-strips/cue-r2c2/invoices-1024-start-lounge-light.png` (table runs past the bottom; right edge shaded, left not), `.../invoices-1024-end-lounge-light.png` (left shaded, right edge clear), `rtl-r2r/invoices-1024-rtl-start.png` and `invoices-1024-rtl-end.png` (mirrored: shade on the left at RTL start, on the right at RTL end). All show 20+ rows with the wrapper taller than the viewport and the shade where the readings say.

## Specs that still clip a screenshot from a box themselves (not moved)
`git grep -n "clip:\|clip(" -- e2e/tests`: `p78-files-shots.spec.ts:141,248,261`, `p79-files-shots.spec.ts:173`, `p74-dots-shots.spec.ts:228`, `p75-polish-shots.spec.ts:151`, `p93-study-shots.spec.ts:236`, `p94-valve-shots.spec.ts:538` build clips from a box or fixed coordinates; they read shots by eye, not edge pixels, so not changed. `p86-admin-shots.spec.ts:420` and `p98-frames-shots.spec.ts:172` are CSS `background-clip` reads. 93 `boundingBox` uses in all.

## Commander's command for the shared run (stack: ports 8781 / 5781 / 9481 or your own)
Stack env: `SR_ADMIN_REQUIRE_REAUTH=0 SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9 SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]' SR_ACTIVITIES__ENABLED=true SR_STORAGE__ENDPOINT=memory:`. Then from `e2e/`, after the sweep seed, in this order on one world:
`SR_P99_REACH_ON=1 SR_P99_REACH_OUT=<f> ... npx playwright test tests/p99-reach-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`; `SR_P98_CUE_ON=1 SR_P98_CUE_OUT=<f> ... tests/p98-cue-shots.spec.ts`; `SR_P100_RTL_ON=1 SR_P100_RTL_OUT=<f> ... tests/p100-rtl-shots.spec.ts`; cue again (each with `SR_E2E_BASE_URL=http://localhost:<vite>`, `SR_CLI_BIN`, `SR_DATABASE__URL`, `DATABASE_URL`). Do not run more than about five registering specs on one database (429 on `/sr/register`).

## Not done
`p100-tables-shots` base was not needed (round 1 already failed).

## Proposed rows and text
- SEC-AUD-447 closing text: "The cue and rtl specs cut their edge strips from `e2e/support/edge-strips.ts` (D-957): the box is scrolled into view, strips come from the part inside the viewport, and under 40 px in view is a spec error. Reproduced on the unedited specs (20 invoices: empty clip); with the helper cue, rtl, cue pass on a fresh world with 10, 15 and 20 invoices." Note for the row: one cue failure at 10 rows (wrapper fully in view, right strip at scroll end) was seen once and not reproduced.
- AGENTS.md sentence (already in the plan): a spec that reads pixels at a box's edge uses `e2e/support/edge-strips.ts` and asserts on the rows it made itself.
- Changelog sentence: "Test tooling: the Invoices edge-shade specs take their strips from a shared helper that brings the table into view, so they no longer depend on how many invoices other specs left in the world."
- No threat-model row.

## Owner questions
None.

## Cleanup and lock
Stack stopped by PID tree, ports 8781 / 5781 / 9481 free; database `sr_p101_strips_ui` dropped; `p101-strips-bin` deleted; the temporary debug spec `zz-strips-debug.spec.ts` was untracked and deleted before the commit. `heavy.sh status` outputs are at the end of this file.

## heavy.sh status
After my last release (06:57): `free`. Just before hand-back (06:58): `chan 06:56` (another builder's hold; none in my name, no `acquire` of mine running, no shell of mine alive). Early in the session two of my `acquire` calls were moved to the background by the tool timeout; both were stopped, and one of them took the lock after being stopped (status showed `strips 06:36`); I released it with `heavy.sh release strips` and it showed `free`.
