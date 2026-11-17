# Wave 91, members: SEC-AUD-400 (D-912), a CI spec for Members' row checkbox

T = 10:48 on 2026-11-17. Lock taken 10:47, released 10:54. Finished 10:55.

## Done
`e2e/tests/members-hit-test.spec.ts` (new, the only file changed). No source, package, support or config change. It reads no `SR_P91_*` variable, has no switch and no `@gate`/`@showroom` tag, so CI's `tests/[g-z]` step runs it in chromium, firefox and webkit.

Step by step:
1. Own data through the API: `scenario.ownerAndMember()` (the Owner, a new server, one member joined by invite) plus a third account (`seedUser('third')`, joined by a channel invite): three rows. Unique names per run. Nothing from `e2e/.state/sweep`.
2. 375 x 812, viewport only (no `isMobile`, no `hasTouch`). Opens Server Settings, then the "Sections" button, then "Members", as a member does.
3. First row's checkbox, found by role and accessible name (`Select <name>`): scrolled to the centre of the viewport; one `page.evaluate` returns, for the centre and 21 px above, below, left and right, the tag and a short path of what `elementFromPoint` hit. Each must be the input, a node inside its wrapper, or its own label; the message prints all five hits so a CI failure names the taker.
4. No other interactive control of the row is returned by any of the five points; each other control of the row (name, Roles, More actions) returns itself at its own centre; no sideways scroll at 375.
5. `click()`: checked, the bulk toolbar shows. Focus + Space: unchecked, toolbar gone.
6. 1280 x 800: "Select all members on this page" exists, one click checks all three rows (and itself), the toolbar shows; a second click clears everything.

Behaviour only: no class name, no pixel size of the target. It should pass unchanged after the kit builder removes `ss-check-solo` (D-905); the commander runs it on the merged tree.

Helpers: `apiPost`, `expect`, `test` from `e2e/support/fixtures` (`scenario`, `seedUser`, `signedIn`). No helper added to `support/`; the probe lives inside the spec.

## Checks
- Chromium, `--workers=1 --retries=0`: the first runs failed on my own spec bug (on a phone the section list is behind the "Sections" button and the item is only in the accessibility tree once it is open; fixed). Then three green runs in a row on the stack, 3.5 s each.
- Watched fail: a scratch copy (deleted, not committed) put a fixed `<a>` over the checkbox's upper half; step 3 failed with `point above ... was taken by: above: a (a < body < html) MISS` and the other four points listed as the input.
- prettier --check, eslint (apps/web's), e2e `tsc --noEmit`, `check-doc-links`: clean. No web build, no vitest (no source changed).
- **Firefox and WebKit: not run locally** (not installed here). They run it first in CI.
- If one of them fails in CI: that is SEC-AUD-400's finding, not a reason to weaken the spec. Open a row naming the engine and the probe point from the message; never delete the spec (mark that one engine `test.fixme` with the row ID only if the push must go green).

## Shot (1)
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-members/members-375.png`, from the scratch copy; opened: three rows, checkbox boxes aligned with avatar and name, Roles and More buttons below, no sideways scroll.

## Command
From `e2e/`, on a stack with the Owner bootstrapped by global setup (API 8695, Vite 5695):
`SR_E2E_BASE_URL=http://localhost:5695 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> npx playwright test tests/members-hit-test.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

## Frontier / docs / rows
- Docs: none state this; none changed. Stash: none touched.
- Inventory row (proposed): `400 | members | A CI spec for Members' checkbox. | Fixed: `e2e/tests/members-hit-test.spec.ts` runs the D-856 hit test and the toggle on every CI push in three engines. | one spec`. Closes the "CI spec" item of D-898's row.
- Threat-model rows: none. Changelog sentence: none (a test only). Owner questions: none.

## Clean-up
Stack stopped by PID tree, ports 8695/5695/9395 checked free, `sr_p91_members_ui` dropped, `p91-members-bin` deleted (`p80-base-bin` untouched), lock released, scratch folder removed.
