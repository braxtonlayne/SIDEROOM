# fix-p112-handchecks (wave 112, D-1038)

## Clock times
Started 01:54 (2026-11-19, before T), page and top note written by about 02:00; doc-links run and commit at the end. Nothing running.

## Done (commits)
- `D-1038 deployment-day checks`: new `docs/dev/deployment-day-checks.md`; top note of `docs/dev/r2-byos-test.md` rewritten (the old "Postponed" note replaced; the rest untouched).

## Not done (why)
- No row text edited (not mine); sentences below.
- The R2 page's origin, stack and clean-up are not rewritten (host unknown, as the task says).

## FINDERS
`git grep -n "r2-byos-test"` across docs, deploy, e2e, .github: only `docs/01-decisions-log.md:1470` (D-1029) and the comment in `e2e/tests/r2-byos.spec.ts:16`. My top-note change breaks neither. Pages that should link the new one (not mine):
- `deploy/GO-LIVE.md`, after the "Reference pages" sentence in the intro: "The tests that wait for a real server (R2, two-network voice, Safari, the payment walk, the playbooks) are listed in order in `docs/dev/deployment-day-checks.md` (link: ../docs/dev/deployment-day-checks.md from deploy/)."
- `docs/dev/owner-checklist.md`: a row "Run the deployment-day checks (`docs/dev/deployment-day-checks.md`) after GO-LIVE section 7 and before the first invites; have a Mac or iPhone and a second person with a phone ready."
- `docs/12-deployment-and-operations.md` section 5 (after the D-1037 search sentence): "After a restore the search folder (`search.dir`) is not part of the database; if search is empty or stale, run `sr-cli reindex` (it queues `search.reindex`)."

## What I checked and each verdict
- GO-LIVE sections and their order (GO-LIVE.md lines 11-70): matches; backup restore is section 5, before the Setup checklist (7), so I put it second and said why.
- `voice-soak.yml` can target a deployed host: stale premise, answer no. It builds the web app and starts its own `sr-server` at `http://localhost:8080` (`SR_E2E_BASE_URL: http://localhost:8080`, the "Start sr-server" step), CI runner only. So the two-network call is the real-server evidence.
- `storage-live-test.md`: does not wait for a server; it runs in CI against compose Garage (`storage.yml`, its line 5). `media-pipeline-test.md`: runs in CI (its status line). Garage on the server is covered by `sr-cli preflight` (GO-LIVE 4) and the Setup checklist storage test (GO-LIVE 7).
- RUNBOOK playbook anchors: 16 playbooks from "Job backlog" to "A member reports CSAM" (RUNBOOK lines 127-277). The anchors are my lower-case-hyphen derivations of the headings; `check-doc-links.mjs` does not check them in this table (they are in code ticks), so they are matches by heading text, not by tool.
- TV-HLS-005: `crates/sr-tv/tests/hls_sync.rs` `both_clients_see_the_discontinuity_at_the_programme_boundary` (lines 147-175) matches my description (EXT-X-DISCONTINUITY, continuous numbering).
- Music UI: "Music in the call" group, mute and volume (`MusicControls.tsx`, `lounge.music.*`), Jukebox "Add tracks" (`lounge.jukebox.add`): match.
- `sr-cli reindex`: exists (`crates/sr-cli/src/main.rs:114`, `ops.rs:456`).
- docs/12 after wave 111: D-1037 sentence is about upgrades only; nothing about restore. Stale-by-omission; replacement above.
- Cannot tell from the tree: how to schedule two back-to-back programmes on a Live TV channel as the Owner (step 5b.1 is written generally); how `r2-byos.spec.ts` signs in on a real server (it uses the e2e Owner of a test stack). Both are for the owner and Fable to settle.
- Checks run: `node scripts/check-doc-links.mjs` exit 0 (before the final commit and again after, see end). Prettier: not applicable (does not cover `*.md`).

## What blocks a first deployment
Nothing found.

## Proposed rows (sentence for the inventory, not mine)
- MUS-JB-001: append "Safari part: by hand on a real Safari at deployment (D-1038), see `docs/dev/deployment-day-checks.md` entry 5a."
- TV-HLS-005: replace "left: the Safari discontinuity test, no WebKit spec covers Live TV in `e2e/tests`" with "left: the Safari discontinuity check, by hand on a real Safari at deployment (D-1038), see `docs/dev/deployment-day-checks.md` entry 5b."
- RDY-VOX-002: append "`voice-soak.yml` cannot target a deployed host (it starts its own sr-server on localhost:8080); on a real server the evidence is the two-network call, see `docs/dev/deployment-day-checks.md` entry 4."
- RDY-DOC-003: append "the walk is tracked in the playbook table of `docs/dev/deployment-day-checks.md` entry 7."

## Owner questions (default in brackets)
- Is there a Mac or an iPhone to hand for the Safari checks? [Assume an iPhone; the checks work on either.]
- Who is the second person for the two-network call? [A friend with a phone on mobile data; the owner's own phone as the second network if nobody else.]
- Are donations to be on at launch? [No: skip entry 6 until they are.]
- Does the owner use an R2 bucket of his own? [No: skip entry 3.]

## PROCESSES
None started by me. Run `bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/strays.sh p112-handchecks` shows nothing of mine (not run by me: I started nothing).
