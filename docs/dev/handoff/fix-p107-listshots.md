# fix-p107-listshots (wave 107, shots builder of pod lounge B: lists + heads)

## Clock
Started 18:37. Spec committed 18:44, base shoot 18:58 to 19:01, after shoot 19:39 to 19:48 (queue waits of 10 to 20 minutes). Last job ended 19:48. No job queued or running.

## Done
- `e2e/tests/p107-list-shots.spec.ts` (4 parts). Part 1 phone: Home list, switcher, Friends and 3 tabs, server list, member sheet. Part 2 phone: channel, header strip, phone search sheet, threads popover, forum, new post, DM, DM strip. Part 3 desktop Lounge Light 1024 (boxes, box-reach hit tests of the left pane, member list and Home list, the five fields, 1280 boxes). Part 4 Lounge Dark 1024 (fields, boxes).
- Merged `fix/p107-heads` (2c5bdc1d) and `fix/p107-lists` (7b6c26c1, with Add a ring); built `0210`.
- The last spec edits (Friends header and ring compare, new-post settle wait, textarea has no placeholder) are in the final commit with this handoff. They changed only the spec; the after jobs 0220 to 0223 ran with them.

## Not done
- No base re-run after the spec's settle wait on the new-post dialog: the base figure of `.lng-post-textarea` was read mid-open animation (see problem lines).
- The tools Drawer's switcher is not cured (tools.css, fault 1).

## Counts (hit test, 375 touch, default reach)
| part | pages | fails base | fails after |
|---|---|---|---|
| 1 | Home list, switcher, Friends and tabs, server list, member sheet | 45 | 6 |
| 2 | channel, DM, forum, popovers | 44 | 42 |
| 3 | 1024 light, box reach | 0 | 0 |
Part 1 left: "1 message request is waiting" link (lng-link-btn, 217x20, taker p "No conversations yet."), switcher tabs Go to / Tools (sr-tl-tab 30 px, tools), Friends notice Dismiss x3 (24 px, kitbits). None is a lists or heads control. Part 2 left (all failing in base too, not lists/heads): message avatars, names, mentions, links, reactions, the composer buttons (compose), the Threads popover tabs and search (heads did not cure them: 29 px search, 32 px tabs), "Edit Channel".

## Figures against the tasks' lists
lists, 375:
- Channel rows 32 -> 44, Home items 42 -> 44, member rows 42 -> 44, find box 28 -> 44, category heads 16 -> 44: all within the limits (0 spec problem lines). Rows' widths unchanged.
- User panel box, its buttons, id and avatar, the server rail and tab bar: identical to base. Mute, Deafen, Settings, both chevrons, avatar and id all pass (base: 12 fails).
- Category + buttons and Create DM +: pass. Ring avatar `.rng-btn` (A.1, accepted): passes, box 40x40 unchanged, takes no neighbour's point. "Add a ring" (`.rng-tray-item > .lng-icon-btn`): passes in the after run (rule 7b6c26c1 is in build 0210).
- Friends header: Back and New Group DM pass; header 85 -> 101 px (+16), tabs y 57 -> 73. Inside the task's allowance (about 13 to 16).
- The ring moves down 20 px in the Home list (rows above it grew: find +16, three items +2 each): its size is unchanged.
- 1024 and 1280: every box of every named element equal to base (0 problem lines for rows, header, user panel). The hit tests at box reach of the left pane, the member list and the Home list: 0 fails after, 0 in base.
heads:
- Header search: ring on the `.lng-search` pill, solid 2px `--ui-focus` (rgb(0,104,201) light, rgb(0,168,252) dark), offset 1, folded (members open) and open, both themes; ring contrast 3.0 or more; placeholder faint at opacity 1 and 4.5 or more; the pill still has no border; box unchanged.
- `.lng-forum-search`, `.lng-threads-search`: rest border equals the 65 mix, hover equals faint, ring solid 2px focus offset 1 reached by Tab, placeholder faint; rest border against outside and field ground 3.0 or more (both themes). Boxes unchanged.
- `.lng-post-textarea`: border, hover, ring as above. Placeholder: the textarea has no placeholder attribute (NewPostDialog.tsx:158), so the placeholder rule cannot be read there (info).
- Phone search sheet: form box unchanged (44 px), the input is now 44 px high (was 20) and fills the form; passes. Its field now shows the 2px ring when the sheet opens focused.
- DM identity: base failed (191x28, taker header); after passes; box 28 px unchanged.
- Forum search 44 px on a phone (already). Thread search 29 px and the post textarea at 375: not cured by heads ("may change").

## FAULTS
1. **tools (tools.css `.sr-tl-drawer .lng-switcher-input`), cure (b):** inside the Drawer the switcher's ring offset is 2px not 1, its rest border is rgb(201,204,209) light / rgb(63,65,71) dark (not the 65 mix), contrast against the ground 1.6 light / 1.4 dark (3.0 required), and hover does not change it. heads flagged it too. It may move only the Drawer field's border-color, hover and ring offset (no box). The switcher does open focused with :focus-visible true and the ring shows (switcher-1024-dark.png): the ring is not missing.
2. Not lists or heads: `.lng-reaction` buttons (msgs' rule) fail at 375 with taker `button.lng-thread-chip` (a control) in base and after. The msgs pod's shots builder should look.
3. Not owned: the Home list's "1 message request is waiting" link button (`.lng-link-btn` in `.lng-pad`, 20 px) is a standalone text button, not exempt: proposed row for wave 108.

## Problem lines of the last runs and what they were
- 0220/0223 (parts 3, 4): `MOVED at desktop width: new-post|.lng-post-textarea` base 256.2,353.5 511.6x135.8, after 252,353 520x138. The base was read while the dialog was still scaling open (511.6/520 = 0.984 and 135.8/138 = 0.984); the after read waits 900 ms. Not a fault; not re-run on the base.
- 0212/0213 first after runs: `.rng-btn` and Friends header "must not change" lines were the spec being too strict (the ring moves with the rows above it; the Friends header is cured). Spec fixed, parts 1 and 2 re-run: 0 problem lines in part 1, 2 reaction lines in part 2 (fault 2).
- The "1 px short" check: no failing points on the DM identity, user panel, category + or Create DM + areas; none found short in lists or heads.

## JOBS (wait s / ran s)
base: 0177 p1 770/38 rc0; 0178 p2 820/48 rc0; 0179 p3 871/61 rc0; 0180 p4 945/63 rc0.
0188 build 16/25 rc0; 0199 shoot after p1: rc 3 stale build (my spec commits moved HEAD); 0200 to 0202 cancelled; 0210 build 594/17 rc0.
after: 0212 p1 583/34; 0213 p2 624/41; 0214 p3 676/54; 0215 p4 735/42; re-runs with the spec fixes: 0220 p3 129/49; 0221 p1 186/32; 0222 p2 228/39; 0223 p4 349/44 (all rc0).
Commander's command per part: `bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_LIST_ON=1 --env SR_P107_LIST_PHASE=after --env SR_P107_LIST_PART=<1|2|3|4> --env SR_P107_LIST_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-listshots -- tests/p107-list-shots.spec.ts` (figures merge into `figures-after.json` by part; base in `figures-base.json`).
Lesson: any commit after a build (even spec-only) makes `shoot after` fail with stale build; uncommitted e2e edits are fine.

## Shots looked at (shots folder: visual-sweep-shots/p107-listshots)
- server-list-375 (base twin): rows and category heads taller, user panel and tab bar identical, text and icons unchanged.
- home-list-375: Find box, Friends and Message Requests rows taller; ring avatar round at 40 px with Add a ring + below; DM head +; user panel unchanged.
- friends-375: header taller (Back, title, New Group DM); tabs and body lower by 16; nothing crowded.
- member-sheet-375: member rows taller; status dots and names as before.
- search-sheet-375 (base twin): form same size, input fills it, ring visible around the pill (new, D-1023), chips unchanged.
- dm-375: header as before to the eye (the tap area is invisible).
- channel-1024-light and channel-members-1024-light: ring on the search pill open and folded; header and tools in the same place as base. The search chips panel is open in these shots because Tab traversal passes through the header search.
- forum-1024-light: forum search shows the ring; same artefact panel.
- threads-1024-light: Threads search shows the ring and the new border.
- switcher-1024-dark: field focused with the ring (offset 2, the tools.css copy).
- new-post-1024-dark: textarea ring on the dark dialog; the title field beside it keeps the kit look.
- home-list-1024-dark: no visible change, user panel and rows as base.
Not opened one by one (figures only): friends-tab-*-375, channel-375, channel-more-375, dm-more-375, forum-375, new-post-375, threads-375, switcher-375, the other 1024 pairs.

## FINDERS
No app file changed: not applicable. The spec imports no `/src` module.

## Checks
prettier --write on the spec and e2e `tsc --noEmit` (from e2e) after the last spec edit: exit 0, no output. eslint on the spec: not run.

## Who imports / chunk
None (spec only).

## Closing text, proposed rows, changelog
Closing text: the phone lists, the user panel, the Friends header and the ring tray are 44 px targets; the header search, the forum, thread and new-post fields and the phone search field follow the field contract and show focus. Proposed rows: tools.css switcher (fault 1); the Home list's message-request link button (fault 3); the Threads popover tabs and search and the new-post textarea on a phone (uncured). Changelog: the channel list, Home list, member list, user panel and Friends header are 44 px tap targets on a phone, and the header, forum, thread and new-post fields show a clear focus ring and border.

## Owner questions
none.

## PROCESSES
strays: none found. No job of mine queued or running.
