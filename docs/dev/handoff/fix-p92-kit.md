# Wave 92, kit: SEC-AUD-401 (D-913), the kit `Select` chip rows get a 44 px pitch on a phone; and the wave's one web build

T = 11:47 (2026-11-17). Spec written and typechecked by 11:46; lock taken 11:54 (queued behind bins and lounge); one stack round 11:54 to 12:03 (base and after in the same round); a short second round 12:08 to 12:10 for the empty control (my spec's first version pressed Escape with no chip chosen, which closed Settings: a probe error, fixed in the spec, not a kit fault). Web build: see the last section.

## Done
- `packages/ui/src/primitives/Select/Select.module.css`, the two 720 px blocks only: `.inputRow { row-gap: 2px }` (was the 4 px `gap`'s row part) and `.chip { margin-block: 11px }`; comments reworded to name D-913. The `::after` target (D-879) is unchanged. No rule outside the 720 px blocks; no theme, section or app name in the sheet.
- `e2e/tests/p92-kit-shots.spec.ts` (new). Commits: `3f52c7bc` (rule and spec), plus a spec-only fix.

## The form kept, and why (both measured or derived)
- The proposal (`row-gap: 0`, `.chip { margin-block: 12px }`) was not kept: it makes a chip line 44 px against the text input's 42 px margin box (the input is a 52 px box with `margin-block: -1px`), so a one-row control would grow by 2 px. Kept: every line stays 42 px (chip 20 + 2 x 11) and the 2 px sits in the row gap, so the pitch is 44, each button's 44 px target lies inside its own row's band (the first row's reaches 1 px into the control's border), and the input's line and a one-row control are unchanged. The proposal was derived, not run; the kept form was measured. The other form (row-gap 24 px with block padding) gives the same height and the padding would move a one-row control, so it was not tried.
- Brief's figure "one-row control 44" is wrong on the base: with chips, with one chip row and with none the control is **52 px** (input 52 px box). After equals base in all three states.

## Figures, 375 x 812 touch, Lounge Light, Onboarding "Default channels", nine chips p92-r1..r9 (spec ran both cases)
| | base | after |
|---|---|---|
| control box | 343 x 124 | 343 x 184 (+60: three chip lines 20 to 42 = +66, two row gaps 4 to 2 = -6) |
| row pitch | 24, 24 | 44, 44 |
| buttons failing up/centre/down (21 px) | r1..r6 fail "down" (6 of 9); 9 centres pass | **0 of 9** |
| own run below the centre (1 px walk) | 1 px for r1..r6, 21 for r7..r9 | 21 px for all nine (up: 22) |
| chevron (3 points) | pass | pass (326 x 184 box) |
| touch tap 20 px below r2's centre | removed r5 (wrong) | removed r2 (right) |
| touch tap at r6's centre | removed r6 | removed r6 |
| text input box | 297 x 52 | 297 x 52 (moved down with the rows: top 386.2 to 446.2) |
| top of what lies below the field | 458.2 | 518.2 (+60 = the control's growth exactly) |
| sideways scroll | 375/375 | 375/375 |
- Two chips (one row) and no chips: control 343 x 52, input box, placeholder ("Default channels everyone sees") and chip boxes equal to the base, 0 px (round 2 for the empty control, `visual-sweep-shots/p92-kit-none/`).
- Lounge Dark after: same figures as Light (0 failing, pitch 44, 343 x 184, no sideways scroll).
- The list opens under the three-row control (control 16,314.2,343,184; list at top 503 = control bottom 498.2 + 4.8); a tenth value (p92-r10) chosen makes its chip (10 chips); with the keyboard Backspace removes the last chip (10 to 9).
- 1280 x 800 Editorial, nine chips: control 720 x 62, every chip's box, the input box and the top of what lies below equal to the base, 0 px.
- Second multi-select: Onboarding, answer 1 of question 1 "Channels" (Add question, Add answer; six chips in two rows at 375 Lounge Light): control 271 x 184, pitch 44, 0 of 6 failing, nothing cut or overlapping.
- Vitest: `packages/ui/src/primitives/select.test.tsx`, 8 passed (not run on the base; a module sheet cannot change it).

## Shots (`visual-sweep-shots/p92-kit`, 14 PNGs plus `base-measure.json` and `after-measure.json`; 7 listed, all 7 opened by me after the release)
base-nine-375-lounge-light, after-nine-375-lounge-light, after-nine-375-lounge-dark, after-two-375-lounge-light-rows (one row), after-list-open-375-lounge-light-list, after-nine-1280-editorial, after-second-multiselect-375-lounge-light. `data-theme` was read back for each (the account theme was set through the API and read back). The other PNGs are duplicates of the same states from the b and c cases.

## Re-shoot command (commander, from `e2e/`, on a stack with the sweep world, API 8702, Vite 5702, metrics 9402, `SR_ADMIN_REQUIRE_REAUTH=0`)
`SR_E2E_BASE_URL=http://localhost:5702 SR_P92_KIT_ON=1 SR_P92_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-kit npx playwright test tests/p92-kit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(`SR_P92_KIT_BASE=1` for the base case with the sheet switched off; `SR_P92_KIT_ONLY=a,b,c,d,e,f` picks cases.) It creates channels p92-r1..r10 in the world's guild (never saved settings).

## Docs
No doc states the chip pitch. `node scripts/check-doc-links.mjs` run at the end (see below).

## Proposed rows
- SEC-AUD-401 closes: "kit Select chip rows on a phone: pitch 44 (`.inputRow` row-gap 2 px, `.chip` margin-block 11 px, 720 px block); 0 of 9 buttons fail the 21 px hit test; a tap 20 px below an upper chip removes that chip; one-row and empty controls 52 px, equal to the base; 1280 unchanged (D-913)."
- Note for D-913's text: the unchanged one-row/empty control height is 52 px, not 44.
- Changelog sentence: "On a phone, the chips of a multi-select (for example Server settings, Onboarding default channels) are 44 px apart, so a touch under an upper chip's remove button no longer removes the chip below."
- Owner questions: none. (D-914 untouched: `.us-stack` gap stays 14 px.)

## Cleanup
Scratch database dropped, `p92-kit-bin` deleted, stack stopped by PID tree (8702 / 5702 / 9402 free), lock released. No stash touched.

## The web build (run 12:34 to 12:35 after every builder's handoff was in; `heavy.sh run kit`, exit 0, under a minute)
- `node scripts/check-bundle-budget.mjs`: all ok. First paint `app/app-hQRZEEuG.js`: **218 359 B gzip, room 37 641 B** (needs 8 KB). Before (wave 91): 218 353 B. **+6 B**; my change is CSS-only in a module sheet, so I do not know the cause of the 6 B (the first-paint script is JS; it may be the base differing from wave 91 by the wave 92 base commits). Not chased.
- `node scripts/check-entrance-isolation.mjs`: every public surface is clean. `node scripts/check-staff-delivery.mjs`: ok.
- Nothing the build wrote was committed. `sr_p92_kit_ui` dropped and `p92-kit-bin` deleted at 12:11.
