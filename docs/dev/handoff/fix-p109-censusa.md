# fix-p109-censusa (wave 109, census: SEC-AUD-537 pages 1 to 5 and 8, part of 529)

## Clock times
Started 21:38. Spec written and first job 21:42. Last job ended about 22:10. Report 22:12. Handoff 22:14.

## Done (commits on fix/p109-censusa)
- `e2e/tests/p109-censusa-shots.spec.ts` (new): measure-only census at 375 touch (default reach) and 1024 (`reach: 'box'`), Lounge Light. Parts 1 (Friends All and Pending, a row's More menu, New Group DM dialog, the group DM: page, header strip, identity/member panel, add friends, message menu), 2 (extras channel: 14 reactions with the line analysis, add-reaction picker, spoiler before and after reveal, message menu, forward dialog), 3 (an injected image and file by `page.route`, the sweep's way; the image viewer; the channel search). Also the message hover toolbar after a tap (375) or hover (1024), and the field contract of every field it meets. The commit subjects say `D-1030`: that number is a placeholder of mine (the task gave none); the commander renumbers.
- Report: `C:/Users/braxt/Desktop/SIDEROOM_wt/p109-census-a-report.md`. Figures: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusa/figures-part1.json`, `figures-part2.json`, `figures-part3.json`.
- No app file changed. No `build` job.

## Not done (why)
- The group DM "Member List" button path at 375 (the tap timed out); the same panel is measured through the identity button.
- Pages 6, 7, 9 are censusb's. Scrapbook and voice channel skipped by the task. See the report, section 6.

## REACH table
Not applicable (census builder; no app change).

## FINDERS (rule 13)
No app file changed, so none. `git grep -n "p109-censusa"`: only this spec and this handoff.

## JOBS (class shoot, mode base; seconds)
| id | waited | ran | rc | note |
|---|---|---|---|---|
| 0292 | 39 | 157 | 0 | part 1; Friends link not found (badge); probe tokens read on body |
| 0293 | 11 | 59 | 0 | part 2; spoiler reveal failed |
| 0298 | 117 | 45 | 0 | part 3; the 1 x 1 test PNG made 1 x 1 tiles (spec error, not a fault) |
| 0302 | 38 | 43 | 0 | part 3 with the sweep's real PNG |
| 0304 | 155 | 127 | 0 | part 1; Friends link still not found; member pane off screen |
| 0308 | 118 | 61 | 0 | part 2; spoiler reveal now by tap with a fallback |
| 0311 | 96 | 105 | 0 | part 1; Pending tab is named "Pending 1" (prefix match fixed) |
| 0313 | 32 | 75 | 0 | part 2, final (with the toolbar states) |
| 0315 | 56 | 83 | 0 | part 1, final |
| 0317 | 25 | 46 | 0 | part 3, final (with the toolbar state) |

The figures files are from the final runs 0315, 0313, 0317. The earlier jobs ended rc 0 but their spec problems were mine, each fixed in the spec.

## Figures
Measure only; no "must not change" list. Totals: 375: 217 pass, 73 fail, 100 skipped, 0 not-reached; 1024: 306 pass, 2 fail (the spoiler span), 93 skipped, 0 not-reached. Wrapped reactions: the areas of the lines touch (gap 0.0 px), no overlap, all pills pass; message 201 px at 375. Details in the report.

## Shots (opened after the last job; `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusa`, `base-` prefix)
375:
- friends-all: Friends, All tab, one row (Marlow, Message, More); the tab strip is scrolled so "Online" is half cut (the strip scrolls, the page does not).
- friends-all-row-more-actions-menu: the row's menu open (Share my Media Shelf highlighted, Remove Friend, Ignore, Block).
- friends-pending: Pending, "Incoming 1", Tess with a check and a cross.
- new-group-dm-dialog: "Select Friends" dialog, search field, Marlow checkbox row.
- group-dm: the three messages and a composer whose placeholder wraps to three lines (78 px).
- group-dm-header-more-strip: the header's second strip of icons open under the header.
- group-dm-identity-group-panel-: the members panel (3 members, Add friends, Group settings, Create invite link).
- group-dm-add-friends-to-dm: "Add Friends to DM" dialog, no friends to add, invite link button.
- group-dm-message-action-menu: the long-press menu (Add Reaction to Report Message).
- group-dm-toolbar: after a tap, the hover toolbar (7 buttons) over Tess's message, covering the line above.
- extras-channel: heading, then 15 pills on three lines and the spoiler message right under them.
- reactions: the same, pills in three lines with wide gaps.
- reactions-add-reaction-picker: the emoji picker open over the message (search field, food, symbols).
- reactions-toolbar: the toolbar (10 buttons) over the date divider and the head of the reactions message.
- spoiler-before: the spoiler as two grey bars.
- spoiler-after: revealed text with the toolbar over the third pill line and the message's first line.
- spoiler-toolbar: the toolbar over the third pill line, the spoiler still hidden.
- extras-channel-message-action-menu: long-press menu (Edit Message, Delete Message present).
- forward-dialog: "Forward To", preview, two half-width fields with cut placeholders, eight checkbox rows.
- attachments: Juniper's message with the image (180 px) and the file card (name ellipsized, download icon).
- attachments-image-viewer: the viewer with the image and a crowded footer ("1 / 1" stacked, links wrapped).
- attachments-message-action-menu: the long-press menu over the attachment message.
- attachments-toolbar: the tap toolbar over the message's head.
- channel-search-phone-sheet: "Search" sheet with the field and the filter chips.

1024:
- friends-all, friends-all-row-more-actions-menu, friends-pending: the same pages in the wide layout, with the right column "Active now".
- new-group-dm-dialog: the same dialog, centred.
- group-dm: three-column layout, the member panel at the right, the title cut to "Sweep...".
- group-dm-header-more-strip: no strip at this width; the header search's filter panel is open (probe artefact, see the report).
- group-dm-identity-group-panel-, group-dm-member-pane: the panel as in the page shot.
- group-dm-add-friends-to-dm, group-dm-message-action-menu: dialog and menu as at 375.
- group-dm-toolbar: the 6-button toolbar over Tess's message.
- extras-channel: three pill lines with a 4 px gap, then the spoiler message.
- reactions: as above, the header search's filter panel open (artefact).
- reactions-add-reaction-picker: picker open from the add pill.
- reactions-toolbar: the toolbar over the reactions message head.
- spoiler-before, spoiler-after, spoiler-toolbar: bars hidden; revealed with the toolbar and a "Forward" tooltip over it; the toolbar over the line above.
- extras-channel-message-action-menu: context menu from a right click.
- forward-dialog: as at 375, fields half width, placeholders cut.
- attachments, attachments-image-viewer: image and file card; viewer with the footer in one row.
- attachments-message-action-menu: menu over the message with the toolbar behind.
- channel-search-inline-box-panel: the filter chips panel under the header search.

## Who imports my files
`p109-censusa-shots.spec.ts`: Playwright only. No app file, no lazy chunk.

## Checks
- `npx prettier --check e2e/tests/p109-censusa-shots.spec.ts`: pass (22:12, exit 0).
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (run after the last spec edit, and before each submit).
- vitest, eslint, web tsc, i18n audit, doc-link check, check-css-tokens: left to the commander / not run (no sheet touched).

## Closing text for SEC-AUD-537 (pages 1 to 5 and 8) and proposed new rows
Closing: "Measured in wave 109 (censusa): 73 fails at 375 on the new Lounge surfaces (Friends row buttons, message author names, the message toolbar after a tap, the forward dialog's checkboxes, the file card and the image viewer's bar) and one at 1024 (the inline text spoiler); the wrapped reactions' areas touch without overlapping (gap 0.0 px). Report `p109-census-a-report.md`."

Proposed new rows (faults the base has):
1. [UX][Medium] Phone: the message hover toolbar is shown by a tap (sticky hover) over the message above and over its own first line; its 28 px buttons fail the hit test and a tap on a spoiler in that line is taken by it (Create Thread at 375, Forward at 1024). Takers: reaction pills, text, the toolbar's own buttons. Cure: not on touch (default), or inside the message plus A.1.
2. [UX][Low] Forward dialog: two `.lng-emoji-search` fields 185 px wide in a full-width dialog, placeholders cut; 11 checkboxes 13 px (A.2 on `.lng-forward-row`).
3. [UX][Low] File card: the download icon is squeezed to 20 px wide by a long file name (needs `flex: none`).
4. [UX][Low] Image viewer footer at 375: six items in one row, labels wrap ("1 / 1" stacked); links 20 to 40 px high (A.3).
5. [UX][Low] Composer at 375: the placeholder wraps to three lines (78 px) and its focus ring is 1 px inset, not the field contract's 2 px (the wrapper draws the box, lounge.css:1400).

## Changelog sentence
Measured the Friends, group DM, reactions, spoiler, forward-dialog and attachment surfaces for the 44 px target and the field contract; nothing changed.

## Owner questions (to Fable; default taken)
Should a tap on a message show the hover toolbar on a phone? Default (Discord-like): no, the long-press menu has every action.

## PROCESSES
`strays.sh p109-censusa`: strays: none found (22:13). No job of mine is queued or running.
