# Wave 90, members: SEC-AUD-387 (D-898), Members' checkboxes are the kit's `Checkbox`

T = 05:50 on 2026-11-17. Clock: code and vitest 05:51-05:57, first commit 05:58, lock 06:00-06:01 (one round: stack, seed, base, after), shots opened 06:03.
Base: `integration/phase90` at `35af3dc7` (the `p90-members` worktree started there).

## Done (commit `dfa7b176`)
- `Members.tsx`: the header's "select all" and the row's checkbox are `<Checkbox label={null} aria-label=... checked onChange>` from the kit (header with `indeterminate`, no ref). No visible text. The kit draws without a visible label (an empty `<label>` beside the box); no kit change.
- `server-settings.css`: wave 85/86's Members checkbox rule is removed (`.ss-table-stack input[type='checkbox']` margin/size and its `::after`). `git grep` shows `ss-table-stack` is used by Members and Invites only, and Invites has no checkbox. **That rule also had to go because it matched the kit's input and would have drawn over its check mark (`::after`).** Four selectors in the same media block changed from `td:has(> input[type='checkbox'])` to `td:has(input[type='checkbox'])` (the kit wraps the input in a div), otherwise the name would drop under the box. These four are outside "the hit-area rule only"; flagging them. The old rule was removed in the same commit, so the after case ran with it gone.
- Vitest (`__tests__/people.test.tsx`, the file that renders Members): new case "SEC-AUD-387 ...": row checkbox found by accessible name and toggles, it sits in the kit's choice row with a label element, select-all shows `aria-checked="mixed"` with one row ticked, ticks everything, mixed again after unticking one. Run: the `Members` cases, 14 passed (the first case of the file timed out once on a cold start and passed alone). Not run against the base: the native input has no `aria-checked` and no label, so the new case cannot pass there.
- Spec `e2e/tests/p90-members-shots.spec.ts` (command in its header).

## Who imports the file (lazy chunk)
`sections/Members.tsx` is imported only by `settings/server/ServerSettings.tsx:37` (`MembersSection`), the Settings chunk. No web build run.

## The base (run: yes, same stack, Members file and sheet switched to `HEAD` for that run)
375 (Lounge Light and Dark identical): row checkbox 20x20 at x 28, hit test cy-21/+21 true, first row 202 px, second 180, first cell 44 wide, table 343 wide (x 16), bulk bar with one row ticked [16,186,343,64]. The 44 px came from `server-settings.css` `.ss-table-stack input[type='checkbox']::after {inset:-12px}` (margin 12, 20 px box); no other selector matched it.
1280 Editorial: box 13x13, first row 59 px, first column 36, table 828 wide, bulk bar [340,263,828,28].

## After (Chromium only)
- 375 Lounge Light and Lounge Dark (same figures): box 18x18 at x 16 (kit input; its `::before` is the target), hit test cy-21 and +21 true for both rows, every other control of the row (name link, Roles, More actions) hits itself at its centre, so no target covers a neighbour. Taps 15 px below, beside and above the centre tick that row only. Row heights 202 and 180 (unchanged), table 343 (unchanged), no sideways scroll (0). First cell 44 -> 28 (the name link sits 16 px further left, x 108 -> 92). With one row ticked the bulk bar is [16,186,343,64], as before; the header shows `aria-checked="mixed"`. "Select all" is still the visually hidden table head on a phone (1 px, clipped, as in the base: it is in the DOM for a screen reader, D-890).
- 1280 Editorial: box 18x18; first row 59 (same), table 828 (same), bulk bar same box. **First column 36 -> 44 (+8 px) and the name column starts 8 px further right (x 424 -> 432): over your 2 px limit.** Cause: the kit's choice row has a 10 px gap before its (empty) label. Not fixed (the kit is fenced; a rule for it would be a new rule in a shared sheet). Row heights and table width did not move. Owner question 1.
- "Select all" at 1280: ticks the page (both rows), mixed state with one unticked (`aria-checked="mixed"`, `indeterminate`), shot.
- Keyboard at 1280: Tab reached "Select all", then the row checkbox; Space ticked "Select Juniper"; focus ring `solid 2px rgb(217, 201, 164)` and `:focus-visible` true.
- Ratio helper: none used.
- Target width on a phone is the kit's: 18 px box + 6 px each side = 30 px wide by 46 tall (the D-856 hit test is the vertical one and passes). Owner question 2.

## Tests / checks
prettier, eslint (Members.tsx, people.test.tsx), web `tsc`, e2e `tsc`, `check-css-tokens`, `i18n-audit --max 0`: clean. `check-doc-links` run at the end. Spec run: `--project=chromium`, one passed on the base and one on the after, no failed guards.
**Firefox and Safari were not run locally** (D-898).

## CI spec that opens Members
None of the CI specs (not `p[0-9]*`, not the sweep) opens Members: `bridge-whole-guild`, `guest-play` and `view-as-role` only match the word. Wave 91 can add the hit test for Firefox and WebKit as a new CI spec.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-members`: 11 PNGs; the 6 asked for (base 375 Light; after 375 Light, 375 Dark, 1280 Editorial; one row ticked at 375 Light; select-all mixed at 1280) plus base 375 Dark, base 1280, after 375 Dark ticked, after 1280 keyboard. I opened the 6 asked for, all fine (Light and Dark: box, name and avatar aligned, check mark drawn, mixed shows a dash).

## Commander's command
From `e2e/` on a stack with the seeded world (API 8683, Vite 5683):
`SR_E2E_BASE_URL=http://localhost:5683 SR_P90_MEMBERS_ON=1 SR_P90_MEMBERS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-members npx playwright test tests/p90-members-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P90_MEMBERS_BASE=1` for the base case).

## Inventory row (proposed)
387 | members | Members' checkboxes are the kit's. | Fixed: both are the kit `Checkbox`, the `::after` hit area is gone (D-898); 1280 first column +8 px (kit gap). | `Members.tsx`, `server-settings.css`

## Changelog sentence
Server settings, Members: the row checkbox and "select all" are the same control as everywhere else, and no longer rely on a hit area only Chromium is known to honour.

## Owner questions
1. At 1280 the checkbox column is 8 px wider (the kit's 10 px gap). Accept, or ask for a small override in `server-settings.css` for the Members cell?
2. On a phone the kit's checkbox target is 30 px wide by 46 px tall (it was about 44 square before). Accept, or widen the kit's `::before` for every checkbox?

## Cleanup
Scratch DB `sr_p90_members_ui` dropped; `p90-members-bin` deleted (`p80-base-bin` untouched); stack stopped by PID tree, ports 8683/5683/9383 free; lock released. No stash touched.
