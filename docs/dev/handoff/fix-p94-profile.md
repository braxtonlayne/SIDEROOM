# Wave 94, profile builder: SEC-AUD-427 (D-933) and the wave's web build

Clock: started 13:55, lock round 14:14 to 14:17 (stack, seed, spec twice), build round 14:19 to 14:20 (the build ran early because the lock was free; it ran on my tree, not on the merged tree). Branch `fix/p94-profile`, commits `cf50c062` (rule + spec), `afbccf43` (the `box-sizing` fix found in the round).

## What I saw on the base (ran it: 375 touch, Lounge Light, `/u/owner`, `hitTestAll` over `main`, 13 controls, 0 not-reached)
| control | box | fail | taker |
|---|---|---|---|
| link block Bitcoin | 160 x 42 | below | `li` |
| link block Ko-fi | 160 x 42 | below | `code.pp-tipcode` |
| link block Blog | 160 x 42 | below | `section.pp-block` |
| author link "Juniper" | 50 x 17 | above, below | `span._fallback` (avatar), `p.pp-text` |
Also failing on base and after, not mine: vouch badge 69 x 19 (taker header / `div.pp-root`), ring button 40 x 40 (taker its own `span.rng-wrap`), and the guestbook's kit buttons "Report" (below: Remove), "Approve" (below: Report), "Report" (below: Remove), each 28 px high and stacked with an 8 px gap. Those are kit buttons (`Button size="sm"`) in `profilepage/blocks.tsx`: listed, not built.

## What changed (`packages/profile-blocks/src/profile-blocks.css`, one `@media (max-width: 720px)` block at the end, comment names SEC-AUD-427 and D-933)
- `.pp-linktree a`: `display: flex; align-items: center; justify-content: center; box-sizing: border-box; min-block-size: 44px`. The first run, without `box-sizing`, drew 66 px (44 + 20 padding + 2 border); I measured that, added `border-box`, and ran again. Padding and centred text stay.
- `.pp-entries .pp-muted > a.pp-link`: `position: relative` and a centred 44 px `::after`. The link stays 50 x 17. The `::after` covers only the link's own width, so it lies over the entry's padding and the text line under it, not over the avatar (beside it) and not over any control (Report, Approve, Remove wrap under the text). The rule is not on bare `a` or `.pp-link`.
- No theme, section or `data-theme` named; logical properties only.

## After (same round, same stack; base = the media block deleted by CSSOM)
- 375 Lounge Light: link blocks 160 x 44, all three pass; "Juniper" 50 x 17, passes (above and below). Regressions (passed on base, not after): none. Fails left: 5 (vouch badge, ring, the three kit buttons above).
- 375 Lounge Dark: same figures (the page draws in its own theme).
- Page height 1239 before and after; scroll width 375 = 375 in both.
- Taps 20 px above and below the centre of each link block reach the block (click handler records its `href`: `bitcoin:...`, `https://ko-fi.com/rowan`, `https://example.com/blog`); taps 20 px above and below "Juniper" both open `/u/sweepmate_mwl2n9kg2bdm`.
- 1280 Editorial: 68 boxes under `main`, 0 moved.
- Public page `/~owner` signed out at 375 (the Owner's page is public in the sweep world): the three link blocks 142 x 42 on base fail below (`code.pp-tipcode`, `section.pp-block`), 142 x 44 after and pass; page height 812 both.
- "View as" preview in Settings: a picture (D-921), not measured.

## Whether the CSS in my last commit is the CSS I measured
Yes: `afbccf43` is the `box-sizing` line measured in the last run (44 px, reported above). Nothing changed after it.

## Who imports the sheet
`packages/profile-blocks` is imported by the public profile page (`apps/web/src/public/profile/PublicProfile.tsx`), the signed-in page, editor, mini profile and Settings' "view as". Public sheet, hence the web build and entrance check below.

## Web build (`pnpm --filter @sideroom/web build`, 36 s, exit 0; on my tree only)
- `check-bundle-budget.mjs`: all ok; first paint `app/app-BOdWJ2DS.js` **218 372 B gzip, room 37 628 B** (before, wave 93: 218 351 B, 37 649 B; +21 B).
- `check-entrance-isolation.mjs`: exit 0, "every public surface is clean" (profile 115.1/150 KB).
- `check-staff-delivery.mjs`: exit 0.
- The build was on my branch alone: the commander should rerun it on the merged tree if other builders touch more than lazy chunks.

## Checks (after my last edit, 14:20)
e2e `npx tsc --noEmit -p tsconfig.json` exit 0; prettier --check on the spec and the sheet clean; `check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok; the spec passed (`--project=chromium`), three runs, the last one reported above. Not run: eslint and web `tsc` (no `.tsx` changed), vitest (CSS only).

## Shots (`visual-sweep-shots/p94-profile`, 5 PNGs plus `p94-profile.json`, all opened by me)
`1-profile-page-375-lounge-light-base`, `...-light-after`, `...-dark-after`, `3-profile-page-1280-after`, `4-public-page-375-after`. (Looked right: the blocks are 44 px, text centred, the guestbook unchanged.)

## The commander's command
From `e2e/`, on a stack with the sweep's world: `SR_E2E_BASE_URL=http://localhost:5722 SR_P94_PROFILE_ON=1 SR_P94_PROFILE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-profile npx playwright test tests/p94-profile-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (use the stack's own Vite port; `SR_P94_PROFILE_BASE=1` for the base figures only). The spec lists the 5 remaining non-mine controls as REPORT lines and fails only on a regression or a moved 1280 box.

## Rows
- SEC-AUD-427 (closes): "On a phone a profile page's link blocks are 44 px tall and the guestbook author link has a 44 px target (D-933)."
- Proposed new row: "Profile page, 375: the guestbook's Report, Approve and Remove buttons (kit `Button size=sm`, 28 px high, stacked 8 px apart) take each other's taps (below); the vouch badge (69 x 19) and the ring button (40 x 40) fail above and below." Lives in `profilepage/blocks.tsx` and the kit: not this wave's files.
- Changelog: "On a phone, the link buttons on a profile page and a guestbook signer's name are easier to tap."
- No AGENTS.md sentence, no owner questions, no threat-model rows.

## Cleanup
Stack stopped by PID tree (8722 / 5722 / 9422 free), `sr_p94_profile_ui` dropped, `p94-profile-bin` and my scratch folder deleted (`p92-base-bin`, `p80-base-bin` untouched), lock released through `heavy.sh` (free), no stash touched. The first lock attempt ran inside a call that the tool moved to background after 180 s; I stopped it by task id before it took the lock and waited with `timeout`-limited foreground `acquire` calls afterwards.
