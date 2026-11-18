# Wave 103, donate: the five Admin > Donations selects (SEC-AUD-465 item 8, D-972)

STATUS 12:30 (checkpoint 1): REACH table and the two dialog moves committed and typechecked; the spec compiles; waiting for a heavy slot for the base-and-after STACK-ROUND. Everything below marked "to measure" is filled in after the round.

## The finding that decides the item (read, not yet measured)
Only the two **dialog** selects can move. The three toolbar selects sit in labels that two layout rules lay out by `label:has(> select)` (a select that is a *direct child* of its label). The kit's `NativeSelect` puts the `<select>` inside a wrapper `<span>`, so `:has(> select)` stops matching and the label loses its layout:
- `admin.css:599` `.ss-root .ss-toolbar > label:has(> select)` (inline-flex, centred, gap 8 px, 0.875rem): the Entitlements Kind filter's whole layout;
- `donations.css:149` `.ss-root .ss-toolbar.inv-toolbar > label:has(> select)` (column, 6 px gap, small-caps look) and `:194` (phone: `flex: 1 1 100%`): the Invoices State and Purpose filters' layout, and the rule at `:159` the task would replace gives their select normal type.
Neither of those blocks is in my row (admin.css is not mine; donations.css is mine only at `:159-165`, "never anything else"). The task says: a layout rule that stops matching means keep that label's select raw rather than edit a rule that is not in the row. So the three toolbar selects stay raw, and `donations.css` is **not changed** (the `:159` rule still serves them). `fit="content"` was therefore never needed and I did not wait for kit's READY.

## REACH table (done by reading; who finds each one: nobody by class, see last column)
| # | file:line (base) | label and layout | styled now by | moved? | found by |
|---|---|---|---|---|---|
| 1 | InvoicesSection.tsx:253 State filter | `<label>State <select>` in `form.ss-toolbar.inv-toolbar`; a column (label above, select filling) by donations.css:149 | admin.css:605 (`select:not([class])`: 40 px, tokens), donations.css:159 (type) | **no, left raw** (layout rules 149/194 and 599 match `label:has(> select)`) | e2e `select` by tag in p90/p92-invoices (`form.locator('select').first()`) and `selectOption`; no unit test |
| 2 | InvoicesSection.tsx:264 Purpose filter | same | same | **no, left raw** | same |
| 3 | InvoicesSection.tsx:198 resolve dialog, Resolution | `<label>Resolution <select>` child of `form.adm-form` (a flex column), no rule on the label | none by class; the browser default select (`admin.css` is `.ss-toolbar` only, and the dialog is not in one) | **yes** `NativeSelect` (fill) | by tag inside `[role=dialog]` in p92/p93; `donations.test.tsx` and `admin.test.tsx` do not reach it |
| 4 | EntitlementsSection.tsx:70 Kind filter | `<label>Kind <select>` in `div.ss-toolbar`; inline-flex by admin.css:599 | admin.css:605 | **no, left raw** (rule 599) | e2e `.ss-toolbar select` in p92; none in unit tests |
| 5 | EntitlementsSection.tsx:181 grant dialog, Kind | `<label>Kind <select>` in the dialog's form, no rule | none (browser default) | **yes** `NativeSelect` (fill) | p87 (`label`, `select` by tag inside the dialog); `donations.test.tsx` opens the dialog and picks by role |
Who imports the files: `AdminApp.tsx:38-39` only (`git grep -n`), the lazy Admin chunk; `donations.css` is imported by the donations sections only. Not first paint, not a public page: no web build.

## Done (commits on `fix/p103-donate`)
1. `SEC-AUD-465 InvoicesSection, EntitlementsSection: the two dialog selects ... become NativeSelect`: two `<select>` to `<NativeSelect>` (same value, onChange, children; imports added; nothing else).
2. `SEC-AUD-465 p103-donate-shots spec`.
