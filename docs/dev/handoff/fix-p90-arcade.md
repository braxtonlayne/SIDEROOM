# Wave 90 arcade builder: SEC-AUD-388 (D-900)

T = 05:50 on 2026-11-17 (no new step 07:45, checkpoint 08:10, finished 08:20, hard limit 08:30). Code and checks 05:50-05:56; lock taken 05:56 (round 1: seed, base case, after case, released 06:00); the shots showed the page scrolled to the end (heading out of frame), so spec fixed and a re-shoot round taken 06:13-06:15 (lock waited 06:02-06:13; this was a second round, taken only because the shots could not show the heading; the commander may count it). Both released, stack stopped by PID tree, ports 8684/5684/9384 free, scratch DB `sr_p90_arcade_ui` dropped, `p90-arcade-bin` deleted (`p80-base-bin` untouched). No stash touched.

## What I saw on the base (run, `SR_P90_ARCADE_BASE=1`, the three files at `35af3dc7`)
- Heading: bare `<h2 id="arc-admin">`, 24px / 700 (Pricing's `h1.ss-title`: 20px / 700), same font family and colour, same start edge (x 16 at 375, 340 at 1280). It sat 40.9 px lower at 375 (y 110.9 vs 70) and 40.9 px lower at 1280 (88.9 vs 48) because of the `.arc-section` rule above it and the intro paragraph's gap.
- Selects: two native `<select>`s, each inside a wrapping `<label class="arc-field">` with a visible small-caps `<span>` ("Released as", "Content tier"), so they already had a visible label and a name. Box 40 px tall, 1px border, radius 8 (4 at 1280), 16px text. At 375 both **failed the hit test** (303 x 40 and 309 x 40); Pricing's "Who" (kit Select) passes at 44 px. Native arrow, not the kit's.
- `aria-labelledby="arc-admin"` pointed at the h2. No sideways scroll (doc 0 in all three).

## What I built (commit `7c2df093`, plus the spec fix and handoff commit)
- `ArcadeAdmin.tsx`: the heading and intro go through `SectionHeader` (`admin/ui.tsx`, as Pricing does); the section is `aria-label` the same text, because `SectionHeader` takes no id (the `arc-admin` id is gone; nothing else used it: `git grep` finds only this file). Both selects are the kit `Select` with `<label htmlFor>` (ids `arc-release-<id>` and `arc-admin-tier`), same values, handlers and options (the "None" option is the first kit option, value `''`).
- `arcade.css`: only `.arc-section.arc-admin` (no rule above the header) and `.arc-admin .arc-field > label` (the label drawn as the kit's/Pricing's `adm-pick-label`: 12px, 600, uppercase, muted). The old `.arc-field select` rules stay (the Arcade room's List-a-cartridge dialog uses them).
- No string added (the labels existed); no fragment.
- Lazy chunk: `git grep -n ArcadeAdmin` finds `admin/AdminApp.tsx:60` (the Admin chunk) and its own test; no first-paint or public page imports it. No web build.

## Figures after (375 Lounge Light and Dark, 1280 Editorial; `visual-sweep-shots/p90-arcade/measure.json`)
- Heading `h1`: 20px / 700, same family, same colour as Pricing's in each theme, x 16 / 340 and y 70 / 48, identical to Pricing's. (Colours are computed strings compared as strings, no ratio taken.)
- Selects: `button[role=combobox]` named "Released as" and "Content tier" by `<label for>`, visible small-caps label. 44 px at 375 and **pass the hit test** (271.9 x 44, 277.9 x 44); 38 px at 1280 (Pricing's is 38 too). Same kit control as Pricing's, so the same drawn look (the kit wrapper draws border and radius; the trigger button itself reads 0px in both pages).
- Doc sideways scroll 0 in all three.
- Other boxes (top / height, page scrolled to the top): at 1280 the three blocks start 49.8 px higher (candidates heading 214.5 -> 164.7), the form 49.8 px higher and the same 360 tall; at 375 the first h3 is 66.8 px higher and the form 80 px higher, 376 tall instead of 372 (the 44 px selects). That is the heading's smaller type and removed rule/padding, plus the intro line gap, and the select height at 375.
- Flows (1280 Editorial, spec): the tier select changed to "Clean-room reimplementation"; the row's "Released as" select opens and lists its options (the seeded world has no Activity that declares a tier, so only "No Activity" exists: choosing a different value was not possible here, the unit test does choose one); an entry was added through the form (needs a summary) and the row list went from 1 to 2, then the spec removed it.

## Tests
- vitest `arcade/__tests__/arcadeAdmin.test.tsx` (single file): 4 passed. One case changed because it queried the native select (`selectOptions`): "releases as the chosen Activity" now clicks the kit combobox and the option. One new case: each select and the heading found by label/role. Base not run for vitest.
- prettier, eslint (`pnpm --filter @sideroom/web exec eslint`), web `tsc`, e2e `tsc`, `check-css-tokens`, `i18n-audit --max 0`: clean. `check-doc-links`: see commit time.
- Spec `e2e/tests/p90-arcade-shots.spec.ts`, `--project=chromium --reporter=line`: passed, base and after. Command (cwd `e2e/`, stack on 8684/5684/9384, world seeded; `SR_P90_ARCADE_BASE=1` for the base case):
  `SR_P90_ARCADE_ON=1 SR_P90_ARCADE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-arcade SR_E2E_BASE_URL=http://localhost:5684 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db url> DATABASE_URL=<db url> npx playwright test tests/p90-arcade-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
- The sweep seed printed `FAIL suspended app` (2FA); world used anyway.

## Shots: `visual-sweep-shots/p90-arcade`, 5, all opened
base-arcade-375-lounge-light, base-arcade-1280-lounge-editorial, arcade-375-lounge-light, arcade-375-lounge-dark, arcade-1280-lounge-editorial (`data-theme` read back in each file name). The base shots show the 24px heading under a rule and native selects; the after shots show the 20px header and kit selects. Not in a shot: the Pricing reference (its figures are in `measure.json`; its shots were deleted).

## Inventory / changelog / owner
- SEC-AUD-388 row: "Admin > Arcade: the heading is Admin's section header (20px, same place as Pricing's) and its two selects are kit Selects with labels, 44 px on a phone and passing the hit test (base: 24px h2, native selects failing the hit test at 375). Handoff `fix-p90-arcade.md`."
- SEC-AUD-363 loses the clause "the Arcade page heading and its bare controls".
- Changelog: "Admin's Arcade page now has the same heading as the other Admin pages and its two pickers match the rest of Admin and are easy to tap on a phone."
- Owner questions: none. The Arcade page's two `h3` sections keep the Arcade styling (not asked).
