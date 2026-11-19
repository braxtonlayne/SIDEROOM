# fix/p107-loungeshots (wave 107, pod lounge A shots builder)

Clock: started 18:37; spec and base 18:40-19:05; merged compose + msgs 18:43; round 1 after 19:29; FAULTS sent ~19:31; cures merged 19:35 (compose 82027deb, msgs 3e99b548); build 0218; round 2 after ended 19:52; reply ~19:56.

## Done
- `e2e/tests/p107-lounge-shots.spec.ts` (new). No app file changed. Parts: 1 = channel (page, emoji picker, GIF/Sticker/Emoji tabs, inline edit on its own room `p107lounge-room-<stamp>`, thread) at 375 touch and 1024 Light (fields also 1024 Dark); 2 = Home list (first, while the request waits), forum post (forum `p107lounge-forum-<stamp>`), DM (accepts the world's request), inbox + Mentions/Unreads tabs. Both made things removed in a `finally`.
- Merged fix/p107-compose and fix/p107-msgs (round 1 at ab736d5c/a1ddf0ba, cures 82027deb/3e99b548).

## Not done
- No Lounge Dark shots (Dark is read for the field figures only: composer, emoji search, inline edit; no png). No wrapped-reactions row exists in the seeded world, so the +16 px per wrapped line figure is from the CSS, not measured (round 1 had row-gap 20; now removed).
- MessageDialogs' two `.lng-emoji-search` inputs (forward / search dialogs) are not reached by the spec: they move with the class (padding, border, 44 px on a phone), unmeasured.

## FAULTS (all sent to the commander at ~19:31) and how each ended
1. compose, `.lng-emoji-search` width 185 -> 187 (all widths): cured by padding 0 7px; round 2: 185x32 at 1024 (equal), 185x44 at 375. Closed.
2. msgs, reaction pill ::after -10 on a 1px border (42 px): -11 now. Round 2: pills STILL fail at 375, point below, taker `div` "Rowan (owner) started a thread" (the next row paints over the tap area; above passes). Open: wave 108 row = bottom margin on `.lng-reactions` (or a z-index on the area).
3. msgs, row-gap 20 over the 12 px cap: removed; message with one reactions line +4 px (121.9 -> 125.9). Wrapped lines: tap areas overlap, "accepted" by msgs.
4. msgs, "Edit Channel" (116x20): A.1 did not fit (date divider painted over); now A.2: passes at 375 (and "1 message request is waiting" 217x20 -> 217x44, passes; 1024 box 216x40 unchanged).
- Proposed new rows (fail in base and after, not named in any task): `.lng-link-btn` "cancel" 38x17 and "save" 26x17 in the inline edit (taker the textarea above, the row below), "Jump to original" 122x22 in the thread (taker the quoted text above).
- Notes, not faults: emoji-search rest border against its field in Light is 2.76 (the D-1018 65 mix; the contract fixes the number). The inline edit draws no border at rest (so no rest/hover border to check) and its `::placeholder` still reads the text colour (rgb 49,51,56 Light; the `.lng-textarea::placeholder` rule did not reach it): never visible unless the edit is emptied. Its 2 px ring is right. Composer: 1 px ring `--ui-focus` offset -1 and faint placeholder pass; emoji search 2 px ring offset 1, hover = `--ui-text-faint`, placeholder faint pass.

## Figures (375 touch, Light; base -> after, jobs 0163/0182/0183 -> 0226/0227)
| area | base fail | after fail |
|---|---|---|
| named controls part 1 (338 rows) | 169 | 5 (2 pills, cancel, save, Jump to original) |
| named controls part 2 (66 rows) | 32 | 0 |
| all rows channel / emoji picker / GIF / Sticker / Emoji tab / inline edit / thread | 29 / 61 / 3 / 4 / 64 / 11 / 8 | 17 / 0 / 0 / 0 / 0 / 5 / 2 |
| forum post / DM / inbox (3 tabs) / Home list | 16 / 10 / 6,6,3 / 14 | 10 / 3 / 0,0,0 / 13 |
The rest are accepted names, mentions, spoilers, links, embed titles (channel 7 names, 1 mention, 1 spoiler, 3 links, 1 embed title; thread 1 name; DM 2 names; forum post 1 name; all fail, counted not listed) and other controls outside the pod's lists (Home list: rail, nav, user panel: lists pod).
- 1024 (box reach): no box moved, no new hit failure; 2 failing rows in the channel base and after alike (accepted links).
- 375 growth: tabs bar 40 -> 52, tabs 32 -> 44, emoji search 32 -> 44, inbox filter rows 20 -> 44 (inbox filters 88 -> 144), picker outer size unchanged, composer unchanged, message with reactions +4. No sideways scroll on any page (scrollWidth = clientWidth in every shot).
- Takes another control's point: none (no row passed in base and failed after; no centre taken).

## Shots (opened after the jobs; base twin beside)
channel-375: pill row 4 px lower, rest as base. emoji-picker-375 / picker-tab-emoji-375: search 44 tall with border, 44 cells, 44 category column, picker same size. gif-picker-375: tabs taller, empty state. picker-tab-sticker-375: search 44. inbox-375: tabs 44, filters 44 rows. thread-375, dm-375, forum-post-375: composer buttons as base, no crowding. inline-edit-375: 2 px ring, Edit Channel row taller. home-list-375: request link 44 tall, no crowding. channel-1024-light: composer 1 px ring visible, rest as base. emoji-picker-1024-light: search ring 2 px, same size as base. inline-edit-1024-light: ring on the textarea. Other 1024 shots (gif, sticker, emoji tab, thread, dm, forum, inbox, home): the figures equal base; not individually viewed.

## JOBS (class, waited, ran, rc)
0163 shoot base p1 (old spec), 0 s, 136 s, 0; 0182 base p1 (scoped), 985 s, 109 s, 0; 0183 base p2, 1105 s, 59 s, 0; 0164 cancelled; 0173 build cancelled (commit after submit); 0184 build 0; 0203 after p1 (round 1), 1409 s, 80 s, 0; 0204 after p2, 1497 s, 45 s, 0; 0218 build (cures), 538 s, 16 s, 0; 0219 base p2 (Home list first), 561 s, 50 s, 0; 0226 after p1, 412 s, 78 s, 0; 0227 after p2, 499 s, 46 s, 0.
Commands (whole): bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_LOUNGE_ON=1 --env SR_P107_LOUNGE_PHASE=after --env SR_P107_LOUNGE_PART=<1|2> --env SR_P107_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-loungeshots -- tests/p107-lounge-shots.spec.ts

## FINDERS
Spec only; no app file changed: none. For the pod: `git grep` of the classes is in compose's and msgs' handoffs.

## Checks
e2e tsc (`npx tsc --noEmit -p tsconfig.json` from e2e) exit 0, 19:31 (after the last spec edit); prettier on the spec run. eslint on the spec: not run. Round 1 problem lines are all above; round 2 problem lines: inline-edit no border/placeholder (notes), emoji-search border ratio 2.76 (note), pills (open), cancel/save/Jump (new rows), forum post name twin (stamp in the name, no real fault).

## Proposed text / changelog / owner questions
Changelog: the Lounge's composer buttons, emoji picker, avatars, link buttons, picker and inbox tabs and inbox filters have 44 px targets on a phone; the composer and emoji search show focus. Owner questions: none (Discord-like default: 44 px on touch).

## PROCESSES
strays: none found
