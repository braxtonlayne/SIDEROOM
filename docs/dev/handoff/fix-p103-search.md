# Wave 103, search: SEC-AUD-461 (Sort select keeps focus, D-980) and SEC-AUD-464 (`.lng-search-sort select` leaves msg.css, D-972)

Branch `fix/p103-search`. Clock: start 11:56, code and spec 12:15, STACK-ROUND 12:28 to 12:32, handoff 12:35.

## Done (commits)
- `SEC-AUD-461 search results Sort select keeps focus and value while the query re-runs (D-980)`: `apps/web/src/lounge/SearchBox.tsx` and the spec `e2e/tests/p103-search-shots.spec.ts`.
- `SEC-AUD-464 .lng-search-sort select leaves msg.css (D-972)`: `apps/web/src/lounge/msg/msg.css` (the 7-line rule deleted).

## Verdict for 461: A (real)
**Cause, one sentence:** the results bar (and with it the Sort select) rendered only when `status.kind === 'done'`, and `changeSort` runs `exec`, which sets `status` to `loading`, so every change unmounted the select; the new node was not focused, and the next ArrowUp went to `<body>`.
(Not a stale closure, not a `key`, not a disabled state. Wave 102's "ArrowUp leaves Oldest" is exactly this: ArrowDown changed Newest to Oldest, the node was replaced, focus was gone.)

**Diff in words** (SearchBox.tsx only): `Status` gets `loading` with an optional `prev` (the last `done` answer); `exec` takes a `keep` flag and, for a sort or page change (`changeSort`, `goPage`), keeps the last answer in `prev`; the bar, list and pager render from `shown` (the finished answer, or `prev` while loading); the "Searching..." line shows only when there is no `prev` (a new query from the box still shows it and no stale list). No new string, no kit file, nothing else.

## Reads (1024 x 768, Lounge Light, six hits in two channels; settle 2.2 s after each key; same node = marker attribute set on the select before the first key)
Sequence 1, from Newest. "ok" = focused and same node.

| step | base value / focus / node | after value / focus / node | first hit (after) |
|---|---|---|---|
| ArrowDown | old / lost / replaced | old / ok | m1 (oldest) |
| ArrowUp | old (key went to body) | new / ok | m6 |
| ArrowDown | old | old / ok | m1 |
| ArrowDown | old | relevant / ok | m6 |
| ArrowUp | old | old / ok | m1 |
| ArrowUp | old | new / ok | m6 |

Sequence 2, the native way, from Newest. **Alt+ArrowDown cannot be used: the Lounge takes it as "next channel"** (`lounge/shortcuts.ts:185`; recorded in both phases: the path changed to the next channel and the results panel closed). The list is opened with F4 instead (Chromium on Windows): F4, ArrowDown, Enter; F4, ArrowUp, Enter; F4, ArrowDown x2, Enter; F4, ArrowUp x2, Enter.

| step | base | after |
|---|---|---|
| F4, Down, Enter | old / focus lost | old / ok |
| F4, Up, Enter | old (stuck) | new / ok |
| F4, Down x2, Enter | old (stuck) | relevant / ok |
| F4, Up x2, Enter | old (stuck) | new / ok |

After: every option reached in both directions with focus on the same node throughout; the first hit follows the sort (Newest m6, Oldest m1; spec asserts it on its own messages). A click still works (`selectOption`, used by the spec between sequences). Unit tests rendering the search box, unedited: `lounge/__tests__/p70Lines.test.tsx` 25/25 and `p71Panels.test.tsx` 19/19.

## 464 (msg.css)
Dependants: `git grep -n lng-search-sort -- apps/web/src e2e`: only `SearchBox.tsx:356` (the label) and the deleted rule; the Sort select is its only dependant and was the kit's since wave 102. Measured base and after (the base from scratch copies of both files, same stack): the numbers are **identical** (the kit's one class already won over the element selector's box at the sizes measured, and the colours were the theme's anyway).

| config | select base / after | bar h | count box | hit test base / after |
|---|---|---|---|---|
| 1024 Lounge Light | 153 x 40 / 153 x 40 | 40 / 40 | same | 6 pass 6 fail / 6 pass 6 fail |
| 1024 Lounge Dark | 153 x 40 / 153 x 40 | 40 / 40 | same | 6 pass 6 fail / 6 pass 6 fail |
| 375 touch Lounge Light (phone search sheet) | 153 x 44 / 153 x 44 | 44 / 44 | same | 12 pass 0 fail / 12 pass 0 fail |

Selected text 56.9 px in 151 px client width: clear of the chevron everywhere. No sideways scroll (1024/1024, 375/375). The 6 fails at 1024 are the filter chips and the select taken by the panel/bar above or below their 21 px points: the same six in the base, 28 to 40 px controls at 1024 (SEC-AUD-467), noted, not a fault of this change.

## Who imports (lazy chunk)
`SearchBox.tsx`: `ChannelHeader.tsx` (desktop) and `header/PhoneSearch.tsx` (phone sheet), both in the Lounge (`routes/lounge.tsx` lazy-loads `LoungeRoot` -> `LoungeShell` -> `ChannelView`/`ChannelHeader`). `msg.css`: imported by `Composer.tsx` and `MessageList.tsx`, also the Lounge. Neither is loaded by first paint or a public page; no web build run.

## STACK-ROUND
Slot held 12:28 (`heavy.sh status`: `search 12:28`), stack 8803/5803/9503 on `sr_p103_search_ui`, `SR_ADMIN_REQUIRE_REAUTH=0`, sweep seed printed no FAIL, base phase 12:29:38 (1 passed, 54 s), after phase 12:30:37 (1 passed, 52 s, `problems: []`), stack stopped by PID, ports free, released 12:32. Earlier rounds of mine (12:06 to 12:14, a message 429 and a Alt+ArrowDown mistake in my spec) were spec errors, fixed; they hold no result.
```
SR_E2E_BASE_URL=http://localhost:5803 SR_P103_SEARCH_ON=1 SR_P103_SEARCH_PHASE=<base|after> SR_P103_SEARCH_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-search npx playwright test tests/p103-search-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
(from `e2e/`, on a stack with the sweep world; `SR_P103_SEARCH_CFG=<regex of 1024-lounge-light | 1024-lounge-dark | 375-lounge-light>` limits configs.) This is the commander's command whole; base needs the base `SearchBox.tsx` and `msg.css` put back (`git show integration/phase103:<path>`) and delete nothing else. The spec makes two channels and six messages of its own (`p103s-a/b-<stamp>`, one word) and removes the channels in a `finally`.

## TYPECHECK
`bash cli-agents/tc.sh run search bash -c "cd .../p103-search/apps/web && npx tsc --noEmit -p ."`, 12:15, exit 0. Run after my last app edit (no `.ts`/`.tsx` edit since; later edits were the spec). Others, after the last edit: `npx prettier --check` on SearchBox.tsx and the spec, clean; eslint on SearchBox.tsx (from `apps/web`), exit 0; `node scripts/check-css-tokens.mjs` clean; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; e2e `tsc --noEmit` exit 0 (12:32). `check-doc-links.mjs` not run (no doc text changed beyond this handoff).

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-search`, 8 made, all 8 opened)
- `base-search-1024-lounge-light.png`: Newest, six results grouped in two channels, select 153 x 40, grey field, no focus ring.
- `search-1024-lounge-light.png`: same; identical box and layout; the select carries the focus ring (it kept focus after the spec's last change).
- `search-1024-lounge-dark.png`: dark panel, select text readable, no overlap, group headings and hits as base.
- `search-375-lounge-light.png`: the phone "Search" sheet, select 153 x 44 on the right of "6 RESULTS", text clear of the chevron. (The row of filter chips above is clipped to a sliver under the box: also in `base-search-375-lounge-light.png`, so not mine; listed below.)
- `search-1024-focus.png`: after ArrowDown, ArrowUp: "Newest", focus ring on the select, first hit m6.
- `base-search-1024-focus.png`: after the same keys on the base: shows "Oldest", no ring, first hit m1 (the stuck state).
- `base-search-1024-lounge-dark.png`, `base-search-375-lounge-light.png`: same as the after shots, no difference.

## STALE
`e2e/tests/p102-media-shots.spec.ts` (the `lounge-search` scenario, `kbd: { ..., settleMs: 2200 }`, and the keyboard check that reports "ArrowUp leaves Oldest"): the ArrowUp problem it reports in both phases is fixed by this branch; that report line is stale on the merged tree (I did not edit the spec; I did not run it). Handoff `fix-p102-mediashots.md` "Spec notes" likewise.

## Not mine, found
- **Alt+ArrowDown on a focused select is taken by the Lounge as "next channel"** (`apps/web/src/lounge/shortcuts.ts:185`, `matchShortcut` has no guard for a focused select/field), so a keyboard user cannot open any select in the Lounge with the native Alt+ArrowDown (F4 works). Proposed row (SEC-AUD-470, low): the Alt+Arrow channel shortcuts ignore events whose target is a `select`.
- The filter-chip row of the phone search sheet is clipped under the search box at 375 (both phases).

## Docs
No doc states this behaviour; none changed. Changelog sentence: "The Lounge search results' Sort select keeps its focus and its value while the results reload, so ArrowUp and ArrowDown reach every option (SEC-AUD-461), and its box rule left the message sheet (SEC-AUD-464)."

## Proposed inventory rows
SEC-AUD-461 closed (A). SEC-AUD-464 search part closed. New: SEC-AUD-470 as above.

## Closing texts / owner questions
D-980 as the task states it holds for the search box. Owner questions: should the page buttons of the results ("Previous"/"Next") keep the stale list while the next page loads (they now do, same as the sort)? Default taken: yes.

## Clean-up
Database `sr_p103_search_ui` dropped by the round script (`DROP DATABASE ... FORCE`); `p103-search-bin` deleted; ports 8803, 5803, 9503 free; scratch copies only in my scratchpad folder.
`heavy.sh status` at the end: no hold in my name (see the line in my reply). No stash touched.
