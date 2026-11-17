# Wave 93, settings: SEC-AUD-412, 413, 415 (D-923) and 414 (D-921)

Clock: start 12:54; lock taken 13:22 (waited behind lounge, study, admin); stack, seed, four spec runs inside the round; lock released 13:29. Round about 7 minutes.

## Who loads the sheet
`git grep -n "user-settings.css"`: `settings/user/parts.tsx` (every Settings tab, lazy), `support/SupportPage.tsx`, `support/Leaderboards.tsx` (lazy in `support/routes.tsx`). Nothing first paint or public. No web build run.

## Base (ran it; 375 touch, Lounge Light)
- Account rows (Username, Display name, Email) 32 px apart (pitch 32). "Edit" x2 are 25.6 x 15: Username's point below taken by "Edit display name"; Display name's below taken by the Email row's "Reveal". A tap 21 px under Username's Edit opened "Change your display name".
- "Edit your profile page" 162 x 20: above and below taken by the card (`section.us-card`). "View leaderboards" 141.8 x 20: above `div.us-card-head`, below `section.us-card`.
- Cards' heights: Account 319, Profiles' page card 141, Support's Leaderboards card 379.

## Built (`user-settings.css`, one `@media (max-width: 720px)` block at the end, comment names the rows and D-923)
`.us-field:has(dd button) { min-block-size: 44px; align-items: center }` and `.us-card > a.us-link, .us-actions > a { display: inline-flex; align-items: center; min-block-size: 44px }`.
Moved (all measured, base to after): Account Username 32 to 56, Display name 32 to 56, Email 53 to 56 (the only `.us-field` rows with a button in the 20 pages); Profiles' link 20 to 44; Support's link 20 to 44. Nothing else (no other `.us-actions > a` or `.us-card > a.us-link` on any of the 20 pages).
Why rows are 56, not 44: `min-block-size: 44px` with the row's 6 px padding is 56. I tried `box-sizing: border-box` at 44 and at 48: Display name's point below was then taken by the Email row's "Reveal" (the Email row has two lines of buttons, Reveal on the first), so I went back to the 56 that passes. **The CSS in my last commit is the CSS I measured in the final run (run 4).**

## After (375 touch)
- Lounge Light and Lounge Dark: all four pass, each its own taker (no failing point); tap 21 px under Username's Edit opens "Change your username". Card heights Account 319 to 370, Profiles' card 141 to 165, Support's 379 to 403. No sideways scroll (375/375 on all three pages, both themes).
- 1280 Editorial, touchless: 46 boxes (`.us-field`, `.us-card`, the links, action buttons, dd buttons) of the three pages, base vs after: 0 moved.

## The 20 pages, 375 Lounge Light, `openCollapsed` then `hitTestAll`, rows inside `.us-view-as [role="region"]` filtered in the spec (the helper is untouched). Not reached: 0 on every page, base and after.
Tested / opened / left out / skipped / failures, base then after: My Account 14/0/0/0 2 then 0; Profiles 79/1/3/0 3 then 2; Privacy & Safety 28/0/0/0 0; Activity Privacy 28/0/0/1 0; Authorized Apps 0; Modules 1/0/0/2 0; Devices 2/0/0/1 0; Connections 13 0; Invites 1/0/0/1 0; Storage 1 0; Support / Patron 8/0/0/10 1 then 0; Appearance 38/0/0/1 0; Accessibility 18 0; Text & Images 15 0; Notifications 22/0/0/1 0; Voice & Video 30/0/0/1 2 then 2; Keybinds 30 0; Language 7 0; Streamer Mode 1/0/0/3 0; Advanced 5 0.
Base failures 8 (the 4 of this task, plus Profiles' two "view as" segmented inputs and Voice & Video's two). **After: 4, all kit controls**: Profiles "view as" inputs 132 x 32 and 70 x 32 (below taken by the preview `div.pp-root`), Voice & Video segmented input 74 x 32 (below: next control's input) and slider 309 x 44 (below: next slider input). Nothing else: no proposed row beyond them.

## Link blocks on the real profile page (`/u/owner`, 375 touch, report only)
Bitcoin, Ko-fi, Blog: each 160 x 42; centre and above pass, **below fails** (21 px under the centre is the box's own edge; taker: the `li` (Bitcoin), `code.pp-tipcode` (Ko-fi), `section.pp-block` (Blog)). Also the guestbook author link "Juniper" 50 x 17: above (`span._fallback`, the avatar) and below (`p.pp-text`) fail. Drawn by `profilepage/**` (`pp-linktree` links, the guestbook entries): not mine. Proposed row: "Profile page: link blocks are 42 px tall (need 44) and the guestbook author link is 50 x 17".

## Shots (`visual-sweep-shots/p93-settings`, 8 PNGs, all opened by me after the release)
1-account light base / light after / dark after; 2-profiles light after and dark after; 3-support light after and dark after; 4-profile-page (it draws dark: the page's own theme). Plus `p93-settings.json`. The dark Profiles and Support shots I did not open individually beyond the light ones seen (same code path): I opened the six listed in the task; the two dark extras are unopened.

## Checks
e2e `tsc` clean; prettier clean; `check-css-tokens` clean; spec passes `--project=chromium --reporter=line`. No vitest (CSS only). Base case in the spec = the D-923 media rule deleted from the loaded sheet by CSSOM.

## Commander's command
From `e2e/` on the merged stack (API 8712 / Vite 5712, sweep world seeded): `SR_E2E_BASE_URL=http://localhost:5712 SR_P93_SETTINGS_ON=1 SR_P93_SETTINGS_STRICT=1 SR_P93_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-settings npx playwright test tests/p93-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (the "base" half of case 1, 2 and 3 strips the rule, so it still runs; STRICT asserts 0 failures on the after pass).

## Rows
- SEC-AUD-412: "My Account: Username and Display name 'Edit' rows are 32 px apart so their 44 px targets overlap; on a phone a `.us-field` row with a button is 56 px (D-923)." 413: "Profiles' 'Edit your profile page' link has a 44 px tall box on a phone." 415: "Support / Patron's 'View leaderboards' likewise." 414: "The 20-page pass runs without the 'view as' preview (D-921): 4 failures left, all kit (Voice & Video x2, 'view as' x2)."
- Changelog: "On a phone, My Account's Edit buttons no longer take each other's taps, and 'Edit your profile page' and 'View leaderboards' have full-size tap areas."
- Owner questions: none. Row pitch 56 grows My Account's card by 51 px at 375 (Discord-like default kept; say if a tighter row is wanted, then Email's row needs its own fix).

## Cleanup
Stack stopped by port owner PID tree (8712 / 5712 / 9412 free), `sr_p93_settings_ui` dropped, `p93-settings-bin` deleted (`p92-base-bin`, `p80-base-bin` untouched), lock released, no stash touched.
