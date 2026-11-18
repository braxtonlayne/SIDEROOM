# Wave 105, donate: the last three Admin > Donations selects move to the kit (D-1002; SEC-AUD-481, 492, 497)

## Clock
Start 15:30. REACH table and all four code commits by 15:34 (ALL READY). `tsc` job 0017-donate-tsc.

## Done (commits, in order)
1. `D-1002 InvoicesSection.tsx (READY)`: State and Purpose are `<NativeSelect className="inv-pick">` (same value, onChange, children, no `fit`); each label has `className="inv-field"` and its text is in a `<span>` (the `{' '}` is gone).
2. `D-1002 donations.css (READY)`: `.ss-root .ss-toolbar.inv-toolbar > label:has(> select)` is now `... > label.inv-field` and **adds `display: inline-flex`** (the base got its display from `admin.css:599`, which no longer matches the label; the other declarations are the same). `... label:has(> select) > select` is now the two-class `.inv-field .inv-pick` (same declarations). The phone rule (`flex: 1 1 100%`) is `label.inv-field`. New block for 497 (below).
3. `D-1002 EntitlementsSection.tsx (READY)`: toolbar Kind is `<NativeSelect fit="content">`; its label has `className="adm-toolbar-field"`; the `{' '}` text node is removed (inside an inline-flex label, the 8 px `gap` replaces it; whitespace between a flex item and text is dropped anyway, so I expect no visible change: staffshots measures). The grant dialog's Kind label has `className="don-field"`, its text in a `<span>`, and the select `className="don-pick"`.
4. `D-1002 admin.css (ALL READY)`: `:599` is `.ss-root .ss-toolbar > label.adm-toolbar-field` (inline-flex, centre, 8 px, 0.875rem); the raw-select rule (`.ss-root .ss-toolbar select:not([class])`, was `:605-613`) and its first selector in the phone block (`:739`) are deleted; the checkbox selector stays. Comment rewritten.

SEC-AUD-497: `.don-field` (flex column, 6 px gap, 0.75rem / 600 / 0.04em / uppercase / `--ui-text-muted`: the values of the kit's `Input.module.css .label`) and `.don-field .don-pick` (0.875rem, no capitals, no tracking, 400, `--ui-text`) are in `donations.css` inside my range (after `.inv-pick`). Nothing outside my files was needed.

## Not done
Nothing left out. No spec, shots or build job (staffshots). The grant dialog's select is 14 px as the task says; the other dialog fields are 16 px (wave 103 saw the Resolution/Kind selects at 16 px with the kit default): this is a real, intended change of the Kind select's type size (and the label above grows by `gap` only if the label's own lines differ). If staffshots flags it, cure (a) is to drop `font-size` from `.don-field .don-pick`.

## REACH table
| # | file:line (base) | element / how opened | styled now by | found by in e2e / tests |
|---|---|---|---|---|
| 1 | InvoicesSection.tsx:251-261 State | `label > select` in `form.ss-toolbar.inv-toolbar`; Admin > Donations > Invoices | admin.css:599 (display, gap), :605 (box), donations.css:149 (column, caps), :159 (type) | by tag: `p90/p92-invoices` take `form.locator('select').first()`; no class; no unit test drives it |
| 2 | InvoicesSection.tsx:262-272 Purpose | same | same | same |
| 3 | EntitlementsSection.tsx:68-75 Kind filter | `label > select` in `div.ss-toolbar`; Admin > Donations > Entitlements | admin.css:599, :605 | `p103-donate-shots.spec.ts:445` `page.locator('.ss-toolbar select').first()` (earlier wave, not edited; still finds the select inside the span) |
| 4 | EntitlementsSection.tsx:179-185 grant dialog Kind | `label > NativeSelect` in the dialog's `form.adm-form`; "Grant" button | none on the label (plain text, browser look) | p87 reads `label, select` by tag; `donations.test.tsx` opens the dialog |
| 5 | admin.css:599 (matches) | `.ss-toolbar > label:has(> select)`: only rows 1-3 (`git grep -n "<select" -- apps/web/src` gives only the three, plus `StackSkin.tsx` which is not in `.ss-root`/`.ss-toolbar`). Of the 42 files that use `.ss-toolbar`, none other has a label with a raw select; the kit's selects sit in a `span`, which `:has(> select)` never matched. | | |
| 6 | admin.css:605 and :739 (`select:not([class])` under `.ss-toolbar`) | same: rows 1-3 only. Now deleted. `admin.css:823` (`.adm-table-stack td[data-actions] select`) is a different rule, left (not mine) | | |
| 7 | Conflict log toolbar (settings/server/bridge) | has `ss-field-label` rule of its own since wave 104; `label.adm-toolbar-field` is not on it, so unchanged | server-settings.css | not touched |

Importers: `AdminApp.tsx:38-39` imports both sections (lazy Admin chunk; not first paint, not a public page). `donations.css` is imported by the Donations sections only; `admin.css` by the Admin chunk.

## JOBS
- `0017-donate-tsc` (tsc): submitted 1795033922, started 1795034007 (waited 85 s), ended 1795034053 (ran 46 s), rc 0.

## Figures
Not measured: staffshots measures for the pod. Expectations to check: Invoices State 270 x 40, Purpose 145 x 40 unchanged and still in two rows at 1280 and 1024; Entitlements Kind label up to 40 high (44 at 375), width at most 36 px over the base (123); grant dialog Kind label becomes small caps.

## Shots
None (no shots builder duty).

## Checks (own tree, last commit)
- prettier `--check` on the two `.tsx`, `donations.css`, `admin.css`: exit 0 after `--write` on EntitlementsSection (15:31).
- eslint on the two `.tsx`: exit 0 (15:31).
- vitest `donations.test.tsx` 12/12, `admin.test.tsx` 25/25, `p10.test.tsx` 23/23, unedited, one file at a time (15:31-15:32).
- `node scripts/check-css-tokens.mjs` exit 0; `node scripts/i18n-audit.mjs --max 0` TOTAL 0 (15:32).
- web `tsc` job 0017: rc 0. `check-doc-links`: not run (no doc edited beyond this handoff).

## Closing texts
- SEC-AUD-481: "Closed (D-1002): Invoices State and Purpose and Entitlements Kind are the kit's `NativeSelect`; the layout rules name a class on the label (`.inv-field`, `.adm-toolbar-field`) and `.inv-field .inv-pick` types the select. Handoff `fix-p105-donate.md`."
- SEC-AUD-492: "Closed: `admin.css:599` is `label.adm-toolbar-field`; `.ss-toolbar select:not([class])` (`:605`, `:739`) is gone, no raw select is left in an Admin toolbar."
- SEC-AUD-497: "Closed: the grant dialog's Kind label has the kit label's look (`.don-field`), its select 14 px, no capitals."
- `docs/`: `git grep -n "has(> select)" -- docs` finds only handoffs and `docs/inventory/21-feature-phase.md` rows 464, 481, 492 (history/rows owned by the commander); no doc states the rule as current, so no replacement sentence is needed besides the closing texts above.
- Proposed new row: none. A note: `admin.css:823` still styles stacked-table action selects by element name under `.adm-table-stack` (not a kit-select fault today; no raw select is in any action cell).

## Changelog sentence
"Admin > Donations: the Invoices State and Purpose filters and the Entitlements Kind filter are now the kit's select field, and the grant dialog's Kind label has the same small-caps look as the fields below it."

## Owner questions
None.

## PROCESSES
strays: none found
