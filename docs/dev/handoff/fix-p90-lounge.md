# Wave 90 lounge handoff: SEC-AUD-385 and SEC-AUD-386 (D-899), "Member List" in voice and stage and the group DM identity tap on a phone

T = 05:50 on 2026-11-17. Clock: ready 05:59, round 1 (voice and stage) lock 06:02 to 06:05, round 2 (the DMs) lock 06:25 to 06:30 (queued from 06:05 behind seed, entrance, arcade, invoices, members), finished 06:35.

## Done (commits on `fix/p90-lounge`)
- `lounge/ChannelView.tsx`: passes `onShowMembers` to `VoiceChannelView` and `StageView`; in the DM `onIdentity` branch that called `setMembersOpen(true)`: `if (onShowMembers) onShowMembers(); else setMembersOpen(true);` (the 1:1 pop-out branch is untouched).
- `lounge/voice/VoiceChannelView.tsx`, `lounge/voice/StageView.tsx`: a new optional prop `onShowMembers`; the header gets `onToggleMembers={onShowMembers ?? (() => undefined)}`. No store field, no import. `LoungeShell` needed no change: `showRight` already draws the members pane for any channel on a phone (`phoneMembers`).
- Vitest: one case each in `voice/__tests__/voiceUi.test.tsx` and `voice/__tests__/stageUi.test.tsx` (the header's "Member List" calls the prop), one in `__tests__/p11ShellDm.test.tsx` (phone `matchMedia` mock: a group DM's identity tap sets `data-pane="members"` and leaves `memberListOpen` false). Watched fail without the change: the p11ShellDm case (base `ChannelView.tsx` restored from `HEAD~2`: fails; with the change: passes). The voice and stage cases pass the new prop, which does not exist on the base, so they cannot pass there (not run on the base).
- Spec `e2e/tests/p90-lounge-shots.spec.ts` (copy of wave 88's; two tests, `-g voice` and `-g dm`; `SR_P90_LOUNGE_*` only).
- Checks: prettier, eslint (the five source and test files), web `tsc`, e2e `tsc`, `i18n-audit --max 0` (TOTAL 0), `check-doc-links` ok. Vitest single files: stageUi 11 passed, voiceUi (new case) passed, p11ShellDm (new case) passed. No web build, no cargo.

## Base (ran it: the spec's base case with the three source files restored from `HEAD~2`, 375x812 touch, Lounge Dark)
- Voice and stage channel (seeded `General` voice channel; a `Town Hall` stage channel made through the API): More, "Member List": pane stays `chat`, members pane at x=375 (off screen), stored `memberListOpen` true before and after. Nothing happens. (A swipe right afterwards goes to the channel list.)
- 1280 Editorial, voice channel: the button reads "Show Member List" with `aria-pressed` false (the view passes `membersOpen={false}`), the member column is open (stored true), and a click changes nothing (stored true, column unchanged). Not changed by this wave; the tooltip/`aria-pressed` staying "closed" while the column is open is the same stale state, listed below.
- Group DM (two new friends of the Owner, since the world has one other member and a stranger's DM is a message request): stored flag set to false first. Identity tap: pane stays `chat`, nothing shows, **stored becomes true** (the fault). 1:1 DM identity tap: the pane stays `chat` and the stored flag is untouched (it opens the profile pop-out, which the spec did not shoot; the branch is not changed by this wave and is the same on the base and after).
- DM header button on a phone: 1:1 DM, "User Profile" (More, then the button): pane `members`, the profile card (name, member since, mutual servers, View Full Profile). Group DM, "Member List": pane `members`, the group panel (Members - 3, owner marked, Add friends, Group settings, Invite link). Stored unchanged. This is D-893 as built; kept.

## How the request travels
Header button `onClick` -> `onToggleMembers()` (then `setMoreOpen(false)`, D-892, `ChannelHeader`) -> `ChannelView`'s `onShowMembers` prop (phone only; `LoungeShell` `Main` passes `isMobile ? showMembers : undefined`) -> `setPane('members')` in the shell -> `showRight` (`phoneMembers`) draws `.lng-right`. Voice and stage now take the same route through the new prop. The identity tap goes `ChannelHeader` `dm.onIdentity` -> `ChannelView` -> `onShowMembers()`.

## After (spec, passed; voice test 34 s, dm test 1.1 min)
- Voice and stage, 375 Lounge Dark; voice 375 Lounge Light: after the tap pane `members`, member row box [8,72,359,42] inside the viewport, members pane [0,29,375,726], strip closed (`aria-expanded` false, `data-open` absent: D-892 holds, no header problem), stored `memberListOpen` equal before and after (true), page overflow 0. A real touch swipe returns to `chat` with the Join Voice (voice [136,391,104,36]) or Join (stage [8,204,105,36]) button in view.
- 1280 Editorial, voice and stage: header box [312,53,724,49] and view box [312,53,724,747] equal to the base before and after the click (0 px), the button does what it did (nothing; stored true).
- Group DM, 375 Lounge Dark and Light: identity tap shows the group panel (`.lng-right`, label "Profile": Members - 3, Add friends, Group settings, Invite link), box [0,29,375,726] inside the viewport; stored stays false (the base wrote true); a swipe returns to the chat with the composer in view; overflow 0. Header buttons as in the base (1:1 shows the profile card, group the members panel).
- 1280 Editorial, group DM: the tap opens the member column [1040,53,240,747], stored becomes true (as before); header [312,53,724,49] and view boxes equal to the base (0 px).

## Who imports my files (lazy chunk, no web build)
`ChannelView.tsx` is imported only by `lounge/LoungeShell.tsx`; `VoiceChannelView.tsx` and `StageView.tsx` only by `ChannelView.tsx`; `LoungeShell.tsx` only by `routes/LoungeRoot.tsx`, which `routes/lounge.tsx` loads with `lazyNamed(() => import('./LoungeRoot'))`. All in the Lounge chunk; no first-paint file touched, no import added.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-lounge`: 12 PNG (listed) plus `measure.json` and `base-measure.json`. Item 1 (6): `base-voice-phone-lounge-dark-after-tap`, `voice-phone-lounge-dark-after-tap`, `voice-phone-lounge-light-after-tap`, `stage-phone-lounge-dark-after-tap`, `voice-phone-lounge-dark-back`, `voice-desktop-editorial`. Item 2 (6): `base-dm-group-phone-lounge-dark-identity-after-tap`, `dm-group-phone-lounge-dark-identity-after-tap`, `dm-group-phone-lounge-light-identity-after-tap`, `dm-one-phone-lounge-dark-header-after-tap`, `dm-group-phone-lounge-dark-header-after-tap`, `dm-group-desktop-editorial-after-tap`. **All 12 opened and looked at** (the 14 files in the folder are these 12 and the two json files). The account theme was set by `PATCH /users/@me/settings` and read back each time (`theme` in `measure.json` shows the room's `data-theme`).

## Commander's command (from `e2e/`, the sweep world seeded on the same database; also SR_CLI_BIN, SR_DATABASE__URL, DATABASE_URL of the stack)
`SR_E2E_BASE_URL=http://localhost:5682 SR_P90_LOUNGE_ON=1 SR_P90_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-lounge npx playwright test tests/p90-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`-g voice` or `-g dm` for one test; the stack needs `SR_VOICE__ENABLED=true`; the base case adds `SR_P90_LOUNGE_BASE=1` with the three source files switched off). The dm test registers two accounts (the spec's own, through `registerUser`).

## Not done / findings (not mine to change)
- The stale button state on a desktop voice and stage channel: tooltip "Show Member List" and `aria-pressed` false while the column is open, and the click does nothing (the views pass `membersOpen={false}`). Unchanged on purpose (the task: the desktop is not changed). A follow-up could pass the stored flag and a toggle on a desktop; owner question below.
- The group DM's panel on a phone has its buttons and heading flush to the left edge ("Add friends", "Group settings", "Invite link" at x=0, no gutter) and the 1:1 card has a gutter: a `lounge.css` / panel CSS matter (fenced).
- The first Playwright run after a source change can fail to find `.lng-header` while Vite reloads: run it twice.
- I found no stash entries and touched none.

## Proposed inventory rows
- SEC-AUD-385: On a phone, "Member List" in a voice and a stage channel's header did nothing (`onToggleMembers={() => undefined}`, `ChannelView` did not pass `onShowMembers`). Fixed (D-899): the optional prop reaches the header, the pane shows, the strip closes, the stored flag is not written. Shots `p90-lounge`.
- SEC-AUD-386: On a phone the group DM identity tap wrote the stored `memberListOpen` and showed nothing. Fixed (D-899): it shows the pane and never writes the flag; the DM header button on a phone is shot (1:1: profile card; group: members panel; D-893 kept).

## Changelog sentences
- SEC-AUD-385: On a phone, "Member List" in a voice or stage channel now shows the members pane.
- SEC-AUD-386: On a phone, tapping a group DM's name now shows its members instead of silently changing the desktop's saved setting.

## Docs
No doc states these behaviours; nothing to change. No threat-model rows.

## Owner questions
- On a desktop voice and stage channel, should "Member List" toggle the member column like in a text channel (it does nothing now and reads "Show Member List" while the column is open)? Default taken: left as it is.

## Cleanup done
Stack stopped by PID tree (ports 5682, 8682, 9382 free), `sr_p90_lounge_ui` dropped, `p90-lounge-bin` deleted (`p80-base-bin` untouched), lock released.
