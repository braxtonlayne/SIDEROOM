# Wave 104 handoff: event (D-992)

Clock: started 14:13, finished 14:16.

## Done (commits on `fix/p104-event`)
- `D-992 CreateEventDialog.tsx (READY)`: `size="sm"` to `size="md"` (line 130).
- `D-992 events.css (ALL READY)`: the `.lng-event-times` track floor `16rem` to `15.875rem` (254 px) and its comment.

## REACH table
Open: the "Events" row at the top of the channel list (`ChannelSidebar.tsx:422`, or the guild menu `GuildRail.tsx:132`), then Create Event (`EventsDialog.tsx:77`), which opens `CreateEventDialog` through `LoungeDialogs.tsx:73-75`. Theme: the Lounge's three. All in `apps/web/src/lounge/events/CreateEventDialog.tsx` unless named. Styled by the kit's `Modal.module.css` (`.dialog`, `.md` 560 px, `.body` 16px 20px) and `lounge.css:1621` `.lng-form` (flex column, 16 px gap).

| control | line | label | sheet rule | moves? |
|---|---|---|---|---|
| Modal | 127-133 | "Create Event" + description | kit `.md` (was `.sm`) | width 402 to about 562 |
| kind radios (3 `label.lng-kind-opt`, `fieldset.lng-kind`) | 135-152 | Where / kind labels | `lounge.css:1673-1708` | full line, wider |
| channel select `#event-channel` (not external) | 161-180 | Channel | `shell.css`/`threads.css` `.lng-field-label`, kit NativeSelect | wider |
| location TextInput (external) | 153-159 | Location | kit TextInput | wider |
| Name TextInput | 181-187 | Name | kit TextInput | wider |
| Starts / Ends (`div.lng-event-times`) | 188-205 | Starts, Ends (optional) | `events.css:64` | side by side |
| Description TextArea | 206-211 | Description | kit TextArea | wider |
| cover field | 212 (`EventCoverField.tsx`, `events.css:102+`) | cover | `events.css` | wider |
| error `p.lng-failed` | 213-217 | alert | | wider |
| Cancel, Create buttons (`.lng-form-actions`) | 218-225 | Cancel, Create | `lounge.css:1626` | right edge moves with width |

Found by tests: `e2e/tests/p103-event-shots.spec.ts:198` (`.lng-event-times`; a p103 spec: expects stacked dates, so its stacked-dates expectation is stale by D-992, not edited). Unit tests that open the dialog: `lounge/__tests__/p13Events.test.tsx`, `p12ChBe.test.tsx`.

## The arithmetic
The 402 px base dialog is the `.sm` 400 px plus the 1 px border each side, body 400, content 360 (wave 103). For `md`: dialog 560 + 2 = 562, body 560 (`.body` has no border), less 20 px padding each side = **520 px content**. Two 16rem tracks and the 12 px gap = 524 px: do not fit by 4 px, so at 16rem the dates would stay stacked in the wider dialog. Wave 103 measured the Starts field 251 px and Ends 254 px (the wider one: text plus picker). The floor is lowered to 15.875rem = 254 px: two tracks and the gap are 520 px, exactly the content, and each track (1fr, so 254) holds the widest field whole. Not lower: a smaller floor would let a track be narrower than the Ends field in a dialog a few px short of 560 and push it past the edge. There is no slack, so any narrower content (zoom giving a fractional width, a dialog under 562 px) stacks the fields, which is the safe fallback (D-978). Assumes a 16 px root; the field's text scales with it. At 375 the track is `min(100%, ...)`, so one column as before; the dialog there is the screen's width already.

## Diff in words
Dialog 402 to about 562 px at 1024 and 1280; Starts and Ends share a line; the dialog is about one field row shorter. Markup, strings, order of fields unchanged. Nothing at 375 changes.

## Imports
`CreateEventDialog` is imported only by `lounge/LoungeDialogs.tsx:8` (the Lounge chunk). `events.css` by `CreateEventDialog.tsx` and `EventsDialog.tsx`. No first paint or public page. `.lng-event-times` is used by no other dialog (`git grep -n lng-event-times -- apps/web/src`: events.css and CreateEventDialog only). No kit file touched.

## For loungeshots
Measure at 1024 and 1280: body `scrollWidth == clientWidth`; both date fields 254 px wide on one line, each value whole with its picker inside the box; Ends right edge inside the body. If Ends is cut or the fields stack, cure (a) `size` back to `sm` or (b) a floor you name (the figure above is already the largest that fits, so (b) can only go lower, not below 14rem).

## TYPECHECK
`bash cli-agents/tc.sh run event bash -c "cd .../p104-event/apps/web && npx tsc --noEmit -p ."`: exit 0, 14:15.
Others (14:13 to 14:15): prettier --check on both files exit 0; `npx pnpm@10.12.1 exec eslint src/lounge/events/CreateEventDialog.tsx` (web package) exit 0; vitest `p13Events.test.tsx` 5 passed, `p12ChBe.test.tsx` 6 passed; `check-css-tokens.mjs` exit 0; `i18n-audit.mjs --max 0` TOTAL 0; `check-doc-links.mjs` ok.

## STACK-ROUND
none (code builder). Visual check: not mine; loungeshots'.

## PROCESSES
Stack-free: nothing started. Final listing at reply: see the reply (no line of mine).

## Changelog sentence
"The new-event dialog is wider, so the start and end dates sit side by side."

## Owner questions
None. Note: `e2e/tests/p103-event-shots.spec.ts` asserts the dates stacked in a 402 px dialog; stale by D-992.
