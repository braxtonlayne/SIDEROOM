# Wave 86, guild: SEC-AUD-374 (D-880), no fault found, nothing built

**T = 16:10 (2026-11-16).** Four clock times: no new item 16:50, checkpoint 17:05, finished 17:15, hard limit 17:25. I started at 15:56 by the machine clock, took the lock at 16:03 and released it at 16:05 (the whole round: stack, seed, two spec runs).

## Done
- `e2e/tests/p86-guild-shots.spec.ts` (new, committed): the confirm-first check, kept. Members at 375 (Lounge Light, Lounge Dark, touch) and 1280 (Editorial), every table checkbox with its box, parent, hit test (cy -/+ 21), the row height and the row's other controls with their own hit tests; after only: a tap 15 px off the first box's centre (below, beside, above) selects that row only; the Admin Accounts page's cell boxes at 375 Dark and 1280 as the "0 px moved" signature.
- **No CSS change.** `server-settings.css` is not touched: the fault the task names is not there.

## What I saw on the base (ran it, `SR_P86_GUILD_BASE=1`, base fa213ab3)
Wave 85 had already fixed it: `server-settings.css:716-726` (phone block, wave 85's SEC-AUD-360 follow-up) makes each `.ss-table-stack input[type='checkbox']` a 20 x 20 box with `margin: 12px` and an `::after` with `inset: -12px`. In Chromium the `::after` is hit-tested as the input, so it works.
- 375, Lounge Light and Lounge Dark, Members, 2 rows: both row checkboxes are 20 x 20 (the parent is the `td`), the hit test passes at cy-21 and cy+21 (`hitTop`/`hitBottom` true, centre owns itself). The other controls of each row pass too: the name link 54.3 x 44 and 105.7 x 44, "Roles" 60 x 44, the row menu 44 x 44. No target covers a neighbour's centre (every centre returns its own control).
- A tap 15 px off the first box's centre (below, beside, above) at 375 selects that row only, in both themes (`only0` true x 3 each; the second row stays unchecked). Row heights 202 and 180 px (stacked rows).
- The only failing checkbox is the header's "Select all members on this page" at 375: it fails the hit test (false/false) because the stacked layout hides `thead` (1 px, clipped). It is not drawn and cannot be tapped, so it is not a touch target; it is a phone with no select-all (a design gap, not a hit-area fault; listed below).
- 1280 (Editorial): row boxes are 13 x 13 and fail the 44 px test, which is a touch rule and not asked on a desktop; unchanged by me.
- Admin Accounts (a table page that loads this sheet): `adm-table adm-table-stack`, 35 cell boxes recorded at 375 Dark and 1280; I changed nothing, so they are the same before and after by construction (the `base-` and `nochange-` runs recorded the same signature).
- ChannelTabs.tsx:568 (forum tag "Moderated"): not in a table (an `li.ss-row`, the checkbox inside a `<label>`, so the label is the tap target); I did not measure it in a browser (the world has no forum channel and no time to make one). Listed, not changed.

Why nothing was built: the task says that if the fault is not there, build nothing and write down the figures. A different rule (a hit area on the cell or label) would only duplicate what wave 85's pseudo-element already does, and the label route needs a `.tsx` change.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-guild`, 12 PNGs, **all 12 opened and looked at**)
`base-` (5: Members 375 Light and Dark, Members 1280, Admin Accounts 375 Dark and 1280) and `nochange-` (7: the same five plus Members 375 with one row selected, Light and Dark); `measure.json` has every figure. The selected row's checkbox shows the accent fill and tick; the bulk bar (Add role, Remove role, Kick, Ban, Clear selection) appears; no sideways scroll (`doc` 0). The Lounge Light and Dark pages read correctly. There is no separate "after" set: nothing changed.

## Tests run
- e2e `tsc` clean; prettier clean on the spec; eslint not run (the e2e package has none installed); the spec ran 2 x (base, then with the click-through) with `--project=chromium --reporter=line`, both passed. Not run (no `.tsx`/CSS/i18n change): web typecheck, css-tokens, i18n audit.
- Stack: sr-server and Vite stopped by PID tree, ports 8642/5642/9342 free, `sr_p86_guild_ui` dropped, `p86-guild-bin` deleted (`p80-base-bin` untouched). No stash touched.

## The commander's command for the spec (from `e2e/`, on a stack with the seeded world, API 8642, Vite 5642)
```
SR_E2E_BASE_URL=http://localhost:5642 SR_P86_GUILD=1 SR_P86_GUILD_TAG=after SR_P86_GUILD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-guild npx playwright test tests/p86-guild-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
(`SR_P86_GUILD_BASE=1` for the base case.)

## Proposed inventory row (SEC-AUD-374)
Close as "no fault on the base": wave 85's pseudo-element hit area (`server-settings.css:716-726`) already gives each Members row checkbox a 44 px target in Chromium (hit test passes at 375 in Lounge Light and Dark; a tap 15 px off the centre selects that row only). Remaining: the header's select-all is not available on a phone (hidden with the head).

## Changelog sentence
None (nothing shipped for this item).

## Owner questions
1. Wave 85's rule rests on `::after` on a native checkbox, which Chromium hit-tests but Firefox and Safari may not draw or hit-test (neither is installed here, so unverified). Should the row checkbox become the kit's `Checkbox` (a label-wrapped, `appearance: none` control that works everywhere), a `Members.tsx` change for a later wave? Default: yes, later.
2. Should a phone get a visible "Select all" for Members (the header's is hidden with the thead)? Default: no, until a bulk action asks for it.
3. D-880's wording ("has a 44 px touch target by a hit area") is already true for Members; the plan can mark it done by wave 85's rule.
