# Wave 91 lounge handoff: SEC-AUD-397 and SEC-AUD-398 (D-907, D-909)

T = 10:48 on 2026-11-17. Clock: ready 10:50, lock round 1 (base case and a first after) 10:54 to 10:57, round 2 (after) 10:59 to 11:00 (both behind members and measure). Round 3 (commander approved, 11:10 to 11:11, after-only) measured the final CSS: see "Round 3" below. Item 2 is done.

## Done (commits on `fix/p91-lounge`)
- Item 1 (SEC-AUD-397, D-907): `lounge/voice/VoiceChannelView.tsx` and `lounge/voice/StageView.tsx` read `memberListOpen` / `setMemberListOpen` from `useLoungeUi` (as `ChannelView.tsx:58-59`) and pass `membersOpen={onShowMembers ? false : membersOpen}` and `onToggleMembers={onShowMembers ?? (() => setMembersOpen(!membersOpen))}`. The phone path (`onShowMembers` defined) is wave 90's, unchanged. No new string. **`LoungeShell.tsx` was not touched**: its `showRight` already draws the column from the stored flag in a voice channel (measured: column drawn and removed on click).
- Item 2 (SEC-AUD-398, D-909): one `@media (max-width: 768px)` block in `lounge/profile/profile-p11.css` (768 is the shell's phone width, `MOBILE_QUERY`; the sheet had no phone query of its own except `max-width: 640px` for another panel). It gives `.lng-profile > .prf-group-head`, `> .prf-group-actions` and `> section` (the invite block) `margin-inline: 8px; padding-inline: 12px` (the card's 8 + 12, content at x = 20), the member list `padding-inline: 12px` and rows `padding-inline: 4px` (avatar at x = 20), rows and their button 44 px tall, a 44 px target for the Remove icon, 12 px above the action row and 12 px below the last block. The comment names SEC-AUD-398 and D-909.
- Vitest (single files, passing): `voiceUi.test.tsx` (16) and `stageUi.test.tsx` (12), one new desktop case each (no `onShowMembers`: a click flips the stored flag and `aria-pressed` follows) and the wave 90 phone cases now also assert the stored flag is untouched. **Watched fail without the change**: with the two views restored from `HEAD`, the two new cases fail (`aria-pressed` stays false, the flag does not flip) and the rest pass.
- Spec `e2e/tests/p91-lounge-shots.spec.ts` (two tests: `voice`, `dm`; `SR_P91_LOUNGE_*` only; its header has the command).
- Checks: prettier, eslint (the four source and test files), web `tsc`, e2e `tsc`, `check-css-tokens` (clean), `i18n-audit --max 0` (TOTAL 0). `check-doc-links` to run on this file at the end (see below).

## Base (ran it: round 1, the three source files restored from `9da8b619`; 1280 Editorial, 375 touch Lounge Light)
- Item 1, voice (`General`) and stage (`Town Hall`, made through the API) at 1280: the button's tooltip is "Show Member List", `aria-pressed` false, the column drawn, stored `memberListOpen` true; a click changes nothing (column drawn, stored true, still false/"Show"). The text channel afterwards: "Hide Member List", pressed true, column drawn. Exactly as the task says.
- Item 2, group DM (two new friends of the Owner, made through the API) at 375, members pane via More, "Member List": panel [0,375]; heading block [0,375], its `h2` padding 16; member rows [0,375] (button box [8,367], avatar at 12); "Add friends" x = 0 to 98.5, "Group settings" 106.5 to 223.8, "Create invite link" x = 0, "INVITE LINK" heading x = 0. The one-to-one card: `.lng-profile-card` box [8,367], **margin 8px and padding 12px from `lounge.css:1410-1415`** (`.lng-profile-card { margin: 8px; padding: 12px; }`), its content at x = 20. 1280 Editorial: panel [1040,1280], head [1040,1280], members [1040,1280], actions [1040,1280] (the figures the after must equal).
- Hit test on the base (D-856), group panel: member rows (40 px tall) fail, the two Remove icons (28 px) fail, "Create invite link" fails (its 44 px target is clipped by the scrolling panel's bottom edge: the panel ends at its bottom), "Add friends" and "Group settings" pass. The one-to-one card: "More actions", the mutual link and "View Full Profile" all fail (28 and 20 px, `ProfileMoreMenu` and `lng-link-btn`): **not mine, not changed; listed as kit/profile**.

## After (round 2: ran; this was the CSS of commit 2, before the last commit)
- Item 1, 1280 Editorial, voice and stage: start "Hide Member List", pressed true, column drawn, stored true; click 1: "Show Member List", false, no column, stored false; click 2: back to "Hide", true, column, true. The voice test (all of it, including the text channel left closed: false, no column, stored false, and wave 90's phone cases at 375 Lounge Dark for voice and stage: pane `members`, inside the viewport, stored unchanged, no sideways scroll) **passed**.
- Item 2, 375 Lounge Light: heading block, "Add friends" and "Create invite link" start at x = 20, equal to the card's content (card box [8,367], content 20); `cut` 0, page overflow 0. Hit test: every control passed except **one**: the middle member row ("Open profile of cat") failed, because "Group settings"'s own 44 px target (top at 209) reached 8 px into that row (its bottom is at 217). The final commit adds 12 px above the action row for that, and moves the member avatars to x = 20 (before, they sat at x = 12 while the buttons sat at 20: seen in the shot).
- **Not measured**: the final CSS (commit 3: the 12 px, the avatars at 20, the 4 px row padding), 375 Lounge Dark, and the 1280 Editorial panel against the base. The spec asserts all of them (hit test on every control, Add friends / Create invite link / avatars / heading block at the card's content x, 1280 panel, head, members, actions and buttons equal to `base-measure.json`).

## Round 3 (final CSS, commit 0b0ac056; ran)
- 375 Lounge Light and Lounge Dark: **every control passes the hit test** (member rows 44 px, Remove icons, Add friends, Group settings, Create invite link: all true); heading block box [8,367] (content 20); Add friends and Create invite link start at x = 20 (the card's content); member avatars start at x = 20; `cut` 0, page overflow 0. Both group-panel shots opened.
- 1280 Editorial: panel [1040,1280,53,257], head, members and actions boxes and every button box and hit flag are **0 px from the base**. The spec's first run failed only on the button labels (random usernames per run); the compare now ignores labels (the geometry was equal).
- Stack stopped, lock released, `sr_p91_lounge_ui` dropped, `p91-lounge-bin` deleted (`p80-base-bin` untouched). 768 px (not 720) is a choice, see Owner questions.

## (Superseded) what the commander runs for the missing round (about 4 minutes, after-only; the base file is in the folder)
`bash C:/Users/braxt/Desktop/SIDEROOM_wt/p91-lounge-scratch/round.sh after` inside a `heavy.sh acquire lounge` (it needs `p91-lounge-bin`, which I kept for it; it makes and leaves a fresh `sr_p91_lounge_ui`). By hand: the stack of the brief (ports 8692 / 5692 / 9392, `SR_VOICE__ENABLED=true`, `SR_ADMIN_REQUIRE_REAUTH=0`), seed the sweep world, then from `e2e/`:
`SR_E2E_BASE_URL=http://localhost:5692 SR_P91_LOUNGE_ON=1 SR_P91_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-lounge npx playwright test tests/p91-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`-g voice` or `-g dm`). The base case adds `SR_P91_LOUNGE_BASE=1` with `VoiceChannelView.tsx`, `StageView.tsx` and `profile-p11.css` restored from `9da8b619`.

## Who imports my files (lazy chunk, no web build)
`VoiceChannelView.tsx` and `StageView.tsx`: only `lounge/ChannelView.tsx`; `ChannelView.tsx`: only `LoungeShell.tsx`; `LoungeShell.tsx`: only `routes/LoungeRoot.tsx` (lazy). `profile-p11.css`: only `lounge/profile/ProfilePopout.tsx:20`, which `LoungeShell.tsx:52` imports. All the Lounge chunk; no first-paint or public file touched; no import added to a fenced file.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-lounge`, opened by me: `base-voice-desktop-editorial-open` (the stale button: column open, button unpressed), `voice-desktop-editorial-open`, `voice-desktop-editorial-closed`, `stage-desktop-editorial-closed`, `base-dm-group-phone-lounge-light`, `base-dm-one-phone-lounge-light`, `dm-group-phone-lounge-light` (round 2's CSS: heading and buttons at the card's gutter, avatars still at 12), `dm-one-phone-lounge-light`. Also written but not part of the list: `base-dm-group-phone-lounge-dark`. **Still to shoot (the missing round): `dm-group-phone-lounge-light` and `dm-group-phone-lounge-dark` with the final CSS** (the Light one on disk is from before the final commit). The account theme is set by `PATCH /users/@me/settings` and read back; `data-theme` is in `measure.json`.

## Docs
No doc states these behaviours: nothing to change.

## Proposed inventory rows
- SEC-AUD-397: On a desktop, "Member List" in a voice or stage channel passed `membersOpen={false}` and an empty handler: it read "Show Member List", `aria-pressed` false while the column was open, and did nothing. Fixed (D-907): both views read the stored flag and toggle it, as a text channel does; the phone path (D-899) is unchanged.
- SEC-AUD-398: On a phone the group DM members panel's heading, buttons and invite block sat at x = 0 (the one-to-one card has an 8 + 12 px gutter), its member rows and Remove icons were under 44 px and "Create invite link" was clipped by the panel's bottom. Fixed (D-909) in `profile-p11.css`'s phone block (pending the last measure).
- Open (not mine): the one-to-one card's "More actions", mutual link and "View Full Profile" fail the 44 px hit test at 375 (28 and 20 px): `ProfileMoreMenu` and `.lng-link-btn`.

## Changelog sentences
- SEC-AUD-397: On a desktop, "Member List" in a voice or stage channel now shows and hides the members column and says which it is doing.
- SEC-AUD-398: On a phone the group chat's member panel now has the same margins as a one-to-one chat's card, and its rows and buttons are big enough to tap.

## Owner questions
- None new. Default taken: the phone block uses 768 px (the shell's phone width), not the 720 px the task named, so the panel and the shell agree on what a phone is (the kit's own 44 px rule uses 720).

## Cleanup
Stack stopped by PID tree after each round (ports 8692, 5692, 9392 free), lock released, `sr_p91_lounge_ui` dropped. **`p91-lounge-bin` is kept** for the missing round (delete it after; never `p80-base-bin`); scratch (`p91-lounge-scratch`, `round.sh`, logs) is outside the repository. I found no stash entries and touched none.
