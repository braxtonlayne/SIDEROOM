# Handoff: p105 bridge (D-1011; SEC-AUD-493, 494)

## Clock
Started 15:30; code, checks and tsc done by 15:40.

## Done
- `server-settings.css`: one block at the very end: `.ss-field-label .ss-form-pick` (14 px, 400, no tracking, no capitals, `--ui-text`).
- `BridgeSection.tsx`: `className="ss-form-pick"` on the three selects (Our channel, Discord channel, Direction).
- `PlanView.tsx`: the same on the two (link here, link there).
- Commits: `D-1011 BridgeSection.tsx (READY)`, `D-1011 PlanView.tsx (ALL READY)` (code + css + this handoff).

## Not done
- No gap added between label text and select. Confirmed from the sheet: `.ss-field-label` is `display:block`, 12 px/600/caps/tracking .04em; NativeSelect has `font: inherit` and `color: var(--ui-text)`, so the select inherited all of the label's type. The label text is a bare text node above the block `.wrap`, so the base gap is 0 (not measured in pixels, read from CSS). The task asks for a gap only if base < 6 px and via a class on the label text, which needs a JSX wrapper and moves the select down; the "must not change" list keeps the select's box, so I did not do it. staffshots: if it measures a cramped label-to-field distance, that is a FAULT for a named cure (wrapper span + `.ss-field-label > .ss-field-label-text` as wave 104 did), not mine to guess.

## REACH table
| file:line | label | page / how opened | rule now | found by class in e2e/tests |
|---|---|---|---|---|
| BridgeSection.tsx:342 | Our channel (fieldOurs) | Server settings > Discord bridge, "Mirror another channel" fieldset (`.ss-bridge-add`) | `.ss-field-label` 12/600/caps via `font: inherit` | `p101-mod-shots.spec.ts:391` `.ss-bridge-add select` |
| BridgeSection.tsx:357 | Discord channel (fieldTheirs) | same | same | same |
| BridgeSection.tsx:372 | Direction (fieldDirection) | same | same | `p101-mod-shots.spec.ts:394` `.ss-bridge-add select >> nth=2` |
| PlanView.tsx:452 | link here (linkHere) | Discord bridge > plan view (`data-testid=bridge-plan`), fieldset "link" | same | `p101-mod-shots.spec.ts:416-419` |
| PlanView.tsx:467 | link there (linkThere) | same | same | same |

States: the add fieldset shows whenever the bridge section renders its mapping area (rendered by `__tests__/bridge.test.tsx`). The plan's link fieldset needs `onlyHere.length > 0 && onlyThere.length > 0` (channels only on SIDE ROOM and only on Discord). Reachable with `e2e/support/bridgeFake.ts` via the `fakeBridge(page, guildId, channelId, withPlan)` route mocks of `p101-mod-shots.spec.ts:103` (read only; staffshots must mock the plan with both lists non-empty).

Other `.ss-field-label` controls: about 50 across settings/server sections, admin/cinema, ListeningRoomSettings, and the Conflict log. My block matches only elements carrying `.ss-form-pick`; `git grep` shows the class only on my five selects. The Conflict log's Kind keeps `.ss-field-pick` (rule unchanged, weight 600). No other control is reached.

## JOBS
0016-bridge-tsc: tsc; submitted 1795033857, started 1795033923, ended 1795033969, rc 0 (queued about 66 s, ran 46 s).

## Figures
None (no shots this role). May change: the five selects' type. Must not change: heights 40/44, widths, labels' text, fieldset layout, Conflict log Kind. By construction the block sets no box property.

## Shots
None; staffshots measures.

## Imports
BridgeSection.tsx and PlanView.tsx: imported by the server settings bridge section (lazy settings chunk, not first paint); `git grep` them for the exact importer. server-settings.css is imported by the lazy server settings chunk.

## Checks (own tree, last commit)
- `npx prettier --write` on the 3 files: ok. eslint on both TSX (from apps/web, `pnpm` not on PATH in the shell): exit 0.
- vitest `bridge.test.tsx` 21 passed; `sync.test.tsx` 17 passed (15:31).
- `node scripts/check-css-tokens.mjs`: exit 0, 73 files clean.
- tsc job 0016: rc 0.

## Closing texts
- SEC-AUD-493 (Mirror another channel selects read as uppercase labels): fixed by D-1011, `.ss-field-label .ss-form-pick` on the three selects; pending staffshots' pixel check.
- SEC-AUD-494 (plan link selects): same rule on the two; pending staffshots reaching the link fieldset. If it cannot, put PlanView.tsx back and 494 stays open.

## Changelog sentence
The Discord bridge's channel and direction pickers now read as normal 14 px form fields instead of small uppercase labels.

## Owner questions
None.

## PROCESSES
strays: none found
