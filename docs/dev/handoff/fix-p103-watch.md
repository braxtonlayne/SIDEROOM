# Wave 103, watch (code builder, pod media): SEC-AUD-462, 464, 465; D-972

Started 11:56. Files: `apps/web/src/watchparty/{ControlBar,TitlePicker}.tsx`, `watchparty.css:412-419`, this handoff. Status at 12:05: REACH table committed; waiting for the kit prop (`fit` is not in my base; `fix/p103-kit` has no commit yet).

## REACH table (for mediashots)
Rule reached: `watchparty.css:412-419` `.wp-field select, .wp-inline select` (element selector: background, color, border, radius, `padding: 6px 8px`). Area theme: Cinema (`sectionThemes.cinema`, shipped `sideroom`); the Start dialog opened from the Lounge call bar is under the Lounge theme. Way in (as `e2e/tests/p102-media-shots.spec.ts`): a movie (admin API, fake `asset_id`) and a party made through the API (`POST /watch-parties {title_id}`; `{}` answers 400).

| # | File:line (base) | Label / id | Page and how to open | Sheet rule now | Now | `fit` after | Found by |
|---|---|---|---|---|---|---|---|
| 1 | `ControlBar.tsx:131` | aria-label "Playback speed", `disabled={!can}`, `title={why}`; in `label.wp-inline` (inline-flex, `watchparty.css:245`) | `/cinema/party/<id>`, control bar, only when `!live`; host | `.wp-inline select` | raw | `content` | no test, no e2e |
| 2 | `PeoplePanel.tsx:151` | "Who can play, pause and seek"; `label.wp-field` (grid) | party, tab People, host | `.wp-field select` | kit | fill | `theatre.test.tsx:302` by label |
| 3 | `PeoplePanel.tsx:164` | "Who can add to the queue" | same tab, under #2 | same | kit | fill | none |
| 4 | `StartPartyDialog.tsx:211` | aria-label "Where to watch"; `label.wp-field`; has optgroups | `/cinema/t/<titleId>`, "Watch together", dialog "Start party"; also Lounge call bar | same | kit | fill | `start.test.tsx:129` by role |
| 5 | `TitlePicker.tsx:182` | aria-label "Season"; visible span in `label.wp-inline` (inline-flex) in `.wp-picker-head` | picker episode step: Queue tab, theatre with no media, or Start dialog without a title; needs a published series | `.wp-inline select` | raw | `content` | `start.test.tsx` "picks an episode" by role |

Dependants of the rule: exactly these five (`git grep wp-field|wp-inline` finds no other file, no e2e or test class use). `.wp-field > input` (link field, StartPartyDialog:189) is not a select and is not touched.

## Who imports each file
- `ControlBar` <- `Theatre.tsx` <- `WatchPartyPage` <- `PartyRoute`, lazy route chunk (`watchparty/routes.ts`). `TitlePicker` <- `PickMedia`, `QueuePanel` (Theatre) and `StartPartyDialog`, lazy everywhere. Not first paint, not public.
- `watchparty.css` is imported by `PartyMessageCard` (static in `lounge/MessageItem.tsx`, Lounge chunk), `StartPartyDialog`, `Theatre`, `WatchPartyPage`, `WatchPartyStrip`; deleting a rule only shrinks it.
