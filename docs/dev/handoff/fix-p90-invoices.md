# Wave 90, invoices builder: SEC-AUD-390 (D-900), the Invoices search label on the labels' line

T = 05:50 on 2026-11-17. Lock taken 06:21 (second round; the first, 06:14, found the fault and the first CSS missed), released 06:24. Finished before 06:30.

## What I saw on the base (ran it: `SR_P90_INVOICES_BASE=1`, the form's new class removed in the page)
- 1280 Editorial: State and Purpose are `<label>text <select>` pairs drawn label-beside-select (`admin.css:599`, `.ss-root .ss-toolbar > label:has(> select)`, inline-flex, centred); label text top 187.5, control top 176, height 40. The search field is the kit's: label above its input, hint below; label top 134, control top 155, height 40. Labels 53.5 px apart, controls 21 px apart. Fault confirmed.
- 375 Lounge Light: State and Purpose labels sit beside their selects (not above), the selects are 270 and 145 px wide (not full width; wave 85's `:has()` rule in `server-settings.css` only fills text fields); the search label sits above its input, full width. Controls 44 px, all pass the hit test.
- Entitlements' toolbar (reference): Kind label/select and "Only active" on one line, 40 px at 1280, 44 px at 375.

## Cause
Two structures in one toolbar: bare label-beside-select pairs (admin.css) next to a kit field with its label above, and `align-items: flex-end` bottom-aligns them, the search block being taller because of its hint.

## What I built (donations.css, one class on the form in InvoicesSection.tsx: `ss-toolbar inv-toolbar`)
Chosen over aligning to the end: the pairs get the kit's label-above structure (kit label look: 0.75rem, 600, uppercase, muted, 6 px gap) so the labels share one line and the controls another. At >= 721 px the toolbar aligns to the top and the checkbox row and Search button (no labels) are pushed down 21 px / 23 px onto the controls' line; at <= 720 px each filter takes the row (`flex: 1 1 100%`) and the selects are full width. Select text keeps 0.875rem. Logical properties and existing `var(--ui-*)` tokens only; no new string.
Known cosmetic: at 1280 the Search button wraps to a second row (the row is too narrow for five items) and keeps its 23 px offset. It was on a second row on the base too.

## Figures after (shots: `visual-sweep-shots/p90-invoices`, measure.json against measure-before.json)
- 1280 Editorial: label tops 134 / 134 / 134, control tops 155 / 155 / 155, control heights 40 / 40 / 40; checkbox row centred on the same line (155-195). Toolbar height 149 against 130 on the base (+19: the button's second row sits 23 px lower).
- 375 Lounge Light and Dark: each label directly above its own control (label text 156-171, control top 177; 229-244 / 250; 302-317 / 323), State/Purpose/Search controls 343 wide (the form's width), 44 px high, hit test passes; toolbar height 284 against 242 (+42: two labels stacked). No sideways scroll (0) in any setting.
- Entitlements' toolbar boxes: identical to the base in all three settings (0 px; the md5 of the three entitlements lines is the same before and after).
- The filter works: choosing State "open" sends a request with `state=open`; the empty state shows ("No invoices"; the world has no invoices).

## Who imports my files
`git grep`: `InvoicesSection.tsx` is imported only by `apps/web/src/admin/AdminApp.tsx` (the Admin chunk); `donations.css` only by the four `admin/donations/*Section.tsx` files. Lazy; no web build. No vitest file renders `InvoicesSection`.
`donations.css` is also loaded by Entitlements, Overview and Pricing; the new rules are all under `.inv-toolbar`, which only the Invoices form carries.

## Tests run
prettier --check (3 files), eslint on InvoicesSection.tsx, web `tsc --noEmit`, e2e `tsc`, `check-css-tokens`, `i18n-audit --max 0` (TOTAL 0), `check-doc-links`, `p90-invoices-shots.spec.ts` base (passed, nothing asserted) and after (passed, no problems; it first failed on the base-measured first CSS: that run found the hint-induced 21 px offset and the non-full-width selects, which the final CSS fixes). The seed printed `FAIL suspended app` and went on (as the brief says).

## Shots: 6 in `visual-sweep-shots/p90-invoices`, 5 required, all 5 opened by me after the release (plus `before-...-phone-dark` not opened, not required)
before: `before-invoices-toolbar-desk-lounge-editorial.png`, `before-invoices-toolbar-phone-light-lounge-light.png`; after: `invoices-toolbar-desk-lounge-editorial.png`, `invoices-toolbar-phone-light-lounge-light.png`, `invoices-toolbar-phone-dark-lounge-dark.png`.

## The commander's command (from `e2e/`, after the sweep world is seeded on the same database)
`SR_P90_INVOICES_ON=1 SR_E2E_BASE_URL=http://localhost:5686 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P90_INVOICES_OUT=<folder> npx playwright test tests/p90-invoices-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P90_INVOICES_BASE=1` for the base case).

## Proposed inventory text
SEC-AUD-390 (closed, wave 90, D-900): on Admin's Invoices page the State and Purpose filters took the kit field's label-above structure (`.inv-toolbar` in `donations.css`): labels share one line and controls share one line at 1280; at 375 each is a full-width 44 px control under its label. SEC-AUD-363 loses the clause "the Invoices search label sits above the State/Purpose line".

## Changelog sentence
Admin's Invoices filters now line up: State, Purpose and Search each have a label above the control, on one line on a desktop and stacked full width on a phone.

## Owner questions
None. Default chosen: label-above for the bare selects (matches the kit's look) rather than bottom alignment.

## Cleanup
Stack stopped by PID tree, ports 8686/5686/9386 free, `sr_p90_invoices_ui` dropped, `p90-invoices-bin` deleted (`p80-base-bin` untouched), lock released. `git stash` not used and not touched.
