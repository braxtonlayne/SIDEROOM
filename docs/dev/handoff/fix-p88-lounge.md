# Wave 88 lounge handoff: SEC-AUD-380 (D-892), the phone header strip closes after "Member List"

T = 16:53 on 2026-11-16. Clock: ready 16:54, lock taken 16:54, released 16:57, finished 16:59.

## Done
- `lounge/ChannelHeader.tsx`: the "Member List" button's `onClick` calls `onToggleMembers()` then `setMoreOpen(false)`. No new prop, no effect, no import.
- Vitest: `lounge/__tests__/p71Panels.test.tsx` (the file that renders `ChannelHeader`), one new case "closes the strip when Member List is chosen on a phone (D-892)" (strip opened with the file's existing phone `header(vi.fn())` helper, no new helper). 19 passed.
- Spec `e2e/tests/p88-lounge-shots.spec.ts` (copy of wave 87's, reads `SR_P88_LOUNGE_*` only).
- Checks: prettier, eslint (header and test), web `tsc`, e2e `tsc`, `check-doc-links` all clean.

## Base (ran it, spec base case, 375x812 touch, Lounge Dark, `#general`)
Strip open (More tapped): `aria-expanded` true, `.lng-dm-extra` data-open true. After "Member List": pane `members`, `aria-expanded` still true, data-open still true. After the swipe back: pane `chat`, `aria-expanded` true, data-open true (the strip open over the chat). Fault confirmed.

## After (spec, all passed in 29 s)
- Lounge Dark and Lounge Light, 375: after the tap pane `members`, member row box [8,72,359,42] inside the viewport, `aria-expanded` false, data-open absent; after the swipe back pane `chat`, strip closed, composer box [8,663,359,68] in view; reload opens `chat`; page overflow 0.
- 1280 Editorial: the button toggles the column as before. Header boxes against the base: open [312,53,724,49], closed [312,53,968,49], identical to the base figures (0 px moved); main column and composer boxes also identical.

## Who imports `ChannelHeader.tsx`
`lounge/ChannelView.tsx`, `lounge/voice/VoiceChannelView.tsx`, `lounge/voice/StageView.tsx`, and the vitest file. I added no import.

## Left
Voice and stage views' Member List buttons (still empty callbacks), the DM header on a phone (not shot), the members pane has no Back control, D-893 not mine.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-lounge`: 17 PNG plus `measure.json`, `base-measure.json`. Opened after the lock was released: base after-tap, base back (strip open), phone-lounge-dark after-tap, phone-lounge-dark back, phone-lounge-light back, desktop-editorial-open (the rest are the same states as these: base before/strip-open, dark before/strip-open, light before/strip-open/after-tap, desktop-closed).

## Commander's command (from `e2e/`, sweep world seeded; also SR_CLI_BIN, SR_DATABASE__URL, DATABASE_URL of the stack)
`SR_E2E_BASE_URL=http://localhost:5662 SR_P88_LOUNGE_ON=1 SR_P88_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-lounge npx playwright test tests/p88-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (base case: add `SR_P88_LOUNGE_BASE=1`; it measures and shoots with no assertions).

## Proposed inventory row
SEC-AUD-380 (closed, wave 88, D-892): on a phone the channel header's strip stayed open after "Member List" and after the swipe back; it now closes when the button is chosen (`ChannelHeader`). Open: voice and stage views' buttons.

## Changelog sentence
On a phone, choosing "Member List" in a channel's header now closes the header's More strip.

## Owner questions
None.

## Housekeeping
Stack stopped by Windows PID tree, ports 8662/5662/9362 free, `sr_p88_lounge_ui` dropped, `p88-lounge-bin` deleted, lock released. No stash.
