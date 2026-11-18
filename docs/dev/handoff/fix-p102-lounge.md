# Wave 102, lounge: D-967, SEC-AUD-453: `.lng-field select` leaves lounge.css; five Lounge dialogs' selects move to the kit's `NativeSelect`; the wave's one web build

2026-11-18. Started 09:30. Code and spec committed 09:41 (4dc71e35). Lock `lounge` 10:21 to 10:26 (stack round), 10:28 to 10:29 (build). ShelfPanel put back to raw after the round (commit after 4dc71e35).

## Done
- `lounge.css`: `.lng-field select,` removed from the shared rule (it is now `.lng-invite-link input { ... }` alone). The grep below shows no raw `<select>` under any `.lng-field` in the base tree plus my changes. Chat's and cmd's trees are not visible to me: neither file they own has a `.lng-field` (grep of the base), so the commander can treat item 1 as finished at integration.
- Moved to `NativeSelect` (class `lng-select` dropped; its two sheets `shell.css:2` and `events.css:70` are not mine and now have no user, listed below): Notification dialog "Mute for", Timeout and Ban dialogs (`MemberDialogs`), Custom status "Clear after", Create Event channel, Create Poll duration.
- **`scrapbook/ShelfPanel.tsx`: left raw** (put back to the base's text). With the kit select, the wrapper span is 100 % wide in the `lng-scr-actions` flex-wrap row and the "Use this shelf" button wraps below it: every control under it moves 48 px at 1024 and 52 px at 375 (allowed 5 and 7). Not fixable with props or classes, so per the rule it stays raw. Its base figures: select 152 x 32, 375 hit test as wave 101 (fails; taker the Storage shelf section / filter form).

## Every `.lng-field` in `apps/web/src` that holds a select (git grep, base tree)
| file | select | before | now | direct child of `.lng-field` |
|---|---|---|---|---|
| `lounge/InviteDialog.tsx` (3) | kit (wave 101) | kit | kit | yes |
| `shell/NotificationDialog.tsx` | Mute for | raw | kit | yes |
| `shell/MemberDialogs.tsx` | Duration / Delete history | raw | kit | yes |
| `shell/CustomStatusDialog.tsx` | Clear after | raw | kit | yes |
| `msg/PollModal.tsx` | Duration | raw | kit | yes |
| `msg/UploadTray.tsx` | none (inputs only) | | | |
CreateEventDialog's select sits in a plain `div` with `lng-field-label` (not a `.lng-field`); ShelfPanel and the other raw selects (SearchBox, EmojiPicker, ForumView, FollowButton, msg/Components) are not under a `.lng-field`.
The selector as I left it: `select` is out entirely. Nothing remains to happen. Note: the Invite selects already measured 40 / 44 in the base (the kit's `min-block-size` beat the old 36 px), so removing the rule changes no figure there.

## The select table (STACK-ROUND, base from scratch copies then edited files, one world; boxes w x h)
| select | 1024 base -> after | 375 touch base -> after | dialog block size 1024 / 375 | hit test 375 base -> after | keyboard |
|---|---|---|---|---|---|
| Notification "Mute for" | 360x36 -> 360x40 | 303x36 -> 303x44 | 662->666 / 782->782 (scrolls) | 10/1 -> 11/0 | ArrowDown 3600 -> 10800, back |
| Timeout "Duration" | 360x36 -> 360x40 | 303x36 -> 303x44 | 335.5->339.5 / 339.5->347.5 | 4/1 -> 5/0 | 3600 -> 86400, back |
| Ban "Delete message history" | 360x36 -> 360x40 | 303x36 -> 303x44 | 313.8->317.8 / 317.8->325.8 | 4/1 -> 5/0 | 0 -> 3600, back |
| Status "Clear after" | 360x36 -> 360x40 | 303x36 -> 303x44 | 284->288 / 288->296 | 4/2 -> 5/1 | never -> 30m, back |
| Event channel | 360x36 -> 360x40 | 303x36 -> 303x44 | 738->738 / 782->782 (scrolls) | 11/1 -> 12/0 | two voice channels: changed, back |
| Poll "Duration" | 520x36 -> 520x40 | 303x36 -> 303x44 | 559->563 / 563->571 | 3/9 -> 4/8 | 24 -> 72, back |
Widths are equal. At 1024 the hit test (points 21 px away, made for 44 px) is identical in count and status base and after for all six (0/11, 0/5, 0/5, 0/6, 4/8, 0/12); Dark equals Light. The spec's comparison (select not shorter, 44 at 375, not narrower, controls moved no more than the select growth above them plus the dialog's re-centring, no overlaps, no sideways scroll, hit test not worse) gave `problems: []` for all six dialogs in all three configs (Light 1024, Dark 1024, Light 375); `:focus-visible` matched after a key in every case. Remaining 375 fails (Status 1, Poll 8, ...) are in the base too (takers are other controls in the dialog; figures in `figures-after.json`; not chased). Nothing was saved: dialogs closed with Escape, the scrapbook switch put back in a `finally`.
- **Invite dialog** (`p101-lounge-shots.spec.ts` run unedited, same world): selects 520/254/254 x 40 at 1024, 303 / 145.5 x 44 at 375; **hit test at 375 8 pass / 0 fail**, same as base; 1024 0/8 same as base.
- **Gallery:** the spec's gallery run was taken with the kit ShelfPanel (flagged, above). ShelfPanel is now raw again, so the gallery is the base's by construction; that final state was not re-run (the base text needs no run, per the brief). The three filters measure 128 x 40 / 44 as in wave 101.

## Found, not mine
- **Create Event dialog, base too:** at 1024 the dialog body is wider than its box (the "Ends" date field runs past the right edge, the body scrolls sideways, the left of the radios is cut). `base-event-1024-lounge-light.png` and `event-1024-lounge-light.png` show the same. Proposed row: SEC-AUD-455 `[Lounge] Create Event dialog scrolls sideways at 1024 (Starts/Ends date fields)`. At 375 it is whole.
- `shell.css:2` `.lng-select` and `events.css:70` `.lng-select` are now unused (no `.tsx` names the class). Dead rules for whoever owns those sheets.

## Who imports each file (all lazy Lounge chunk, not first paint, no public page)
`NotificationDialog`, `MemberDialogs`, `CustomStatusDialog`, `CreateEventDialog`: `lounge/LoungeDialogs.tsx` (LoungeRoot chunk). `PollModal`: `lounge/Composer.tsx`. `lounge.css`: imported by the Lounge shell (lazy Lounge chunk); built file `app/lounge-BvsZOYpt.css`, 35 746 B (6 751 B gzip), not in first paint and not loaded by any public page.

## The web build (10:28 to 10:29, tree of my last commit; only my changes are in it)
`npx pnpm@10.12.1 --filter @sideroom/web build`: exit 0. `check-bundle-budget.mjs`: exit 0, first paint `app/app-vU1n0HAV.js` **218 432 B gzip, room 37 568 B** (before 218 419 B / 37 581 B: +13 B, my tree only). `check-entrance-isolation.mjs`: exit 0 ("every public surface is clean"). `check-staff-delivery.mjs`: exit 0. No budget miss. The kit's `NativeSelect` sheet (`_listbox_`) is now in its own chunk **`app/NativeSelect-254EnpQc.css`** (no other CSS file holds it; wave 101 had it in `Channels-*.css`): the commander's merged build may place it elsewhere again, search `dist/app/*.css` for `_listbox_`.

## STACK-ROUND
Slot: `status` showed `lounge 10:21` alone while it ran. 10:21 to 10:26: stack, seed (0 `FAIL` lines), p102 spec base (1.4 min), p101 spec base, files restored, p102 spec after, p101 spec after; stack stopped by PID tree, ports free, database dropped. Release 10:26, `status` free. (The `after` p102 run reported only the ShelfPanel problems above.) Build slot 10:28 to 10:29, released, `status` free.

## TYPECHECK
`bash cli-agents/tc.sh run lounge bash -c "cd .../p102-lounge/apps/web && npx tsc --noEmit -p ."` 09:39 to 09:40, exit 0 (after my last code edit except the ShelfPanel put-back, which restores the base's exact text, so not re-run). e2e `npx tsc --noEmit -p tsconfig.json` exit 0 (09:36); prettier --check on my web files and the spec: clean (after `--write`); eslint (web package's own) on the five `.tsx`: exit 0; `i18n-audit.mjs --max 0`: exit 0; `check-css-tokens.mjs`: exit 0; `check-doc-links.mjs`: exit 0 (before this file was written). Single vitest files, unedited: `p12FeatUi` 24, `p54SilentFailures`, `scrapbook` 20, `msgLogic` 22, `shellFlows`: all passed (before the put-back; ShelfPanel's text equals the base's).

## Shots: `visual-sweep-shots/p102-lounge` (opened: the 14 after shots of the six dialogs at 1024 Light and 375 Light plus shelf-1024, the Invite 1024 and 375 shots; not opened: the base- twins except `base-event-1024`, shelf-375, the Invite dark/focus pair, Dark shots (the spec records Dark figures, no Dark shot was asked for))
Seen: kit chevron at the inline end, kit fill and border, labels unchanged, no overlap, nothing cut except the base's Create Event 1024 overflow (above). `invite/` holds the p101 spec's shots (base- twins included); they show an invite left in the world by an earlier spec.

## The commander's command (from `e2e/`, stack with the sweep's world)
```
SR_E2E_BASE_URL=http://localhost:<vite> SR_P102_LOUNGE_ON=1 SR_P102_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-lounge \
npx playwright test tests/p102-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
After the merge there is no base: delete `figures-base.json` first. With ShelfPanel raw the spec's `shelf` scenario expects 4 selects (it passes the same way as the base).

## SEC-AUD-453 text
Wave 102 lounge: the Notification, Timeout, Ban, Custom status, Create Event and Create Poll dialogs use the kit's `NativeSelect` (40 px, 44 px on a phone); `.lng-field select` left `lounge.css` (D-967). `scrapbook/ShelfPanel.tsx` stays raw: the kit's wrapper wraps its "Use this shelf" button below the select (48 px).

## Proposed rows / sentences
- Inventory: SEC-AUD-455 Create Event dialog sideways overflow at 1024 (above).
- Changelog: "Six more Lounge dialogs' drop-downs (notifications, timeout, ban, custom status, new event, new poll) use the kit's look and a 44 px target on a phone."
- No threat-model row, no AGENTS.md sentence.

## Owner questions
- Should `ShelfPanel`'s row become a grid (select on its own line, button below) so the kit select can move? That is `scrapbook.css`, not in my row.

## Cleanup
Database `sr_p102_lounge_ui` dropped (checked), `p102-lounge-bin` deleted, stack stopped, no stash touched, scratch in the session scratchpad. `heavy.sh status` before reply: see final report.
