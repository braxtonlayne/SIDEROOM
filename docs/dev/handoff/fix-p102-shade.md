# Wave 102, shade: the Invoices edge shade leaves no 1 px line (SEC-AUD-454, D-965)

## Times (2026-11-18, local)
Lock slot 2 from 09:38; stack up 09:39; two runs of my spec (09:40 and 09:46); tables (after), cue and rtl 09:47 to 09:50; stack stopped, database dropped, released about 09:51 (about 13 min: three over the 10, kept to run the real order in the same world). Checks after the last edit: prettier (donations.css, the spec) exit 0; `node scripts/check-css-tokens.mjs` exit 0; e2e `npx tsc --noEmit -p tsconfig.json` exit 0; i18n audit and doc links: see "Checks".

## The fault is not as the plan says (found by pixels)
The plan said the cover ends half a pixel short of the edge and proposed covers 1 px past the edge. **That does not work.** I built it (covers at `left -1px` / `right -1px`) and ran it: the line stayed. Cause, measured: the wrapper's width is fractional (585.28 px at 1024) while the scroll area is a whole number of pixels, so the local cover is clipped to the scroll area and the wrapper's last partial pixel is outside it; the scroll shade shows there. Positioning or widening the covers (-1, -2, 40 px wide at -4; a probe of five variants at three fractions) changes nothing; only moving the shade off that pixel does. Options (1) and (2) of the task were rejected by the pixels; option (3) is built, with a width change so the darkest column stays.

## Built (donations.css, the shade block and its comment only)
Covers 36 px wide (opaque for 18 px, was 32 / 16), positions unchanged; shades 17 px wide (was 16) standing 1 px in from each edge (`left 1px center`, `right 1px center`). Four layers, backgrounds only, same tokens, same 721 px query. The comment names SEC-AUD-454 and D-965. Nothing else.

## Fraction table (my spec, one world; off / base / after in the same page; base = the wave 98 declarations injected word for word)
"flat" = strip identical to the rule-off strip. The table's width is forced by a test harness step (header of the spec): max(clientWidth + 81, natural width) + f.
| theme | width | f | end, right strip: base | end, right strip: after | start, left strip: base | start, left strip: after |
|---|---|---|---|---|---|---|
| light | 1024 | 0, 0.25 | flat | flat | flat | flat |
| light | 1024 | 0.5, 0.75 | **line at x15 (rgb 223 vs 255)** | flat | flat | flat |
| dark | 1024 | 0, 0.25 | flat | flat | flat | flat |
| dark | 1024 | 0.5, 0.75 | **line at x15 (rgb 75,77,82 vs 49,51,56)** | flat | flat | flat |
| light, dark | 900 | all four | flat (the 900 wrapper is 496 px, a whole number: no line even in the base) | flat | flat | flat |

So the line appears only where the wrapper is fractionally wide and the scroll width rounds up (1024: wrapper 585.28, scroll 667). The left strip at start was never wrong in the base. The first run (covers at -1 px) also left a faint 1-level column at x15 on the start side: another reason it was dropped.

Shade kept: the strip at the side with more beyond it differs from off in 15 columns (base 16; the 1 px gap at the very edge). Its darkest column (largest over columns of the mean over rows of the largest channel difference to off), after against base: light 1024 start/right 29.8 to 30.0 against 28.6 to 29.0, end/left 29.3 / 28.9 against 29.9 / 29.3; 900 about 21.3 against 21.2 and 29.8 / 29.4 against 30.5 / 29.9; dark 1024 24.2 against 23.1 and 24.0 against 24.6. Within 2 everywhere (largest difference 1.2). All 32 lines are in `visual-sweep-shots/p102-shade2/table.txt`.

A table that fits (1280, wrapper 828/828, four invoices): strips equal to rule-off at both edges, light and dark, after (and in the base in that run). 375: wrapper `background-image: none`, page does not scroll sideways.

Real order, same world: `p100-tables-shots` (after) `problems []`; `p98-cue-shots` `problems []` with `start Lfalse Rtrue; mid Ltrue Rtrue; end Ltrue Rfalse` in light and dark and FIT 1200 `Lfalse Rfalse`; `p100-rtl-shots` `findings []`, `specErrors []`.

## STACK-ROUND
Slot 2 (`status` showed `toolshots 09:38`, `shade 09:38`). Round 09:38 to about 09:51: stack on 8782 / 5782 / 9482 with the public rate source and `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`; seed with no FAIL; my spec twice (first: covers -1 px, failed with the figures above; second: the final sheet, `problems []`); tables; cue; rtl. Registrations in the world: my spec twice (2 accounts each), tables, cue, rtl; no 429. Ports free afterwards (only TIME_WAIT sockets), database `sr_p102_shade_ui` dropped, `p102-shade-bin` deleted.

## TYPECHECK
Not needed: a sheet only (no `.ts` or `.tsx` under `apps/web`; the new spec is under `e2e/` and passed e2e `tsc`).

## Who imports donations.css
Only the Admin donations sections (lazy Admin chunk); not first paint, not a public page. No web build.

## Shots
Final run, `visual-sweep-shots/p102-shade2`, all six opened: `shade-1024-end-f50-lounge-light.png` and its `base-` twin (the base's 1 px line is only clear in the zoom), `strip-r-end-f50.png` and `base-strip-r-end-f50.png` (16x zoom: the base has a grey column at the right, after none), `shade-1024-start-lounge-dark.png` (shade at the right edge, none at the left), `shade-1280-fits-lounge-light.png` (no shade). First run (covers -1 px) in `visual-sweep-shots/p102-shade`; tables, cue and rtl shots in `.../p102-shade/{tables,cue,rtl}` (not opened one by one; their specs assert).

## Commander's command (group E; stack env `SR_ADMIN_REQUIRE_REAUTH=0 SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9 SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]' SR_ACTIVITIES__ENABLED=true SR_STORAGE__ENDPOINT=memory:`)
From `e2e/`, after the sweep seed on the same database (ports 8782 / 5782): `SR_P102_SHADE_ON=1 SR_E2E_BASE_URL=http://localhost:5782 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P102_SHADE_OUT=<folder> npx playwright test tests/p102-shade-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`. It injects the base declarations itself, so it works on any tree.

## Closing text for SEC-AUD-454
"The Invoices edge shade no longer leaves a 1 px line at scroll end. Cause: the wrapper is fractionally wide (585.28 px at 1024) and the scroll area rounds up, so the wrapper's last partial pixel lies outside the local covers and the scroll shade showed there; moving the covers does not help. Fix (D-965): the two scroll shades stand 1 px in from the edge and are 17 px wide, the covers are 36 px wide. At four fractions, both ends, 1024 and 900, Lounge Light and Dark, every strip at an edge that is at its end equals the shade-off strip; the shade's darkest column is within 1.2 levels of before; cue and rtl pass."

## Proposed rows
- D-965 should read: "An edge shade's scroll layers stand 1 px in from the wrapper's edge (and are 17 px wide), so no shade paints on the wrapper's last partial pixel, whatever fraction its width has." The plan's "the cover overlaps the edge by a pixel" is what I disproved.
- No new inventory or threat-model row.

## Changelog sentence
"Admin > Donations > Invoices: the soft shade at the table's edges no longer leaves a 1 px line at the end of the scroll."

## Owner questions
None.

## Checks
See the first lines and the commit message; i18n audit and doc-link results are in the final reply.

## heavy.sh status
After my release: `pageshots 09:44` (not mine; no hold in my name; no acquire of mine alive). Two of my `acquire` calls were moved to the background by the tool timeout; both ended (the first took slot 2 at 09:38, the second answered "already holds").
