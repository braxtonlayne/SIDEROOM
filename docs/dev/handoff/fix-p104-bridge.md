# fix-p104-bridge (wave 104, pod lounge, code builder, stack-free)

Clock: started 14:12, code and typecheck done 14:15, handoff 14:17 (the session clock).

## REACH table
| element | file:line | label | how to open | sheet rule now | theme | found by class/id in e2e or tests |
|---|---|---|---|---|---|---|
| Kind select (the change) | `settings/server/bridge/ConflictLog.tsx:107-125` | "Kind" text (`settings.bridgeSync.conflicts.filterKind`), `label.ss-field-label` in `.ss-toolbar` beside the Switch | Server settings > Discord bridge, card `data-testid="bridge-conflicts"`. `e2e/tests/p103-lounge-shots.spec.ts` fakes `/bridge/status`, `/bridge/conflicts` and the features list with `page.route` (see its lines 185-236, 318-331) | kit `.select`/`.wrapFit` + my new `.ss-field-label .ss-field-pick`; the label is `.ss-field-label` (:237) | Lounge Light/Dark (settings page) | testid `bridge-conflicts` + role combobox (p103-lounge-shots, bridge-whole-guild.spec.ts:120, p102-pages-shots); nobody finds it by `ss-select` |
| Bridge map "Ours/Theirs/Direction" | `BridgeSection.tsx:340,351,362` | label.ss-field-label | Discord bridge > channel map "Add" fieldset | kit NativeSelect (fill), no extra type rule | Lounge | not seen in p103 shots (not measured) |
| Plan link "Here/There" | `PlanView.tsx:450,461` | label.ss-field-label | Discord bridge > plan > link fieldset | kit NativeSelect (fill) | Lounge | not seen |
| Other `label.ss-field-label` holding a control (Members, ChannelTabs, Channels, Overview, Moderation, AuditLog, etc., ~45 in `apps/web/src/settings/server/sections`, `lounge/rooms/settings/ListeningRoomSettings.tsx`) | see `git grep -n ss-field-label -- apps/web/src` | various | Server settings pages | unchanged | | unchanged by me; they carry neither new class |

Unchanged by me: all of the above; `.ss-field-label` and `.ss-select` rules are not edited.

## Done
- `D-997 ConflictLog.tsx (READY)`: raw `<select className="ss-select">` is `<NativeSelect fit="content" className="ss-field-pick">`, same value/onChange/children; label text wrapped in `<span className="ss-field-label-text">`.
- `D-997 server-settings.css (ALL READY)`: one block at the end: `.ss-field-label .ss-field-pick { font-size: 0.875rem; font-weight: 400; letter-spacing: normal; text-transform: none }` and `.ss-field-label > .ss-field-label-text { display: inline-block; margin-inline-end: 9px }`.
- `ss-select` dropped from this select: `git grep -n ss-select -- e2e apps/web/src` finds only the sheet, Members (:308,:324), ChannelTabs (:594) and selector lists in p84/p85 specs; none finds this select.

## What I read, and what I could not measure
- The base `.ss-select` rule (server-settings.css:222) has no margin and `font: inherit`, so it does not itself give a 9 px gap or 14 px type on paper; the 9 px and "14 px bold" come from wave 103's measurements of the base (`fix-p103-loungeshots.md`). I used 9px and 14px as given, with weight 400 as D-997 says (wave 103 saw the base as bold: if loungeshots finds the base at 600, "weight" is the one value to change; it is in my block). 9px is a literal in a margin (check-css-tokens passes). Not measured: stack-free.
- The base label was shrink-to-fit in the flex toolbar; `.wrapFit` is `inline-block`, `max-content`, so label text and field sit on one line and the Switch should move only by the width growth.

## Who imports
`ConflictLog.tsx` <- `BridgeSection.tsx` <- `ServerSettings.tsx`, loaded lazily by `routes/settingsServer.tsx`: Settings chunk. `server-settings.css` is the Settings chunk's sheet (not loaded by first paint or public pages; the new block names no section or theme beyond the existing `ss-` classes). No first-paint file touched.

## Checks (all on the committed tree)
- TYPECHECK: `tc.sh run bridge ... npx tsc --noEmit -p .` 14:14, exit 0.
- prettier --write on both files (css unchanged, tsx reformatted before commit); web eslint on ConflictLog.tsx exit 0; `check-css-tokens.mjs` exit 0; `i18n-audit.mjs --max 0` TOTAL 0, exit 0.
- Unit tests: none render ConflictLog (no `__tests__` file names it); not run.

## STACK-ROUND: none (code builder)

## Proposed text
- SEC-AUD-465 remainder: the Conflict log's Kind filter is the kit's `NativeSelect` (`fit="content"`), with the base's 14 px type and 9 px label gap kept through `.ss-field-pick` and `.ss-field-label-text`; no raw select is left under the bridge.
- Proposed row (not built): the other bridge `label.ss-field-label` selects (BridgeSection :340-362, PlanView :450,:461) are kit selects with the same inherited small-caps type; they probably want `ss-field-pick` too, once loungeshots measures them.
- Changelog: The Discord bridge conflict log's Kind filter uses the shared select field again, keeping its label gap and text size.
- Owner questions: none (weight 400 vs the base's bold: Fable to confirm if loungeshots measures the base bold).

## FAULTS from loungeshots: none received.

## PROCESSES
Nothing of mine runs (no stack, no vitest). Listing run before the reply: see the final report.
