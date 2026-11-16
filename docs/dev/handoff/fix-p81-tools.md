# Wave 81 handoff: tools (SEC-AUD-333 / D-837, SEC-AUD-334 / D-840)

Branch `fix/p81-tools` (base `12d480b8`). Web only, CSS plus one class name; no Rust, no cargo, no migration, no new string.

**Clock.** T = 09:35 on 2026-11-16. Four times: no new item from 11:35, checkpoint 11:55, finished 12:20, hard limit 12:35. Round 1 on the stack 09:41 to 09:52 (four Playwright runs); round 2 09:55 to 09:58 (the whole spec once more, 2 passed, 2.0 min, to put the checkbox in view in the phone shots and to write a complete measurements file). Finished 09:5x, about T+25 min.

## Done

Commits: `318ea0e7` (checkbox rule, naming table, spec), `07e629eb` (label row rule for a checkbox in a `.ud-field`, spec), and the handoff commit.

### SEC-AUD-333 (D-837): a checkbox or radio in a tool is checkbox-sized and beside its label

- **Confirmed on the base first.** The new rule deleted from the live style sheet (`holdBack` in the spec, by selector; the page then wears the base CSS), measured with `e2e/tests/p81-tools-shots.spec.ts`. Base: Base64 "URL-safe alphabet" 143.5 x 32 px, 95 % of its row, stacked above its label (label text 35.5 px below the box centre); Markdown table "Line the columns up" 154.4 x 32, 96 %, stacked. JSON "Sort keys" (a column `.ud-field` label) 80 x 32 with the box below its caption.
- **Built** in `apps/web/src/tools/utl-dev.css`, two rules after the field rule (my choice: a separate rule, not `:not()` guards on the field rule, so the field rule is untouched):
  1. `.ud-root input[type='checkbox'], .ud-root input[type='radio'], .ud-timer ...: inline-size: auto; min-block-size: 0; padding: 0; border: 0;` (the contract's rule).
  2. The label rule the task allows: `label:has(> input[type='checkbox'])` inside `.ud-root` / `.ud-timer` is `flex: 0 0 auto; flex-direction: row; align-items: center; gap: 8px`, with its checkbox `order: -1`. It was needed for ONE tool: JSON's "Sort keys" is a column `.ud-field` (caption above the box, and `.ud-field-narrow` is 80 px wide), so with rule 1 alone the box was 13 x 13 but stretched to 73 px wide (column flex, `align-items: stretch`) and sat below its caption (centre 21 px below the text centre, wrong side). Base64 and the Markdown table are `.ud-row` labels (a row already) and needed rule 1 only. A first version of rule 2 had `flex-wrap: wrap` and kept the 80 px basis: the caption wrapped below the box in the 80 px label; it now sizes to its content.
- **Measured after** (box, then the vertical centre of the label's first text line against the box centre, and the box on the label's inline-start side; Lounge Light desktop, Lounge Dark desktop, both phones for Base64 and Markdown table): every checkbox and radio of the 17 tools, and of the drawer's other tools that hold one:

| tool | checkbox | base | after | dy | start side |
|---|---|---|---|---|---|
| Base64 | URL-safe alphabet | 143.5 x 32 (stacked) | 13 x 13 | 0 | yes |
| Markdown table | Line the columns up | 154.4 x 32 (stacked) | 13 x 13 | 0 | yes |
| JSON | Sort keys | 80 x 32 (below the caption) | 13 x 13 | 0 | yes |
| Programmer calculator | Signed | 13 x 13 | 13 x 13 | 0 | yes |
| Unit converter | Show every unit | 13 x 13 | 13 x 13 | 0 | yes |
| Time zones | Skip weekends | 13 x 13 | 13 x 13 | 0 | yes |

  The other 14 tools of wave 79's list have no checkbox or radio (the spec logged none); the Programmer, Units and Time zones checkboxes are not under `.ud-root` and were never affected (rule held back: 0 rules; same box). The Poll builder and the Audio tool were tried by query and logged no checkbox (the drawer did not show them), so they are not covered; they use `sr-room-check` and `utl-aud-check` classes of their own, outside `.ud-root`. Base64 and the Markdown table are the same 13 x 13 with dy 0 in all four variants (light/dark, desktop/phone). The spec asserts at most 20 px, |dy| at most 4, box first, not stacked: green.
- **Wave 80's height log re-run, equal.** For every tool the heights of each `.ud-field` and its control with my rule in and with it held back: equal for all 21 tools tried (`field heights base == after: ... true`). Values: Regex Pattern label 51 / control 32, every textarea field 139 / 120, selects 51 / 32, Loot "Name" 51 / 32: the same as wave 80's "after" column.
- **Click-through.** Base64: ticked "URL-safe alphabet", the output changed (`+` and `/` become `-` and `_`); Markdown table: unticked "Line the columns up", the output changed (padding gone); JSON: ticked "Sort keys", the output changed. Each ticked and unticked again.

### SEC-AUD-334 (D-840): the naming table reads in phrases on a phone

- **Who loads the files (checked before the lock).** `review/review.css` is imported by `review/NamingGuide.tsx`, `review/shared.tsx`, `review/RequestsBoard.tsx`, `review/ReviewExtras.tsx` and `admin/sections/IngestSources.tsx`. `NamingGuide` is used by `cinema/submit/SubmitPage.tsx` (route `SubmitPage = lazyNamed(...)` in `cinema/routes.tsx:16`), `library/Submissions.tsx` (lazy in `library/routes.tsx:18`, 19) and `admin/sections/IngestSources.tsx` (the Admin chunk: `AdminApp` is imported only by `admin/StaffDoor.tsx` by dynamic `import()`). Nothing first paint or a public page loads: **no web build, no bundle or entrance check**; the round was a short one.
- **Confirmed on the base** (the `.rv-naming` rules deleted from the live sheet, same spec; the Cinema's submit page at 375 x 812, Lounge Light for the Cinema, three names parsed by the real server: the 75-character `The.Long.Corridor...x264-GROUP.mkv`, `Heat (1995).mkv`, and `Some.Movie.Title.mkv`, which gives the warning "no year in the name"; the page needs the Study switch `platform.ingest_queues` on, which the spec sets and resets). Base at 375: name column 486 px, "Read as" column 99 px wide with 12 lines for the long name, the table 585 px in a 292 px wrapper (the "Read as" cell scrolled off to the right: the base shot shows only the Name column), document 375/375 (no sideways scroll).
- **Built.** `NamingGuide.tsx`: `className="rv-table rv-naming"` (nothing else). `review.css`: `.rv-naming td:first-child { overflow-wrap: anywhere; }` and `.rv-naming th:nth-child(2), .rv-naming td:nth-child(2) { min-inline-size: 12rem; }`. Chromium honoured `min-inline-size` on the cells: no `inline-size: 55%` fallback needed.
- **After at 375:** "Read as" 221 px (Lounge Light) and 222 px (shipped), 5 / 1 / 2 lines, name column 70 px (the name breaks anywhere), table 292 = wrapper 292, document 375/375. At least 192 px in both. Admin > ingest sources (Library guide, three same names, phone, Light): "Read as" 213 px, 3 lines each, table 297 = wrapper, document 375/375 (one shot).
- **At 1280 (Lounge Light, before and after):** columns at 94 and 654 before, 94 and 632 after; widths 560 / 546 before, 538 / 568 after: **the name column is 22 px narrower** (and "Read as" 22 px wider). The table does not scroll (1106/1106) before or after. The contract says "nothing moved" at 1280; this moved 22 px, in the table that has the 75-character name (the long name used to take its full 560 px of text width, now it wraps at 538); a table with short names only was not measured. I think it is the intended effect of "the name gives way". Shipped theme at 1280: 94 / 636, widths 542 / 564, no scroll.

## Findings only (nothing changed)

- **The JSON tool by its own name** (palette entry "JSON", picked with the option's name; the query `json` alone ranks the JWT decoder first): one shot, `json-lounge-light-desktop-after.png`. It is whole: caption "JSON to check", textarea, "Layout" select and "Sort keys" box on one row, "This is valid JSON." and the output. Its checkbox is the one that needed the label rule (above).
- The submit page and Admin's ingest sources are hidden while the Study switch `platform.ingest_queues` is off (the default); that is how a member reaches the guide.

## Not done

- Poll builder (`sr-room-check`) and Audio (`utl-aud-check`) checkboxes were not shot or measured (the drawer queries did not open them; they are outside `.ud-root` so the field rule never matched them). No fault known.
- No vitest of mine: `review/__tests__/naming.test.tsx` (renders `NamingGuide`) passes as a single file; there is no test of CSS, so the spec is the check.

## Shots (folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-tools`, `measurements.txt` beside them)

20 PNG files (plus `measurements.txt`), from the final whole-spec run (round 2, 09:55 to 09:58): Base64 and Markdown table after in Lounge Light and Dark, desktop and phone (8) and Light desktop base (2); JSON, Programmer calculator, Unit converter, Time zones, Light desktop (4); the naming table at 375 and 1280 in the Cinema's Lounge Light and shipped theme (4), the 375 Light base (1), Admin's guide at 375 (1). Each Cinema shot reads `data-theme` of `.cin-root` back (asserted) and the room's account theme is read back for the tool shots. **I opened all 20.** Honest note: I opened the images of the previous run (same code, same spec apart from scrolling the checkbox into view and not shooting tools without a checkbox) for all of them, and in the final run re-opened the two Markdown table phone shots, which are the ones the change of that run was for (the checkbox was cut off by the drawer's bottom edge before; it is whole now). The other 18 are overwritten by an identical-code rerun whose measurements (in `measurements.txt`) equal the first.

## Tests and checks

- `npx pnpm@10.12.1 --filter @sideroom/web exec tsc --noEmit -p tsconfig.json`, eslint on `NamingGuide.tsx`, prettier on all four files, `node scripts/i18n-audit.mjs --max 0` (TOTAL 0), `node scripts/check-css-tokens.mjs` (73 files clean): clean. `e2e` `tsc --noEmit`: clean. `vitest run src/review/__tests__/naming.test.tsx`: 1 passed.
- The shots spec (`SR_P81_TOOLS=1`, `--project=chromium --workers=1 --reporter=line`): I did not run the spec against a base build; the base is the rule held back on the live page (the "BASE" lines of `measurements.txt`), and there the boxes are 143 x 32, 154 x 32 and 80 x 32, stacked, which the spec's limits (20 px, 4 px, beside) would fail. Its runs: run 1 (tool part green, naming part failed on my selector: the guide's "Test" button matched "tester"), run 2 (JSON's box stretched to 73 px: `checks outside the target`), run 3 (JSON's caption wrapped under the box), run 4 (tools part, green). The naming part's last run is run 3 of the whole spec (green).

## Docs

Nothing in `docs/` states these rules (searched `rv-table`, `rv-naming`, `utl-dev`, the tools' checkboxes): no doc changed. `node scripts/check-doc-links.mjs` not needed (no doc touched).

## Proposed inventory rows (the commander adds them after SEC-AUD-326)

- **SEC-AUD-333** (closed): A checkbox or radio in a tool is checkbox-sized and beside its label. The tools' field rule (full width, 32 px, padding, border) matched every `input` under `.ud-root`, so Base64's "URL-safe alphabet" and the Markdown table's "Line the columns up" were 32 px full-width boxes above their labels and JSON's "Sort keys" a stretched box below its caption. Fixed in `tools/utl-dev.css`: the rule is for text controls; a checkbox or radio keeps its natural size, and a label that holds a checkbox is a row with the box first (D-837). Measured on the real stack in Lounge Light and Dark, desktop and phone.
- **SEC-AUD-334** (closed): The naming table reads in phrases on a phone. A file name is one unbreakable word and took the width, so "Read as" was 99 px wide and wrapped one word per line (12 lines) at 375. `NamingGuide` marks its table `rv-naming`; the name breaks anywhere and "Read as" is at least 12rem (D-840). At 375 "Read as" is 221 px; the document does not scroll sideways; Admin's copy of the guide reads the same.

## Changelog sentences

- Checkboxes in the Base64, Markdown table and JSON tools are checkbox-sized and sit beside their labels again, not 32 px boxes above them.
- On a phone the naming help's table reads in sentences: the file name wraps and "Read as" keeps a readable width.

## Owner questions

- At 1280 the naming table's name column is about 22 px narrower where a long file name used to take the width (the contract said nothing would move). Kept as the intended effect of "the name gives way"; say if you want the name column's width held (it would need a `min-inline-size` on it).

## Clean-up

Scratch database `sr_p81_tools_ui` dropped; `p81-tools-bin` deleted (`p80-base-bin` untouched); my server and Vite stopped by Windows PID (checked the ports 8592, 5592, 9292 are free); lock released. I found no stash and touched none. A command of mine that removed the shots folder's contents with a glob was refused by the safety check and not re-run another way; the spec overwrites its own shots (the tools without a checkbox were removed by name earlier).
