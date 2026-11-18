# Wave 102, watch (code builder, pod media): SEC-AUD-453, D-958, the watch party's five raw `<select>` move to `NativeSelect`

2026-11-18. Started 09:30; four files moved and committed by 09:33; TYPECHECK 09:33 to 09:34. Files: `apps/web/src/watchparty/{ControlBar,PeoplePanel,StartPartyDialog,TitlePicker}.tsx`, this handoff. No sheet, no kit file, no string, no spec.

## REACH table (for mediashots)
Base commit `b308d5e2`. All five moved (READY commits `0e58f4d1`, `1d040fe9`, `814b1a38`, `1c923f1f`). Each keeps its props, children and `aria-label`; no class was on any of them, so none was dropped or kept.

| # | File:line (base) | Label / id | Where, and how to open it | Sheet rule now | Flex? | Found by tests |
|---|---|---|---|---|---|---|
| 1 | `ControlBar.tsx:131` | `aria-label` "Playback speed" (`watchparty.controls.speed`; the span before it is `wp-sr-only`); `disabled={!can}`, `title={why}`; no id | The theatre `/cinema/party/<id>`, the control bar under the player; only when `!live`. Open: sign in as the host, `POST /watch-parties {title_id}` (the title made as in `p80-dark-shots.spec.ts` 259-336: `POST /admin/cinema/titles`, `PUT .../asset {asset_id:'4242'}`, `POST .../publish`), then `page.goto('/cinema/party/<id>')` and wait for `.wp-root`. | `watchparty.css:412-419` `.wp-field select, .wp-inline select` (element selector, specificity 0,1,1: background, border, radius, `padding: 6px 8px`). It beats the kit's `.select` and replaces its 36 px end padding, so the chevron may sit over the text; it does not set a height. | **Yes: `label.wp-inline` is `display: inline-flex`** (gap 6, `watchparty.css:245`); the kit's wrapper span (`display:block; width:100%`) is now the flex item inside a shrink-to-fit label. Measure its width against the base. | No e2e finds it. `theatre.test.tsx` does not name it. |
| 2 | `PeoplePanel.tsx:142` | label text "Who can play, pause and seek" (`watchparty.people.whoControls`, wrapping `label.wp-field`); no id | Theatre, tab "People" (`getByRole('tab', {name:'People'})`), as the host. Same party as #1. | `.wp-field select` (same rule). | `label.wp-field` is `display: grid`; the span is a block grid item, 100% wide. Low risk. | `theatre.test.tsx:302` `findByLabelText('Who can play, pause and seek')` + `selectOptions('everyone')`; by label, not class (passes). |
| 3 | `PeoplePanel.tsx:155` | label text "Who can add to the queue" (`watchparty.people.whoQueues`) | Same tab, directly under #2. | same | grid | none |
| 4 | `StartPartyDialog.tsx:211` | `aria-label` "Where to watch" (`watchparty.startDialog.whereToWatch`); visible span "Where" | Dialog "Start party". Open: sign in, `/cinema/t/<titleId>`, click `getByRole('button',{name:'Watch together'}).first()` (`polish-media-flows.spec.ts:63` and 113-114 do this and click "Start party" in the dialog). Also from a voice channel's call bar (`StartPartyButton`), a Play-here card, the TV page. Has `optgroup`s (a server's channels, "Direct messages"). | `.wp-field select` | `label.wp-field` grid; low risk. The dialog is a kit `Modal`. | `start.test.tsx:129` `findByRole('combobox', {name:'Where to watch'})` (passes). |
| 5 | `TitlePicker.tsx:182` | `aria-label` "Season" (visible span "Season" in `label.wp-inline`); no id | The episode step of the picker: pick a series, then "Season". Reached from (a) a theatre with no media (`PickMedia`), (b) the Queue tab (`QueuePanel`), (c) the Start dialog without a title (from a voice channel's call bar). Needs a published series in the cinema search (`kind:'series'`, as in the p80 world) and a click on its result. `season_count` may be absent: the select then has one option "1" and still renders. An empty episode list is fine. | `.wp-inline select` | **Yes: `label.wp-inline`, inline-flex**, a visible "Season" span plus the select, inside `.wp-picker-head` (a row with Back and the title; I did not read its rule). Measure. | `start.test.tsx` "picks an episode of a series" passes. |

- Area theme: the cinema section (`/cinema/party/...` and the title page sit under `.cin-root`); `p80-dark-shots.spec.ts` toggles `sectionTheme` and reads `.cin-root[data-theme]`. The Start dialog opened from the Lounge call bar is under the Lounge theme.
- **Memory stack:** a `memory:` stack opens everything above. Titles are made with the admin API and a fake `asset_id` (no bucket), so the title page, "Watch together" and the party page open (`p80-dark-shots.spec.ts` does this). It cannot play video (no object store, SEC-AUD-429). Whether `ControlBar` (#1) renders with a fake asset I did not prove: `Theatre.tsx:119` shows `PickMedia` when there is no media and `ControlBar` at 131 otherwise; a party with `title_id` has media, so the bar should show with a stalled player. If #1 is not reachable, say "not reached".

## Who imports each, and the chunk (first paint check)
- `ControlBar` and `PeoplePanel` <- `Theatre.tsx` only. `Theatre` <- `WatchPartyPage` <- `PartyRoute` <- `watchparty/routes.ts` `lazyNamed(() => import('./PartyRoute'))`: a lazy route chunk. Not first paint, not public.
- `TitlePicker` <- `PickMedia`, `QueuePanel` (both from `Theatre`), and `StartPartyDialog`.
- `StartPartyDialog` is only loaded with `lazy(() => import(...))`: `cinema/TitlePage`, `cinema/tv/WatchTogether`, `lounge/msg/EmbedsPlay`, `lounge/msg/RichCards`, `lounge/voice/ScreeningRoom`, `watchparty/StartPartyButton`; the unit tests import it directly. Lazy each time.
- For the commander: `lounge/MessageItem.tsx` imports `PartyMessageCard` statically, and that file imports `watchparty.css`; so the sheet (not my four files) is in the Lounge's chunk. My files are not.

## Checks (after my last code edit, 09:33)
- Diff read line by line: only the import, the open and the close tag per select (PeoplePanel's import was reflowed to one name per line by prettier). No id, prop or child changed; no class was present.
- prettier on the four files: clean (09:32). eslint on the four: exit 0 (09:33).
- Unit tests (unedited): `npx vitest run src/watchparty/__tests__/{start,startLink,theatre,p16bubbles}.test.tsx` from `apps/web`: 4 files, 44 tests passed (09:33). They find the selects by label and role, never by class.
- TYPECHECK: `bash cli-agents/tc.sh run watch bash -c "cd .../p102-watch/apps/web && npx tsc --noEmit -p ."` 09:33 to 09:34, exit 0.
- `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0 (09:33). `node scripts/check-doc-links.mjs`: ok, exit 0 (09:33; re-run after this file, see the commit).
- STACK-ROUND: none (code builder). No lock taken, nothing running.

## Faults to expect (for mediashots to measure, then FAULTS back to me)
1. `watchparty.css:412-419` still reaches all five by element name and sets `padding: 6px 8px`, which replaces the kit's `padding-inline-end: 36px`: the chevron (14 px from the end, 6 px wide) may overlap the selected text, most on "Where to watch" with a long channel name. The kit's `min-block-size` (40; 44 on touch) is not overridden, so the height should equal the kit's. If the chevron overlaps the text I cannot cure it (the sheet is not mine and `className` cannot beat specificity 0,1,1): that select goes back to raw.
2. Inline-flex labels (#1, #5): the select's box can come out narrower than the base's (shrink-to-fit with a 100 % child). If narrower than the base, or the label wraps, those two go back to raw.

## Text for SEC-AUD-453
The watch party's five selects (the speed in the control bar, the two People settings, "Where to watch" in the Start dialog, "Season" in the title picker) moved to `NativeSelect` (D-958), pending the pod's after-round. `watchparty.css:412-419` (`.wp-field select, .wp-inline select`) was not edited (not mine) and still styles them by element name: it should become `.wp-field > select` and go with the last raw select (D-967), by the sheet's owner. Anything put back to raw is listed below.

## Put back to raw
None yet.

## Proposed rows and sentence
- Proposed inventory row: "Watch party sheet `watchparty.css:412-419` styles `select` by element name (`.wp-field select, .wp-inline select`) and replaces the kit's end padding; remove it now that the party's selects are `NativeSelect` (D-967)."
- Changelog sentence: "The watch party's drop-downs (playback speed, who controls and who queues, where to watch, the season) have the kit's look and a full touch size on a phone."

## Owner questions
None.
