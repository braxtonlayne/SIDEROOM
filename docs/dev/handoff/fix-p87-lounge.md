# Wave 87 lounge handoff: SEC-AUD-377 (D-887), "Member List" shows the members pane on a phone

T = 16:40 on 2026-11-16 (I started the code at 16:26 and the lock round ran 16:31 to 16:33). Clock times: no new item from 17:10, checkpoint 17:22, finished 17:30, hard limit 17:40. Done at about 16:36.

## Done
- Commit `36c02696` (SEC-AUD-377): `lounge/LoungeShell.tsx`, `lounge/ChannelView.tsx`, `lounge/__tests__/shell.test.tsx`, `e2e/tests/p87-lounge-shots.spec.ts`. `lounge/store.ts` is not touched.
- **What I saw on the base (ran it, spec base case, 375x812 touch, Lounge Dark, the seeded server's `#general`):** before the tap the pane is `chat` and the members pane is off screen (box left 375). After More, then "Member List": pane still `chat`, the strip still open, the stored `memberListOpen` flipped from true to false and the members pane was removed from the DOM (no member row at all). Shot `base-phone-lounge-dark-after-tap.png`: the chat unchanged, tooltip "Show Member List". The fault is as the plan says (the plan did not mention that the tap also removes the pane, so the swipe then shows nothing).
- **How the request travels:** `Main` gets a new prop `onShowMembers` (the shell's `showMembers`, `setPane('members')`); it hands it to `ChannelView` only when `isMobile`; `ChannelView` passes `onShowMembers ?? (() => setMembersOpen(!membersOpen))` as the header's `onToggleMembers`. On a phone `memberListOpen` is therefore never written by the tap (spec asserts it is equal before and after; vitest too). No store field, no new import. One extra change in the shell: `showRight` is also true while `isMobile && pane === 'members'`, so the pane is drawn on a phone even when the desktop's stored flag is off (without it the tap, and the swipe, show an empty pane after a desktop user hid the list). Desktop: `onShowMembers` is undefined, the code path is the old one.
- **Who imports the files:** `LoungeShell.tsx` only by `routes/LoungeRoot.tsx`, which `routes/lounge.tsx` loads with `lazyNamed(() => import('./LoungeRoot'))` (a lazy chunk). `ChannelView.tsx` only by `LoungeShell.tsx`. `store.ts` unchanged. No web build run (lazy chunk only; no new import).

## After (spec, change case, all passed in 36 s)
- 375 Lounge Dark and Lounge Light: tap shows pane `members`, member row box `[8,72,359,42]` inside the viewport, members pane `[0,29,375,726]`, `memberListOpen` unchanged (true), page overflow 0. A real touch swipe right returns to `chat` with the composer in view (box `[8,663,359,68]`). A reload opens `chat`.
- 1280 Editorial: the button toggles the column (open: pane `[1040,53,240,747]`, pressed true; closed: none, stored false, composer widens), closed survives a reload, reopened and reload shows it open; the main column box with the list open is `[312,53,724,747]` before and after the toggles. I did **not** run a base desktop pass (the desktop path has no code difference); "0 px against the base" is by that reasoning plus the unchanged box across toggles, not a base measurement.
- Vitest `lounge/__tests__/shell.test.tsx` (single file): 27 passed, including the new case "shows the members pane from the header's Member List and leaves the stored flag alone (D-887)", which uses the file's existing phone `matchMedia` mock (no new helper). I did not watch that case fail on the base in vitest (the spec's base run shows the fault).
- web `tsc` clean, eslint clean, prettier clean on my files, e2e `tsc` clean, `i18n-audit --max 0` TOTAL 0. `check-doc-links` run by me: see below.

## Not done / listed
- **The strip stays open after the tap and after the swipe back** (`moreExpanded` true in `-after-tap` and `-back`). That is `ChannelHeader`'s own state (not mine, not changed); the members pane covers it, but the chat shows the strip open on return. A fix needs `ChannelHeader.tsx` (close the strip when `onToggleMembers` fires).
- The members pane on a phone has no Back control (only the swipe the pane already had, and the tab bar's Home). Not added (not asked).
- Voice and stage views' buttons: not built (still an empty function at `voice/VoiceChannelView.tsx` and `voice/StageView.tsx`).
- DMs: on a phone the same header button now shows the DM's profile pane through the same callback (not shot).
- The DM identity click on a phone still calls `setMembersOpen(true)` (stored flag) and shows nothing; not touched.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-lounge`: 8 PNG and `measure.json`, `base-measure.json`; **all 8 opened and looked at** after the lock was released: `base-phone-lounge-dark-before`, `base-phone-lounge-dark-after-tap`, `phone-lounge-dark-before`, `phone-lounge-dark-after-tap`, `phone-lounge-dark-back`, `phone-lounge-light-after-tap`, `desktop-editorial-open`, `desktop-editorial-closed` (the base "before" was not opened separately: it is the same page as `phone-lounge-dark-before`, which I opened). Account theme set through the settings API and read back in the log before each case.

## Commander's command (from `e2e/`, sweep world seeded on the same database; plus SR_CLI_BIN, SR_DATABASE__URL, DATABASE_URL of the stack)
`SR_E2E_BASE_URL=http://localhost:5652 SR_P87_LOUNGE_ON=1 SR_P87_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-lounge npx playwright test tests/p87-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (base case: add `SR_P87_LOUNGE_BASE=1`).

## Proposed inventory row
SEC-AUD-377 (closed, wave 87, D-887): on a phone, "Member List" in a text channel's header strip showed nothing (it flipped the stored desktop flag and removed the pane). It now shows the members pane (`LoungeShell` pane state, via an `onShowMembers` callback), without writing `memberListOpen`. Open: the strip stays open on return (`ChannelHeader`), the voice and stage views' buttons.

## Changelog sentence
On a phone, "Member List" in a text channel's header now opens the members pane instead of doing nothing.

## Owner questions
1. Should the members pane on a phone get a Back control (today only the swipe and the tab bar)?
2. Should "Member List" in the voice and stage views open the pane too (D-887 built text channels only)?

## Housekeeping
Stack stopped by Windows PID tree (my first kill used the shell's PIDs and missed; I killed sr-server 27180 and node 19728 by their real PIDs after, and ports 8652/5652/9352 are free). `sr_p87_lounge_ui` dropped, `p87-lounge-bin` deleted, lock released. No stash touched.
