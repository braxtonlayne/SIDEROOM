# Wave 98 handoff: gate (SEC-AUD-437, D-944)

Clock: started 16:30, finished 16:32 (2026-11-17).

## Confirmed by reading
Spec polled fa (member frame) for phase 'reveal', then read fg (guest frame) once and asserted after.word. Before the reveal state().word is '' (packages/starter-packs/activities/sketch/host/engine.js:99 initial, :421 reset at turn start; set to the prompt at :503 on reveal). So a guest frame not yet at reveal gives an empty word.

## Diff in words
One comment line and a second expect.poll on fg (state()?.phase to be 'reveal', 15 s, message naming D-944 and the late guest frame) between the fa poll and the fg read. Read and assertions unchanged.

## Places with the same shape
| line (before edit) | waited | read | result |
|---|---|---|---|
| ~245 | fa (2 players) | fg.guest() | left: guest flag is fixed at join, not event-driven; not asserted from a phase |
| ~296-298 | fa phase pick | fa hostId/drawer | left: same client |
| ~300-306 | fd choices | fd | left: same client |
| ~313 | fd prompt | fd | left |
| ~328 | fq turnNo | fq mask | left: same client |
| ~337 | fa reveal | fg state | FIXED (the item) |
| after | fg reveal | gl overlay | expect auto-waits |

## ECONNRESET
webServer in e2e/playwright.config.ts:77+ (server mode) starts or reuses sr-server; in CI deploy/compose/e2e-run.sh starts it. I did not trace what proxies through it; no finding, no change.

## Checks
- prettier on the spec: ran, written.
- e2e tsc (npx tsc --noEmit -p tsconfig.json, from e2e/) after the last edit, 16:31: exit 0.
- node scripts/check-doc-links.mjs: exit 0.
- The spec itself: not run locally (needs the voice stack; the PR's live job runs it).
- heavy.sh status: free (I hold nothing; took no lock).

## Proposed row
SEC-AUD-437 (low, test harness): sketch-gate spec now waits for the reveal on the guest's frame before reading its word. Closes on the merge; reopens if the live job fails on the word again.

Changelog: test harness only: the sketch-gate spec waits for the guest's reveal before reading the word.

Owner questions: none.
