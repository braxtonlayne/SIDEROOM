# Wave 102, mediashots (shots builder, pod media): SEC-AUD-453, D-958

Branch `fix/p102-mediashots`: the pod's three branches merged (watch, media, chat), spec `e2e/tests/p102-media-shots.spec.ts`. Shots and figures: `visual-sweep-shots/p102-media` (round 2's: `figures-base.json`, `figures-after.json`, `compare.json`, `reach-*.json`, `base-*.png`, `<scenario>-<config>.png`, `cmp-*.png` = base | after).

## FAULTS (code builders: that select goes back to raw, the base's text)
1. **watch, `apps/web/src/watchparty/ControlBar.tsx`, Speed** (label.wp-inline, inline-flex): 72 -> 58 px wide (1024 shipped, 375 shipped, 1024 light); height 31 -> 40 (44 at 375) is fine; the bar's row re-centres (Play, -10, +10, Position up 2 px, the select 6.5 px). Put back to raw.
2. **watch, `watchparty/TitlePicker.tsx`, Season** (label.wp-inline in `.wp-picker-head`): **46 -> 18 px wide** at 1024 and 375; the chevron sits over the word "Season" and the value "1" is gone (cmp-watch-picker-*). Put back to raw.
3. **media, `music/panel/TrackList.tsx`, "Add {title} to a playlist"** (in the flex-wrap `.sr-mlib-more`): 214 x 29 -> 506 x 40 at 1024, 321 x 44 at 375; it leaves the button row for a line of its own and every row below moves 42 px where the height difference allows 11 (7 controls, both Lounge themes at 1024). Put back to raw.

Builders with entries: **watch** (ControlBar, TitlePicker), **media** (TrackList). **chat has none.**
No fault: `PeoplePanel.tsx` x2 (335 wide, 31 -> 40 / 44, chevron clear of the text); `StartPartyDialog.tsx` (520 x 40; at 375 the base select ran 27 px past the dialog's edge and the kit's fits: 303 against 330 is the base overflow gone, not a narrowing); `FollowButton.tsx` (360 x 36 -> 40, label +4; at 375 reached through "More actions"); `SearchBox.tsx` (103 x 28 -> 153 x 40; `msg.css:779` does not win over the kit's height; the bar grows 12 px). The reverted selects are "put back, not re-shot: the base's text".

## STACK-ROUND
- Round 1: 09:50 to 09:56, slot 2 (`status` read "pageshots 09:44 / mediashots 09:50"). Base = the 7 pod app files put back to `b308d5e2` from git, then the merged text restored (git status clean after); after = merged. Spec exit 1 (assertions: the faults above, and the search keyboard note below).
- Round 2: 10:32 to 10:37, `status` read "mediashots 10:32". Same method; reached what round 1 missed (Season through the Queue tab, Follow at 375 through "More actions", the 375 theatre base after a rejoin). Released 10:37; ports 8799/5799/9499 free; database `sr_p102_mediashots_ui` dropped; `p102-mediashots-bin` deleted.
- Final `heavy.sh status` before the last commit: free, no hold in my name.

## TYPECHECK
`bash cli-agents/tc.sh run mediashots bash -c "cd .../p102-mediashots/apps/web && npx tsc --noEmit -p ."` 10:37 to 10:38, exit 0 (merged tree). prettier --check on the spec: clean; e2e `npx tsc --noEmit -p tsconfig.json`: exit 0; eslint (the web package's, on the spec): exit 0; all at 10:37, after the last edit.

## The pod's 15 selects
| file | select | reached | base -> after (1024 / 375) | verdict |
|---|---|---|---|---|
| watchparty/PeoplePanel.tsx | Who can play, pause and seek | yes | 335x31 -> 335x40 / 351x31 -> 351x44 | pass |
| watchparty/PeoplePanel.tsx | Who can add to the queue | yes | same | pass |
| watchparty/ControlBar.tsx | Speed | yes | 72x31 -> 58x40 / 72x31 -> 58x44 | FAULT 1 |
| watchparty/StartPartyDialog.tsx | Where to watch | yes | 520x31 -> 520x40 / 330x31 -> 303x44 (base overflowed) | pass |
| watchparty/TitlePicker.tsx | Season | yes (Queue tab, Add to queue, a series) | 46x31 -> 18x40 / 46x31 -> 18x44 | FAULT 2 |
| music/panel/TrackList.tsx | Add to playlist | yes (faked `GET /music/library`, own playlist, "..." on a row) | 214x29 -> 506x40 / 214x29 -> 321x44 | FAULT 3 |
| lounge/header/FollowButton.tsx | Post announcements in | yes | 360x36 -> 360x40 / 44 | pass |
| lounge/SearchBox.tsx | Sort by | 1024 yes; 375 not reached (the header search box is not shown on a phone) | 103x28 -> 153x40 | pass |
| music/panel/sections.tsx x2, study/StudyDialogs.tsx, cinema/tv/RemindersCard.tsx, player/skins/backdrop/StackSkin.tsx, lounge/forum/ForumView.tsx, lounge/components/EmojiPicker.tsx | left raw by media / chat | not run: `SR_P102_MEDIA_ALL=1` adds their scenarios, which were never run (their ways in are untested) | | files unchanged |

Hit test (shared helper, `openCollapsed` then `hitTestAll`): no row worse than the base in any scenario; counts equal in both phases on desktop; at 375 the theatre went from 7 pass / 16 fail / 1 not-reached (round 1 base: the page was the "not in this party" screen, a spec error fixed by a rejoin) to 11 / 13 in round 1's after. Document `scrollWidth` never above `clientWidth`; no overlaps. `data-theme` read back: lounge scenarios lounge-dark / lounge-light; cinema scenarios `sideroom` and, in the light config, `lounge-light` (the theatre itself stays dark: `.wp-root` paints its own ground).
Keyboard (focus-visible true, ArrowDown changes, ArrowUp restores, set back): who-controls (host, controllers, host; one PATCH), Speed (1, 1.25, 1), Where to watch (private, a channel, private), Follow (none, a channel), TrackList add-to-playlist (one POST seen, faked; the value stays empty). The focus ring is 1 px with offset after the move against 2 px before (the kit's).

## Shots (all opened, composite = base | after)
20 composites, round 2's, each opened: cmp-watch-theatre (1024 shipped, 375 shipped, 1024 light): People settings, selects taller, chevron at the end, 375 base now the same page; cmp-watch-speed (3): Speed narrower, chevron clear of "1x"; cmp-watch-picker (3): Season collapsed to a sliver with the chevron on the label (fault 2); cmp-watch-start (3): 40 / 44 tall select, at 375 the base overflow gone; cmp-music-tracks (1024 dark and light, 375 light): the select on its own full-width line under the buttons (fault 3); cmp-lounge-follow (1024 dark, light, 375 light): 40 / 44 select, dialog +4 px; cmp-lounge-search (1024 dark, light): "Newest" select 153 x 40, bar +12 px, popover tidy.

## Spec notes and base faults (proposed rows)
- Lounge search Sort keyboard: ArrowDown changes new to old but ArrowUp leaves "old" (the same in base and after, 2.2 s settle): a behaviour of the base (the sort re-runs the search and re-renders the select). The spec reports it as a problem in both phases. Proposed row: "Lounge search: ArrowUp on the Sort select does not return to Newest".
- `POST /watch-parties {}` answers 400 ("Give a title or a Live TV channel"): no empty party; the picker is reached through the Queue tab.
- Data made and removed in a `finally`: a movie, a series, a party, a TV channel, an announcement and a forum channel, an own playlist; the storefront switches on and off. Faked in the browser: `GET /music/library`, the add-to-playlist POST.

## Commander's command (from e2e/, stack API 8799, Vite 5799, metrics 9499, sweep world seeded, DATABASE_URL and SR_CLI_BIN set)
`SR_E2E_BASE_URL=http://localhost:5799 SR_P102_MEDIA_ON=1 SR_P102_MEDIA_PHASE=base SR_P102_MEDIA_OUT=<folder> npx playwright test tests/p102-media-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` on the unmerged tree, then the same with `SR_P102_MEDIA_PHASE=after` on the merged tree (after the reverts Speed, Season and the track select are raw in both: expect equal figures). With no base, delete `figures-base.json` first.

## Owner questions
None. Suggestion: the kit's wrapper gets `inline-size:auto` inside inline-flex labels (frees Speed, Season and three of media's) and a flex-wrap option (TrackList): a kit change, not this wave's.
