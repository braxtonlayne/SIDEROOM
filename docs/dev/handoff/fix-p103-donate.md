# Wave 103, donate: the five Admin > Donations selects (SEC-AUD-465 item 8, D-972)

## Result in one paragraph
**Two of the five selects moved; three stay raw, and `donations.css` is not changed.** The two dialog selects (the resolve dialog's Resolution, the grant dialog's Kind) are now `<NativeSelect>` (fill, no `fit`). The three toolbar selects (Invoices State and Purpose, Entitlements Kind) sit in labels that two layout rules lay out by `label:has(> select)`, which stops matching when the kit's wrapper span sits between label and select. The task says to keep such a select raw rather than edit a rule outside the row, so they stay raw and the rule at `donations.css:159` still serves them. `fit="content"` was not needed, so I did not wait for kit. The Invoices table and its shade did not move (figures below).

## Times (2026-11-18, local)
Start 11:55. Code, tests and typecheck 12:00 to 12:03; spec 12:00 to 12:02. Queue for a slot from 12:05; slot taken 13:12 (other builders took each slot that came free; `heavy.sh` polls every 15 s and a gap between two of my `acquire` calls lost slots; a looping `until acquire` won it). STACK-ROUND 13:12 to 13:25 (13 min: three over the 10, kept to run the fresh-database re-run of shade that the lesson asks for). Released 13:25, `status` read `free`.

## REACH table (read; brought up to date after the round)
| # | file:line (base) | label and layout | styled by | moved? | found by |
|---|---|---|---|---|---|
| 1 | InvoicesSection.tsx:253 State filter | `<label>State <select>` in `form.ss-toolbar.inv-toolbar`; a column (label above, select filling) by donations.css:149 | admin.css:605 (`.ss-toolbar select:not([class])`: 40 px, tokens); donations.css:159 (type) | **no, raw** (layout rules donations.css:149 and :194, admin.css:599 match `label:has(> select)`) | no class or id anywhere: p90/p92-invoices take `form.locator('select').first()`; p98 and p100 read selects only by tag in generic probes; no unit test drives it |
| 2 | InvoicesSection.tsx:264 Purpose filter | same | same | **no, raw** | same |
| 3 | InvoicesSection.tsx:198 resolve dialog, Resolution | `<label>Resolution <select>` child of the dialog's `form.adm-form` (flex column); no rule on the label | none: the browser's default select (the `.ss-toolbar` rules do not reach a dialog) | **yes**, `NativeSelect`, fill | by tag; `donations.test.tsx` resolves with the default value (`mark_paid`) and does not touch the select |
| 4 | EntitlementsSection.tsx:70 Kind filter | `<label>Kind <select>` in `div.ss-toolbar`; inline-flex, centred, 8 px gap, 0.875rem by admin.css:599 | admin.css:605 | **no, raw** (rule 599) | by tag only |
| 5 | EntitlementsSection.tsx:181 grant dialog, Kind | `<label>Kind <select>` in the dialog form; no rule on the label | none: browser default | **yes**, `NativeSelect`, fill | p87 reads the dialog's `label, select` by tag; `donations.test.tsx` opens the dialog and fills the other fields |

Who imports the two files: `apps/web/src/admin/AdminApp.tsx:38-39` only; `donations.css` only the donations sections. Lazy Admin chunk; not first paint, not a public page; no web build. Tests that render the sections: `donations.test.tsx` (12), `admin.test.tsx` (25), `p10.test.tsx` (23), run unedited after the last edit: all pass.

## The diff in words
`InvoicesSection.tsx`: import `NativeSelect` (the import list wraps to one name per line, prettier); the resolve dialog's `<select>` becomes `<NativeSelect>`. `EntitlementsSection.tsx`: import `NativeSelect`; the grant dialog's `<select>` becomes `<NativeSelect>`. Same value, onChange, children; no class, no string. `donations.css`: untouched. `admin.css`: untouched (not mine).

## Figures (`visual-sweep-shots/p103-donate/measure-base.json` and `measure.json`, compared by a script field by field)
Base and after are the same world and the same four invoices (my spec seeds two on its first run and reads `seed.json` afterwards; one world allows four registering specs). Settings: Lounge Light 1024, 900, 1280, 375 touch; Lounge Dark 1024.
- **The three toolbar selects, both toolbars (every item box), the row breaks and the Invoices table (box; wrapper `scrollWidth`/`clientWidth`: 585/585 at 1024, 496/582 at 900, 828/828 at 1280, 343/343 at 375): identical to the base in all five settings** (the script prints "identical" for `invoices` and `entitlements` in every row). State 270 x 40, Purpose 145 x 40, Entitlements Kind 123 x 40 (44 at 375), text 14 px, `text-transform: none`. Invoices row tops at 1280: 136 and 232 in both (two rows); at 1024: three rows (136, 204, 300) in both.
- **Resolve dialog, Resolution select** (1024): base 103 x 19 beside its label (browser default, 13.33 px text); after 360 x 40 (the field's width) under its label, 16 px text (the dialog's own size). 375: base 103 x 19; after 303 x 44. The dialog is 20 px taller (the label sits above the select). Text clear of the chevron: yes.
- **Grant dialog, Kind select** (1024): base 193 x 19; after 360 x 40; 375: base 193 x 19, after 303 x 44. Dialog 20 px taller. The fields below moved down by that and by nothing else.
- No page scrolls sideways in any setting (`doc 0`). No label newly wraps.
- Computed `text-transform` / `font-size`: toolbar selects none / 14 px (base and after); dialog selects none / 16 px after (base 13.33 px). The task's "14 px" is the toolbar's; the dialog's select now reads like the dialog's other fields, which are 16 px.
- **Hit test at 375 (`openCollapsed`, then `hitTestAll`)**: Invoices toolbar 5 rows, pass 5, fail 0 (base the same); Entitlements toolbar 2 rows, pass 2 (same); **resolve dialog: base 6 rows with 1 fail (the raw Resolution select, taker the `div` of its own label stack above it and the next label below), after 6 rows, pass 5, fail 0**; **grant dialog: base 8 rows with 2 fails (Close and the raw Kind select), after 8 rows with 1 fail: Close** (a kit Modal control: centre `span.x`, above `button.Close`, below the dialog's own `div`; also in the base: "kit", not mine). Nothing opened by `openCollapsed`, no not-reached.
- **Keyboard**: Invoices State filter, ArrowDown: value `open`, the table stays at four rows, all OPEN (every invoice of the world is open), set back to Any state. Resolve dialog Resolution, ArrowDown: `refund_noted` (set back, nothing saved). Entitlements Kind filter: `patron` (set back). Grant Kind, ArrowDown: `server_pool` and the field's label changes from "Account id" to "Server id" (set back, nothing saved).

## Neighbours, same world, after phase (last lines)
- `p100-tables-shots` (`PHASE=after`): `REPORT problems []`, `1 passed (2.6m)` (no base files, so "boxes not compared").
- `p98-cue-shots`: `REPORT problems []`, `1 passed (30.3s)`; light `start Lfalse Rtrue; mid Ltrue Rtrue; end Ltrue Rfalse`, `FIT 1200: client 748 scroll 748; shade Lfalse Rfalse`, dark the same.
- `p102-shade-shots`: **first run in the shared world: 4 problem lines, all "darkest column" figures at the start edge** (`light 1024 f0 start: darkest column 29.8 vs base 27.5`, `f0.25` the same; dark `23.7 vs 21.6` twice), which the spec fails above a 2-level difference; no "line" problem. This is the lesson's case: the world held 11 invoices by then (my four, shade's two, tables' five) and the strip figure is a mean over rows. **Re-run alone on a fresh database (seeded again): `REPORT problems []`, `1 passed (55.4s)`.** I touch neither the table nor the shade block (the tree's diff is two `<select>` in two `.tsx` files and a new spec). A world effect, not a fault of the page. Output of both: `p103-donate/shade` and `p103-donate/shade-fresh`.
- Registrations in the world: my spec (2 accounts), tables, cue, shade: four. No 429.

## STACK-ROUND
Start 13:12, end 13:25. `heavy.sh status` at start: `donate 13:12` (single slot); after `release`: `released`, then `free`. Stack on 8806 / 5806 / 9506 with `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and `SR_PAYMENTS_RATE_SOURCES` set to the public Coinbase URL named in the header of `p98-cue-shots.spec.ts`. Seed: no FAIL line (the tail shows `ok suspended app`). Order: base (the two `.tsx` files put back to the base's text with `git show 49ca8946:<path>`, my copies kept in the scratch folder, never a stash; run twice because I changed a shot name in between, which is also why the world holds four of my invoices), my files restored, after, tables, cue, shade; stack stopped; a fresh database for the shade re-run; stack stopped by PID tree again, ports free (TIME_WAIT sockets only), database `sr_p103_donate_ui` dropped.

## TYPECHECK
`bash cli-agents/tc.sh run donate bash -c "cd .../p103-donate/apps/web && npx tsc --noEmit -p ."` at 12:03, after my last `.tsx` edit: exit 0. e2e `npx tsc --noEmit -p tsconfig.json` at 13:25, after the last spec edit: exit 0. Also at 13:25 after the last edit: `prettier --check` on the two `.tsx` and the spec exit 0; `eslint` on the two `.tsx` exit 0; `node scripts/check-css-tokens.mjs` exit 0; `node scripts/i18n-audit.mjs --max 0` `TOTAL 0`; `node scripts/check-doc-links.mjs` exit 0 (run before the commit).

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-donate`; 22 files, all opened)
After: `invoices-1024-lounge-light`, `invoices-1024-lounge-dark`, `invoices-1280-lounge-light`, `invoices-375-lounge-light`, `resolve-dialog-1024`, `resolve-dialog-375`, `entitlements-1024-lounge-light`, `entitlements-375-lounge-light`, `grant-dialog-1024`, `grant-dialog-375` (10). Base: each with `base-` in front, plus `base-invoices-1024.png` and `base-entitlements-1024.png`, the two the task names (12). `data-theme` read back before each shot (`lounge-light` / `lounge-dark`).
What I saw: the toolbar shots (Invoices at 1024 light and dark, 1280, 375; Entitlements at 1024 and 375) equal the base's: the raw selects wear the sheet's field look (40 px, token colours, native chevron), the Invoices filters have their small-caps labels above them, 1280 keeps two rows, 375 stacks full width, dark and light are right. Resolve dialog: base is a 19 px browser-default select squeezed beside the word "Resolution", black on white; after is the label above a full-width 40 px field in the kit look with the kit chevron, like the text areas below it; at 375 the same at 44 px. Grant dialog: the same; the "Account id" field and the rest move down by the label-above height and nothing overlaps; at 375 no sideways scroll. In both dialogs the word "Resolution" / "Kind" is plain 16 px text while the labels below ("Note", "Days") are small caps; that was so in the base and I did not change it (a label look, not in my row).

## STALE
None. No spec of an earlier wave named a raw select's old box.

## The commander's command
Stack env (from the worktree root, never inside `apps/web/`): `SR_DATABASE__URL=postgres://postgres:<pw>@127.0.0.1:5432/<db>`, `SR_DATABASE__AUTO_MIGRATE=true`, `SR_SERVER__BIND=127.0.0.1:<api>`, `SR_SERVER__PUBLIC_URL=http://localhost:<vite>`, `SR_STORAGE__ENDPOINT=memory:`, `SR_ACTIVITIES__ENABLED=true`, `SR_SERVER__METRICS_BIND=127.0.0.1:<metrics>`, `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`, `SR_PAYMENTS_RATE_SOURCES` (the one-entry JSON with the Coinbase spot price URL from the header of `e2e/tests/p98-cue-shots.spec.ts`; a public URL, not a secret). After the sweep seed on the same database, from `e2e/` (my spec seeds its own two invoices and reads them back from `seed.json` in the out folder; run it first in the world):

`SR_P103_DONATE_ON=1 SR_E2E_BASE_URL=http://localhost:<vite> SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P103_DONATE_OUT=<folder> npx playwright test tests/p103-donate-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

Add `SR_P103_DONATE_BASE=1` on a tree with the base's two `.tsx` files to get the `base-` shots and `measure-base.json`. Expect `problems []`. Then `p100-tables-shots` (`SR_P100_TABLES_PHASE=after`), `p98-cue-shots`, `p102-shade-shots`. In a world that has seen many invoices shade's "darkest column" lines can exceed 2 by a fraction: re-run it alone on a fresh database before calling it a fault.

## Text for SEC-AUD-465 item 8
"Admin > Donations: the two dialog selects (the resolve dialog's Resolution, the grant dialog's Kind) are the kit's `NativeSelect`: a 40 px field (44 px on a phone) under its label instead of a 19 px browser-default select beside it; the resolve dialog's select no longer fails the touch test. The three toolbar selects (Invoices State and Purpose, Entitlements Kind) stay raw: their labels are laid out by `label:has(> select)` in `admin.css:599` and `donations.css:149`/`:194`, which the kit's wrapper span breaks. Invoices table, toolbar boxes and row breaks are equal to the base at 1024, 900, 1280 and 375."

## Proposed rows
- **New (SEC-AUD-470 or the next number): the last three raw Admin donations selects.** Move State, Purpose and Entitlements Kind to `NativeSelect` together with the layout rules that name them: `admin.css:599` and `donations.css:149`/`:194` match `label:has(> select)` and need `label:has(select)` (or a class on the label given in the `.tsx`); `donations.css:159` becomes `.inv-toolbar .inv-pick` as the task described; the Entitlements one wants `fit="content"` (an inline label), the Invoices ones fill. One builder owning `admin.css:599`, `donations.css:143-200` and the two files.
- `admin.css:605` and `:739` (`select:not([class])` under `.ss-toolbar`): **still needed**. `git grep -n "<select" -- apps/web/src/admin apps/web/src/settings` on my tree shows `InvoicesSection.tsx:253` and `:264`, `EntitlementsSection.tsx:70` (mine, raw) and `settings/server/bridge/ConflictLog.tsx:109` (pages' file; not inside a `.ss-toolbar`). So the two rules serve exactly my three until the row above is done; then they can go. (My tree does not have trans's work; if trans moves other selects nothing here changes.)
- The grant dialog's Close button fails the 44 px hit test at 375, in the base and after (kit's Modal; SEC-AUD-467's class): no new row from me.

## Changelog sentence
"Admin > Donations: the Resolve dialog's Resolution and the Grant dialog's Kind are now the kit's select field (full width, 40 px, 44 px on a phone) instead of a small browser-default select."

## Owner questions
None. (A choice I made: the three toolbar selects stay raw rather than editing label rules outside my row, as the task directs; the owner may want them moved with the rules in a later wave.)

## Not done
The three toolbar selects (above). That is the finding, not an omission.

## Housekeeping
`p103-donate-bin` deleted, database `sr_p103_donate_ui` dropped, ports 8806 / 5806 / 9506 free, `heavy.sh status`: `free`, no hold in my name. `git stash list` not touched. Scratch files only in the scratchpad.
