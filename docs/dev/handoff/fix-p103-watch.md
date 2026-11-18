# Wave 103, watch (code builder, pod media): SEC-AUD-462, 464, 465; D-972

Started 11:56; ALL READY 11:58; finished 12:02. kit merged (`fit` in base) at 11:57.

## REACH table (for mediashots)
Rule reached: `watchparty.css:412-419` `.wp-field select, .wp-inline select` (element selector: background, color, border, radius, `padding: 6px 8px`). Area theme: Cinema (`sectionThemes.cinema`, shipped `sideroom`); the Start dialog opened from the Lounge call bar is under the Lounge theme. Way in (as `e2e/tests/p102-media-shots.spec.ts`): a movie (admin API, fake `asset_id`) and a party made through the API (`POST /watch-parties {title_id}`; `{}` answers 400).

| # | File:line (base) | Label / id | Page and how to open | Sheet rule now | Now | `fit` after | Found by |
|---|---|---|---|---|---|---|---|
| 1 | `ControlBar.tsx:131` | aria-label "Playback speed", `disabled={!can}`, `title={why}`; in `label.wp-inline` (inline-flex, `watchparty.css:245`) | `/cinema/party/<id>`, control bar, only when `!live`; host | `.wp-inline select` | now kit | `content` | no test, no e2e |
| 2 | `PeoplePanel.tsx:151` | "Who can play, pause and seek"; `label.wp-field` (grid) | party, tab People, host | `.wp-field select` | kit | fill | `theatre.test.tsx:302` by label |
| 3 | `PeoplePanel.tsx:164` | "Who can add to the queue" | same tab, under #2 | same | kit | fill | none |
| 4 | `StartPartyDialog.tsx:211` | aria-label "Where to watch"; `label.wp-field`; has optgroups | `/cinema/t/<titleId>`, "Watch together", dialog "Start party"; also Lounge call bar | same | kit | fill | `start.test.tsx:129` by role |
| 5 | `TitlePicker.tsx:182` | aria-label "Season"; visible span in `label.wp-inline` (inline-flex) in `.wp-picker-head` | picker episode step: Queue tab, theatre with no media, or Start dialog without a title; needs a published series | `.wp-inline select` | now kit | `content` | `start.test.tsx` "picks an episode" by role |

Dependants of the rule: exactly these five (`git grep wp-field|wp-inline` finds no other file, no e2e or test class use). `.wp-field > input` (link field, StartPartyDialog:189) is not a select and is not touched.

## Who imports each file
- `ControlBar` <- `Theatre.tsx` <- `WatchPartyPage` <- `PartyRoute`, lazy route chunk (`watchparty/routes.ts`). `TitlePicker` <- `PickMedia`, `QueuePanel` (Theatre) and `StartPartyDialog`, lazy everywhere. Not first paint, not public.
- `watchparty.css` is imported by `PartyMessageCard` (static in `lounge/MessageItem.tsx`, Lounge chunk), `StartPartyDialog`, `Theatre`, `WatchPartyPage`, `WatchPartyStrip`; deleting a rule only shrinks it.

## Done (commits on `fix/p103-watch`)
- Merged `fix/p103-kit` (the `fit` prop). `ControlBar.tsx` Speed and `TitlePicker.tsx` Season: `<NativeSelect fit="content">`, props kept (`aria-label`, `title`, `disabled`, `value`, `onChange`, children); diff read line by line (import, open tag plus `fit`, close tag). Each committed `SEC-AUD-465 ... (READY)`.
- `watchparty.css`: the rule `.wp-field select, .wp-inline select` (background, color, border, radius, `padding: 6px 8px`) deleted whole (8 lines), nothing else in the sheet changed. Subject `SEC-AUD-464 ... (ALL READY)`. All five dependants (ControlBar Speed, PeoplePanel x2, StartPartyDialog Where, TitlePicker Season) are now kit selects, so the rule reached nothing raw. Theme check: Cinema shipped `sideroom` has `--ui-bg-raised` `#1c1b17` (themes.ts:257) against the party panel's `--sr-bg-1` `#14120f` fallback: readable, but mediashots' shots decide (neither measured by me).
- No class was on any of the five selects, none kept. `.wp-field > input` (link field) untouched.

## Not done
Nothing. Not measured (stack-free): widths, chevron clearance, hit tests, shots.

## Checks (after my last edit, 11:58)
- prettier on the two tsx and the css: clean. eslint on the two tsx: exit 0.
- Unit tests, unedited: vitest `start, startLink, theatre, p16bubbles` in `src/watchparty/__tests__`: 4 files, 44 tests passed (11:58).
- `node scripts/check-css-tokens.mjs`: exit 0 (73 files clean). `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0.
- TYPECHECK: `bash cli-agents/tc.sh run watch bash -c "cd .../p103-watch/apps/web && npx tsc --noEmit -p ."` 11:58 to 12:00, exit 0.
- STACK-ROUND: none (code builder). No lock taken, nothing running.

## Text for the inventory
- SEC-AUD-462: the Speed and Season selects sit in inline-flex labels and use `fit="content"` so they are as wide as their text.
- SEC-AUD-464: `watchparty.css` no longer styles selects by element name (rule deleted).
- SEC-AUD-465: the watch party's last two raw selects (Speed, Season) are the kit's; all five party selects now share the kit look.

Changelog: The watch party's Speed and Season selects now use the kit's select with its chevron, and the party's sheet no longer restyles selects.

Owner questions: none. If mediashots finds the kit's `--ui-bg-raised` unreadable on the party panel, the cure is a class-based colour fix in `watchparty.css` (named in FAULTS), not a return of the element rule.
