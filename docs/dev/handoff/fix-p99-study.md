# Wave 99, study: SEC-AUD-439 (D-947), Study's dots and flecks run the whole scrolled height on a phone; and the wave's one web build

2026-11-17. Clock: started 16:55; CSS and spec written and typechecked 16:56; lock `study` 16:56 to 16:59 (one round: stack, seed with no FAIL line, my spec three times: the first two were figure-only, see "Spec"); released 16:59; web build 17:00 to 17:01 (`heavy.sh run study`); commit 7f349945 at 17:00. Ports 8771 / 5771 / 9471 free, `sr_p99_study_ui` dropped, `p99-study-bin` deleted. Files: `apps/web/src/player/skins/backdrop/study.css`, `e2e/tests/p99-study-shots.spec.ts` (new), this handoff. No TSX, no other sheet, no cargo.

## Base (confirmed first, 700 x 812 and 375 x 812 touch, the block switched off by an injected style)
- 700: root client / scroll height 723 / 836. `.st-dots` and `.st-grain` both 8 to 731 (h 723): 105 px short, as wave 98.
- 375: 723 / 1250. Both spans 8 to 731: 519 px short of the scroll end (wave 98 said about 527; mine is the measured one).

## What changed (one block at the end of `study.css`, under `@media (max-width: 720px)`, comment names SEC-AUD-439, D-947, D-940)
On `.sr-ply-study` the background gets three texture layers on top of the cream and pink layers: the dots tile (18 x 18) and the two flecks tiles (7 x 9 at 3px 5px; 11 x 7), same gradients and sizes as the spans, each `local` and `padding-box`. Element opacity became `color-mix(in oklab, <token> 55%, transparent)` for the dots and `60%` for the two flecks colours. `.st-dots` and `.st-grain` are `display: none` in the block. No change above 720 px.

## After (same run as the base: on = as built, off = base restored by an injected style; animations stopped in both so a pixel that differs is the texture)
| | 700 | 375 |
|---|---|---|
| root client / scroll height, on vs off | 723 / 836 both | 723 / 1250 both |
| controls (22 in the root), moved | 0 (at start and at end) | 0 (at start and at end) |
| lowest 80 px strip at scroll end, pixels differing, largest channel step | 172 of 54 720, 31 | 55 of 28 720, 31 |
| first screen at scroll start, pixels differing | 11 969 of 505 476, largest step 36 | 5 147 of 265 301, largest step 36 |
| root / document scrollWidth vs clientWidth | 668/668, 700/700 | 343/343, 375/375 |
| hit test (`openCollapsed`, `hitTestAll`) | on 22 of 22, off 22 of 22, 0 fail, 0 not-reached | not run (task: 700) |

- The strip differs at both widths: the texture is there now where the base had none. It differs in few pixels because the strip is mostly the opaque cards; the texture shows in the gaps between them.
- First screen: the pixels that differ are faint. At 700 (decoded with a small script of mine): 11 632 differ by 1 to 3 levels, 229 by 4 to 9, 106 by 10 to 19, 2 by 20 or more (largest 36). Mixing in the layers against element opacity (the two flecks are no longer one faded group) accounts for it; a shifted tile would give thousands of steps of 10 or more, and there are 108. At the scroll end the same count by size: 61 pixels at 20 or more, all on the rim columns (x 14 and 672) at rows 606 to 624, one fleck column at the skin's edge; I list it and did not chase it. Listed as the faint mixing difference; no tile origin fix.
- 1024 x 768 (no touch): the root's computed `background-image` equals the 700 "off" string; `.st-dots` and `.st-grain` display `block`: the block does not apply.
- By eye: at the end shots (`study-700-end-before.png` against `study-700-end.png`, and 375) I can not tell the two apart, the dots and flecks are very faint on the cream; the figures show it. The start shots look the same.

## Spec
`e2e/tests/p99-study-shots.spec.ts`: base and after in one run. Its first run compared moving frames (the turning record, the title marquee): 31 582 pixels differed with steps up to 189. I stopped animations with an injected style in both states and ran again; the figures above are that run (third run, 16:59; the second only repeated the first because my edit had not been written). Spec errors, not page faults.

## Shots (`visual-sweep-shots/p99-study`, 8 PNG and `study.json`; I opened 8 of 8)
`study-700-end-before.png`, `study-700-end.png`, `study-700-start.png`, `study-700-start-before.png`, `study-375-end.png`, `study-375-end-before.png`, `study-375-start.png`, `study-375-start-before.png`. No crowding or cut in the on shots; the 375 start (title, controls wrapping) equals its before shot.

## Web build (17:00 to 17:01, tree of commit 7f349945, the CSS in it is the CSS I measured; the build log is in the scratchpad)
`pnpm --filter @sideroom/web build`: done. `study.css` lands in the lazy chunk `app/StudySkin-B4Bsqb8a.css` (the player's Study skin): not first paint. `check-bundle-budget.mjs`: exit 0, first paint `app/app-CJI_s49S.js` 218 359 B gzip, room 37 641 B (before 218 365 B, room 37 635 B: 6 B less, the chunk hash). `check-entrance-isolation.mjs`: exit 0, "every public surface is clean". `check-staff-delivery.mjs`: exit 0.

## Checks (own tree, after my last edit, 17:00)
e2e `npx tsc --noEmit -p tsconfig.json` exit 0; prettier --check on the sheet and the spec exit 0; `check-css-tokens.mjs` exit 0 (73 files clean); `check-doc-links.mjs` run after this handoff (see the commit message of the handoff: exit 0). The CSS in my last commit is the CSS I measured and the CSS the build ran on.

## Commander's command (from `e2e/`, stack with the sweep's world, API 8771 / Vite 5771 / metrics 9471, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set)
`SR_E2E_BASE_URL=http://localhost:5771 SR_P99_STUDY_ON=1 SR_P99_STUDY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p99-study npx playwright test tests/p99-study-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

## Closing text for SEC-AUD-439
Closed in wave 99 (D-947): at 720 px and under Study's dots and flecks are the root's own `background-attachment: local` layers (tile colours mixed with transparent in place of element opacity) and the two absolute spans are off there, so the texture runs the whole scrolled height (was 105 px short at 700, 519 px at 375). Roots, controls and hit test unchanged (22 of 22 at 700); nothing above 720 px changes.

## Changelog sentence
On a phone with the Study skin the polka dots and paper flecks now run to the foot of the scrolled skin.

## Owner questions
None.

## Stash and lock
No stash touched. After release (16:59) `heavy.sh status` printed `free`; after the build (17:01) `free`; just before the reply: see the final report.
