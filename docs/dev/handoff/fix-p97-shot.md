# Wave 97, shot: SEC-AUD-436 (D-943), the probe spec's long-menu shot is taken with no submenu open

Clock: started 15:58, round 15:58 to 16:00 (lock taken 15:58, released 16:00), committed 16:00.

## Done
- `e2e/tests/p96-probe-shots.spec.ts` only (+16 / -1 lines). The base (173dfdef) did NOT have the `page.mouse.move(1, 1)` step (log shows it reverted).
- Diff in words: between the menu's `waitFor` and the scroll-to-top and shot, the spec now takes `menu.locator('[role=menuitem]:not([aria-haspopup]):not([aria-disabled="true"])').first()` (here "Edit Message", since "Add Reaction" has a submenu), `scrollIntoViewIfNeeded()`, `hover()` (worked in the touch context; no `mouse.move` needed), waits 450 ms (hover intent `SUBMENU_HOVER_INTENT_MS` = 150 in Menu.tsx:25), then asserts no `[aria-haspopup="menu"][aria-expanded="true"]` in the menu and `getByRole('menu')` count 1 (both messages name D-943). One header line added. Four cases, assertions, probe.json, variables and the command are unchanged.
- `menu = getByRole('menu').first()` is the first in document order; with the count assertion it is one panel either way.
- Which way the last commit holds: hover on the first plain item (the first way; no fallback needed). The spec in the commit is the spec the round ran.

## Round
Fresh DB `sr_p97_shot_ui`, seed printed no FAIL, spec: 1 passed (13.4 s), `--project=chromium --reporter=line`, SR_P96_PROBE_OUT=visual-sweep-shots/p97-shot. `probe.json` is identical to wave 96's `p96-probe/probe.json` (compared whitespace-stripped: no difference; items=14, case1 0 rows, case2 14 rows, skipped none, main 148/148).

## Shot (1, opened)
`visual-sweep-shots/p97-shot/long-menu-375-light.png`: 14 rows whole, one panel, "Remind Me" submenu gone, last three rows (Remind Me, Pin Message, Delete Message) uncovered. Change versus wave 96's: submenu gone; the first plain row (Edit Message) shows the hover highlight (acceptable). Nothing cut or crowded.

## Checks (own tree, after the last edit)
- `npx tsc --noEmit -p tsconfig.json` in e2e: exit 0 at 15:58 (after the spec edit; no edit since).
- `npx prettier --check tests/p96-probe-shots.spec.ts`: clean, 15:58.
- `node scripts/check-doc-links.mjs`: see commit time below.

## Commander's command (header, unchanged), from e2e/, ports for your stack
`SR_E2E_BASE_URL=http://localhost:5741 SR_P96_PROBE_ON=1 SR_P96_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p96-probe npx playwright test tests/p96-probe-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (set SR_CLI_BIN, SR_DATABASE__URL, DATABASE_URL; sweep world seeded; pick your own OUT folder to keep p96-probe as the base).

## Text for the commander
- SEC-AUD-436 closing: "Closed (wave 97, D-943): the probe spec rests the pointer on the first plain item and asserts no open submenu and one menu panel; shot visual-sweep-shots/p97-shot/long-menu-375-light.png shows 14 rows, no submenu; probe.json unchanged."
- AGENTS.md sentence as the plan gives it.
- Changelog: nothing a member sees (test harness only).
- Docs updated: none. Threat-model rows: none. Owner questions: none.

## Cleanup
Servers stopped (by the PIDs owning ports 8751/5751/9451; all three free), DB `sr_p97_shot_ui` dropped, `p97-shot-bin` deleted, `e2e/.state` removed, no stash touched. `heavy.sh status` after release: `free`.
