# Wave 51 handoff: ci (verification of rows that waited on CI)

Branch `feat/p51-ci`. Docs only: no workflow, spec, code, UI string or migration changed (nothing needed wiring). `gh` was used read-only.

## Done

Evidence is from main at merge commit `a0b72d01` (runs of 2026-11-06 19:03 UTC) and the scheduled sdk-compat runs.

- **SEC-ENT-002 ticked.** Web run 42276293423, job `static-browser-checks` (128102255378): 26 static tests passed, all 12 of `pwa-security.spec.ts` including the five wave 50 ones (worker scope and the single `sr-static-<build>` cache, public-only precache list, no `/app` request or cache entry, the two manifests, standalone error pages). E2E run 42276293453, job `playwright (chromium)` (128093355269): the three `pwa-update.spec.ts` drills passed (the spec is Chromium-only; firefox and webkit list it as skipped). The PR #94 run of E2E (42261444098) was green too.
- **SEC-ENT-004 stays unticked**, row now says "Waits on: the owner's next voice-soak dispatch". `voice-soak.yml` is dispatch-only and has not run since wave 50 added its steps. Everything else the row names is already proven by main (isolation check in `web.yml`/`e2e.yml`, spec on three engines, script tests in `repo-checks.yml`).
- **ARC-SBX-001 ticked.** The `live` job of `e2e.yml` already runs `tests/arcade-sandbox.spec.ts` with a shared `SR_SERVER__SECRET_KEY` (so `sr-cli arcade sign-code` and sr-server share the sealed seed and CI's Garage storage). E2E run 42276293453, job `live` (128093355238): `arcade-sandbox.spec.ts:49` passed (38.7s), covering sign-code, the vouch, the UNVERIFIED UPDATE flip and the refused stale vouch. No workflow or spec change was needed, so no parse test was necessary.
- **DEV-SDK-003 ticked.** `sdk-compat.yml` green on 2026-10-20 (dispatch, run 39432856749), 2026-10-26 (40318281178), 2026-11-02 (41459846383). In all three, `compat (discord.js)` and `compat (serenity)` each printed the 7 tier-2 checks `ok` (counted in the job logs). RDY-DEV-003 (discord.py, twilight) untouched.

## Main's latest runs since a0b72d01

All of Rust, Web, WASM, OpenAPI drift, Storage (live S3) and Repo checks are green. One red:
- **E2E run 42276293453: `built-bundle (firefox)` (128093355335) and `built-bundle (webkit)` (128093355391) failed in the "Install the browser" step**: `playwright install` failed or ran over 360s on all three tries, then exit 1 (a browser download/CDN stall; the same job on Chromium, and the full `playwright` jobs for all three engines, passed). No test failed. Not fixed (not in scope); a re-run by the owner would clear it. Consequence: the entrance-isolation/smoke/journey specs on Firefox and WebKit were not exercised on this merge commit (they were green on the PR #94 run).
- Earlier "cancelled" runs on 2026-11-06 (07:37, 08:18) are concurrency supersessions, not failures. E2E run 42254622234 on an earlier PR commit failed but was superseded by the green 42261444098.
- Not seen: flaky retries; I did not open the job logs of every green job for "flaky" lines beyond the ones named above.

## Not done

Nothing left in scope. SEC-ENT-004 waits only on the owner's voice-soak dispatch.

## Tests run

`node scripts/check-doc-links.mjs` (docs only change). No e2e or workflow file touched, so no workflow parse or `playwright --list` run was required. No visual check (no UI).

## Owner questions

None. Owner action: dispatch `voice-soak.yml` once (SEC-ENT-004), and optionally re-run the two failed built-bundle jobs.
