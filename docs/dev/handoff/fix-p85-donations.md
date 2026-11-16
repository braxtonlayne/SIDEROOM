# Wave 85 donations builder: SEC-AUD-372 (D-875)

T = 15:00 on 2026-11-16; no new item 16:00, checkpoint 16:15, finished 16:30, hard limit 16:45. Lock waited from 15:02, taken 15:25 (round 1) and 15:35 (round 2, shots retaken); both released, stack stopped by PID, scratch DB `sr_p85_donations_ui` dropped, `p85-donations-bin` deleted (`p80-base-bin` untouched). No stash touched.

## Done
- Commits on `fix/p85-donations`: SEC-AUD-372 (code, spec), caption on a phone, spec fixes.
- **Base run (done, `SR_P85_BASE=1`, old files restored from 774d3073 in the worktree, then checked out again):** Pricing at 375: 30 bare `<input>` (79 x 21, white ground, 2px inset border, radius 0, Arial 13px, UA focus ring), all failing the hit test; per-tier table scrollWidth 701 vs 309 box, wrapper scrolls 392 px sideways; Viewers, Audio kbps, Watch parties, Downloads/day cut at the right. Donations: Network `<select>` 70 x 19, native, fails the hit test. Page itself never scrolled sideways (doc 0). At 1280 both are native-looking but pass the hit test.
- **Kit `TextInput` or class: the class.** The kit's `TextInput`/`Field` always renders a visible `<label>` (no aria-label-only mode), so it cannot stand in a table cell. Inputs keep `aria-label`, `inputMode`, `size`, handlers and take `className="adm-perk-input"` in `donations.css`: `--ui-bg-raised` ground, `1px solid --ui-border`, `--ui-radius`, `--ui-text`, 8px 12px padding, `font: inherit`, hover `--ui-text-faint`, focus `2px solid --ui-focus` offset 1px (same tokens as `Input.module.css`), 40 px tall, 44 px under 720px.
- Selects: Pricing "Who" (it stood in `.ss-toolbar`) and Donations "Network" are the kit `Select` with a `<label htmlFor>` (class `adm-pick`/`adm-pick-label`, label look = the kit's). Same values and handlers. No new strings.
- Phone (<=720px): `adm-perk-table` and `data-label={t(p.label)}` on each cell; each tier a block (row header as title, rule between tiers), each perk a flex line (name, field 120 px wide ending at one x), head hidden to the eye. The table's `us-sr` caption was visible in Admin (`.us-sr` lives in `user-settings.css`, not loaded there) and showed as a narrow stack on a phone, so the caption is hidden to the eye on a phone too.

## Probe table (computed, kit `TextInput` probe vs perk input; identical in all three)
| theme | ground | border | radius | text | padding | focus ring |
|---|---|---|---|---|---|---|
| Lounge Light | 242,243,245 | 1px 201,204,209 | 8px | 49,51,56 | 12/12 | 2px 0,104,201 off 1 |
| Lounge Dark | 43,45,49 | 1px 63,65,71 | 8px | 219,222,225 | 12/12 | 2px 0,168,252 off 1 |
| Editorial | 28,27,23 | 1px 58,56,47 | 4px | 239,230,210 | 12/12 | 2px 217,201,164 off 1 |
Only difference: font size 14px (the table's) vs the kit's 16px.

## Counts, hit test
- Bare text/number/select outside a toolbar: Pricing 30 -> 0 (30 now `adm-perk-input`, probed equal), Donations 1 -> 0. Left, listed not changed: checkbox "Donations are on" (Pricing, 13 px, fails the hit test at 375: SEC-AUD-363's list), plus a skip link not mine (42 px).
- Hit test at 375 (Light, Dark): every perk field (44), both selects, all kit controls pass; kit failures: none. Sideways scroll 0 (page and table; table 309 = its box) at 375. Lists open inside the viewport at 375: Who x34 y571 119x72, Network x34 y459 307x144.
- Click-through on the live stack (phone Light): change "Level 2 Upload MB": Save and Reset enable; Reset: value back (100); change again, Save: reason dialog opens; closed without saving. Accessible name unchanged: `getByRole('textbox', { name: 'Level 2 Upload MB' })` finds exactly one.
- What moved at 1280: still a table, same header cells; the fields are 40 px tall (was 21) so rows are taller, the inputs fill their column (min 5rem; was `size=6`), "Video senders" and "Watch parties" headers wrap. The "Network" and "Who" labels are now the small uppercase kit label above the field (was inline text). Pre-existing, left: the `caption` ("Perks per tier", "Suggested against configured prices") shows on desktop because `.us-sr` is not loaded in Admin; proposal for the admin builder / commander: define the sr-only class in `admin.css`.
- Font note: the fields' 14px vs kit 16px; iOS zoom-on-focus below 16px is possible; owner question below.

## Who loads what
`PricingSection.tsx`, `OverviewSection.tsx` imported only by `admin/AdminApp.tsx` (lazy Admin chunk); `donations.css` imported only by the four `admin/donations/*Section.tsx`. No first-paint or public page loads them; no build run.

## Tests
`admin/__tests__/donations.test.tsx`: single file, before and after: 12/12 pass both. One test changed: the wallet test used `selectOptions(getByLabelText('Network'), 'signet')`; now clicks `getByRole('combobox', { name: 'Network' })` and the option `signet`. No assertion about what is sent weakened. Tests that found perk fields by `getByLabelText('Level 2 Upload MB')` still pass unchanged (aria-label kept). Did not watch the changed test fail on the base (the old selector cannot find a kit Select by design). Typecheck, eslint, prettier, css-tokens, i18n-audit (0), doc-links, e2e tsc: clean.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-donations`, 25 PNG (9 before, 16 after) + `measure-before.json`, `measure.json`. Opened: all before shots (round 1) and, from the final run, donations-wallet x3 and before-donations-wallet x3, network-open, pricing-table (Light, Dark, Editorial), pricing-calc (Light, Dark), click-dialog; from round 1 (same code path, caption rule since added) pricing-top x3, pricing-calc Editorial, who-open, click-changed. Not re-opened from the final run: pricing-top x3, pricing-calc-desk, who-open, click-changed, before-pricing x6 (round 1 versions were opened).

## Proposed inventory row
SEC-AUD-372: Admin Pricing's 30 per-tier inputs and the Network and Who pickers were native-looking (79 x 21, failed hit test, table 392 px wider than a phone). Fixed: `adm-perk-input` on the kit field's tokens, 44 px on a phone, phone cards per tier, kit `Select` for both pickers. Left of SEC-AUD-363 on these pages: the "Donations are on" checkbox (13 px). Changelog: "Admin's Pricing and perks fields and the Donations network picker now look like the rest of the app and fit a phone, with each tier shown as its own block."

## Owner questions
1. Perk fields at 14px (table size) or the kit's 16px (avoids iOS focus zoom)? Default kept 14px.
2. Should the visible table captions on desktop (see above) be hidden? Not mine.
