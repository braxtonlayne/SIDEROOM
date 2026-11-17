# Wave 91, kit builder: SEC-AUD-394 (D-904), SEC-AUD-395 (D-905), SEC-AUD-396 (D-906); the wave's one web build

T = 10:48 (2026-11-17). Binaries copied 10:46, spec and code ready 10:50, lock taken 11:02 (round 1, one round for all three items), released 11:09 (stack stopped by PID tree, ports 8691/5691/9391 free). Web build: taken 11:23 as a one-shot `heavy.sh run kit` (the lock was free, every builder had had a first round), ended 11:24, lock released by the one-shot; finished 11:27. Branch `fix/p91-kit`.

## Done (commits)
1. `7f1d726f` SEC-AUD-394: guestbook "Sign" button hover and pressed keep 4.5:1.
2. `a4a693c2` SEC-AUD-395: kit choice control with no label draws no label; `ss-check-solo` removed.
3. `dd081bbb` SEC-AUD-396: `FileDrop`'s drop text is a prop.
4. the spec `e2e/tests/p91-kit-shots.spec.ts` (own commit) and this handoff.

All three items fit one stack round. Nothing dropped.

## Item 1: SEC-AUD-394 (D-904)
**Base (I ran it: `SR_P91_KIT_BASE=1`, `/u/owner`, sweep world, a note typed and not sent; identical in the three settings, because the profile's colours do not change with the app theme):** rest rgb(79,91,211), text 5.04:1; hover rgb(104,113,216), **3.82:1**; pressed (mouse down, released off the button so nothing was sent) the same rgb(104,113,216), **3.82:1** (there was no `:active` rule); keyboard focus 5.04:1 with the ring `solid 2px rgb(242,243,245)`; disabled 7.72:1.
**Rules as built** (`packages/profile-blocks/src/profile-blocks.css`, the comment line names SEC-AUD-394 and D-904; only `--pp-*` variables, no `--ui-*`, no theme or section name):
```css
.pp-sign > button:not(.pp-link):not(:disabled):hover  { background: color-mix(in srgb, var(--pp-accent) 70%, var(--pp-bg)); }
.pp-sign > button:not(.pp-link):not(:disabled):active { background: color-mix(in srgb, var(--pp-accent) 60%, var(--pp-bg)); }
```
**After (figures from `e2e/support/contrast.ts`; the helper's self-check printed 7.70 and 7.73 for 7.70 and 7.72):** rest rgb(79,91,211) **5.04:1** (as before); hover rgb(71,80,180) **6.16:1**; pressed rgb(65,73,159) **7.07:1**; focus 5.04:1, ring `solid 2px rgb(242,243,245)` (as before). Hover's ground against rest's ground **1.22:1** (needs 1.15; 70% stays), pressed's against rest's 1.40:1. The same in all three settings (375 Lounge Light, 375 Lounge Dark, 1280 Editorial; account theme PATCHed and read back each time). Disabled (7.72:1), the rest and focus grounds, ratios and boxes, and the signed-out `/~owner` form (ground rgb(128,129,131), 2.27:1, box) are **equal to the base**; the boxes in hover and pressed equal the base: 0 px moved. Document sideways scroll 0.
Vitest: `packages/profile-blocks/src/blocks.test.tsx` as it is: 21 passed.

## Item 2: SEC-AUD-395 (D-905)
**Base (ran it):** Members (the seeded server, `ss-check-solo` on): 1280 Editorial: first column 34, row 59, table 828, `label` elements in the first cell 1 (the empty one); 375 Lounge Light: first cell 44, box 18 x 18 at x=29, the hit test true 21 px above, below, left and right of the box's centre, one label in the first cell. Labelled controls on five user Settings pages (Accessibility, Profile, Privacy, Notifications, Text) at 375 and 1280: switches 38 x 22 and checkboxes 18 x 18 (Privacy has both), row heights, label x.
**Rules as built:** `Controls.tsx` `ChoiceShell`: `solo = (label is null, undefined, false or '') && !description`; when `solo`: no `<label>`, `data-solo` on the root, the input keeps every prop (`aria-label` arrives through `inputProps`). `Controls.module.css`: `.choice[data-solo] { gap: 0 }`; in the existing `@media (max-width: 720px)` block `.choice[data-solo] { padding-inline: 13px }` and `.choice[data-solo] .input::before { inset-inline: -14.5px }` (one comment, SEC-AUD-395, D-905). `Members.tsx`: both `className="ss-check-solo"` removed; `server-settings.css`: the three `ss-check-solo` rules removed, the "44 px targets" comment reworded to name D-905; `people.test.tsx`: the class assertion replaced by "no `label` element in the cell, the input has its `aria-label`".
**After (ran it, same round):** **every Members figure equal to the base** (the whole measurement object compared, only the empty-label count changed from 1 to 0): 1280 first column 34, row 59, table 828; 375 first cell 44, box 18 x 18, hit test true on all four sides; the bulk bar's box with one row ticked equal (375: 16,186,343 x 64; 1280: 340,263,828 x 28); select all (both rows ticked), Space toggles (Select Juniper checked), the focus ring `solid 2px rgb(217,201,164)` with `:focus-visible` true: all as base. The labelled Checkbox and Switch of the five user Settings pages: **every box, row height and label x equal to the base**, at 375 Lounge Light, Lounge Dark and 1280 Editorial. `git grep -n ss-check-solo` returns nothing outside `docs/` (and this spec's header comment, which names it).
**The label-less `Switch`/`Radio` finding:** `git grep` for `label={null}` / `label=""` (and `label: null`) in `apps/web/src` and `packages` finds only the two Members checkboxes. There is no label-less `Switch` or `Radio` anywhere; their insets are left as they are (a solo `Switch` would need its own measured inset: 38 px wide box, so `inset-inline: -3px` would give 44; not built, not needed).
Vitest: `forms.test.tsx` (new case: a label-less `Checkbox` renders no `label`, has `data-solo`, the `aria-label` is on the input; a labelled one has its label and no `data-solo`), 19 passed; `people.test.tsx`, 17 passed. I did not run them against the base.

## Item 3: SEC-AUD-396 (D-906)
`FileDrop.tsx`: `dropText?: ReactNode` with a doc comment; the span renders `dropText ?? (multiple ? 'or drop files here' : 'or drop a file here')`. New fragment `apps/web/src/i18n/messages/en/p91-kit-1.ts` (`kit.fileDrop.orDropOne`, `kit.fileDrop.orDropMany`). `EntranceHero.tsx` (that prop is the only change; the refusal text is as wave 90 built it, D-908) passes `t('kit.fileDrop.orDropOne')`; `Expressions.tsx` passes `orDropMany` on the emoji picker (`multiple`) and `orDropOne` on the sticker picker.
**After (ran base and after):** at 375 Lounge Light, Lounge Dark and 1280, Admin's Entrance page ("or drop a file here") and the server's Expressions page ("or drop files here") read as before; the drop box, the button box and the text's position are equal to the base; the text span is 109.2 px wide against 109.3 (and 105.1 against 105.2), a sub-pixel difference from the text now being one string instead of three nodes. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0.
Vitest: `misc.test.tsx` (new: the English default, the plural default with `multiple`, a passed `dropText` replaces it), 29 passed. No test of the two callers asserts the text (`git grep -n "drop a file" -- apps/web/src` finds none).

## Tests and checks run
- Single vitest files: `forms.test.tsx` 19, `misc.test.tsx` 29, `people.test.tsx` 17: all passed. (The new cases were not run against the base.)
- web `tsc --noEmit`, e2e `tsc --noEmit`: clean. prettier --check, `pnpm --filter @sideroom/ui exec eslint` and `@sideroom/web exec eslint` on the changed files: clean. `node scripts/check-css-tokens.mjs`: 73 files clean. `node scripts/check-doc-links.mjs`: ok. `node scripts/i18n-audit.mjs --max 0`: 0.
- The spec `e2e/tests/p91-kit-shots.spec.ts`, `--project=chromium`: 4 passed in the base run (1.1 min) and 4 passed in the after run (1.0 min). It asserts only the helper self-check (7.70 and 7.72, 0.05 tolerance); the rest is measurement, compared by a script of mine (scratchpad `kit/cmp91.mjs`).
- Base case: run on this worktree with the files of the three commits checked out from `9da8b619` (`git checkout <base> -- <files>`, then `git checkout HEAD -- <files>`), never a stash and never another worktree.

## The web build
`npx pnpm@10.12.1 --filter @sideroom/web build` (the build's own run took under a minute; exit 0), then the three checks, all exit 0:
- `node scripts/check-bundle-budget.mjs`: every line ok. **First paint `app/app-Crcw14gI.js`: 218 353 B gzip, room 37 647 B to the 250 KB limit (needs 8 KB). Before (wave 90): 218 312 B. +41 B** (the two `dropText` props, the solo branch of `ChoiceShell`; the `profile-blocks.css` change and the fragment are not in the first-paint script).
- `node scripts/check-entrance-isolation.mjs`: "every public surface is clean" (auth 121.4/150 KB, invite 113.5/120, profile 115.1/150, guest-play 115.5/120).
- `node scripts/check-staff-delivery.mjs`: ok (7 staff files, 731.6 KB).

## Shots (folder `visual-sweep-shots/p91-kit`)
The spec made 19 `after-*` and 19 `base-*` shots (hover, pressed and rest of the Sign button; Members; the labelled-controls page; both pickers; per setting). **I opened the 11 the task lists, all of them:** `base-sign-hover-desk-editorial`, `after-sign-hover-desk-editorial`, `after-sign-pressed-desk-editorial`, `after-sign-rest-phone-light`, `after-members-phone-light`, `after-members-phone-dark`, `after-members-desk-editorial`, `after-members-selected-desk-editorial` (bulk bar, mixed header), `after-labelled-controls-phone-light`, `after-drop-entrance-phone-light`, `after-drop-expressions-phone-light`. The others (the base shots, the dark and Editorial pickers, the keyboard shot, the signed-out form) I did not open; their figures are in the JSON files. The "labelled checkbox and switch" shot is of Accessibility (switches only); the checkbox figures (Privacy page: `checkbox:Messages in ...`, 18 x 18, row 64 at 375 and 20 at 1280) are in `*-members-measure.json` and are equal to the base. Sign button hover: the base is a lighter, greyer blurple; the after is a visibly deeper one, still the profile's blue; pressed deeper again.

## The commander's command (from `e2e/`, on a stack with the sweep world, API 8691, Vite 5691, metrics 9391, `SR_ADMIN_REQUIRE_REAUTH=0`)
```
SR_P91_KIT_ON=1 SR_P91_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-kit SR_E2E_BASE_URL=http://localhost:5691 npx playwright test tests/p91-kit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
(`SR_P91_KIT_BASE=1` in place of `_ON=1` is the base case; `SR_P91_KIT_PART=sign,members,drop` picks parts.)

## Docs
Nothing outside my files states these facts (`git grep` of `ss-check-solo`, `or drop a file`, the Sign hover: only the decisions log, inventory and old handoffs, which are not mine).

## Proposed inventory rows
- SEC-AUD-394: the guestbook "Sign" button's hover (70% accent / 30% profile bg) and new pressed state (60%) keep 4.5:1 (hover 6.16, pressed 7.07, rest 5.04); only `--pp-*` variables. Closes the wave 90 public Q1.
- SEC-AUD-395: the kit's `Checkbox`, `Radio` and `Switch` with no label and no description draw no label element and no gap (`data-solo`); on a phone a lone checkbox's row is 44 px wide and its target lies inside it; Members' `ss-check-solo` and its three rules removed; Members' figures unchanged; the labelled controls unchanged.
- SEC-AUD-396: the kit's `FileDrop` takes `dropText` (English default); Admin Entrance and the server Expressions pass `t()` (`kit.fileDrop.orDropOne` / `orDropMany`).

## Proposed threat-model rows
None.

## Changelog sentences
- Guestbook "Sign" button: its hover and pressed states now keep at least 4.5:1 contrast on any profile (SEC-AUD-394).
- The UI kit's checkbox, radio and switch with no label draw no empty label and keep a 44 px phone target; the Members table no longer needs its own override (SEC-AUD-395).
- The UI kit's file drop zone takes its "or drop a file here" text as a prop, and the app's two upload pages pass it translated (SEC-AUD-396).

## AGENTS.md sentence (optional)
A kit choice control with no visible label must be given `aria-label` (and `label={null}`); it then draws no label element.

## Owner questions
None. Choices made: hover 70% and pressed 60% as the task gave (hover against rest 1.22:1, no 5% step needed); a solo control with a description keeps its label (the description needs it).

## Cleanup
Stack stopped by PID tree (ports free), lock released at 11:09 and again after the build; `sr_p91_kit_ui` dropped and `p91-kit-bin` deleted (`p80-base-bin` untouched). I found no stash and touched none.
