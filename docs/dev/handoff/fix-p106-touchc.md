# fix-p106-touchc (wave 106 census: 44 px hit test at 375, Server settings, Admin, tools and rooms)

Measure only. No app file changed. Commits start with D-1021 (a number I took; the task names none: the commander renumbers).

## Clock times
Started 16:30; spec written 16:33; first job 16:34; part 3 and part 2 results by 17:15; part 1 final 17:24; handoff 17:30. The runner was stopped about 16:42 to 16:57 (mount cargo round).

## Done
- e2e/tests/p106-touchc-shots.spec.ts (part 1 Server settings, 2 Admin, 3 tools and rooms; SR_P106_TOUCHC_PART). Figures: visual-sweep-shots/p106-touchc/figures-part1..3.json, one viewport shot per page.
- Report: the commander harness refused my Write of p106-touch-c-report.md (a report file), so the report is in my reply text, not on disk.

## Not done
- No fixes (census). Not covered: see the reply (moderation case page, listening-room card and watch strip absent in the world, Studio tabs, re-auth).

## FINDERS
none: no app file, no control changed.

## JOBS (id, class, waited, ran, rc)
0088 shoot base part 1 (first ten sections), 92 s, 54 s, 0 (superseded)
0097 shoot base part 2, 1125 s (runner stopped), 162 s, 0 (superseded: Create Role was tapped)
0109 shoot base part 1 (all), 588 s, 96 s, 0 (superseded: Create Role)
0111 shoot base part 2 (Admin), 679 s, 152 s, 0 (final)
0112 shoot base part 3 (tools), 869 s, 81 s, 0 (final)
0127 shoot base part 1 (Server settings), 665 s, 89 s, 0 (final)

## Figures
274 failing rows of 1378 (1023 pass, 81 skipped, 0 not-reached); grouped in the reply as G1 to G13. No limits to compare (census).

## Shots
Every page shot in visual-sweep-shots/p106-touchc was opened after its job: Server settings 24 shots, Admin 26, tools 6. Each shows the page at 375 with its shell (Sections toggle and close button at the top for settings and Admin; the Drawer for tools). Server bridge shows the Overview page (module off); game servers is a static preview. No page scrolls sideways (the spec notes any that does: none).

## Who imports my files
Only the spec (e2e, not a chunk).

## Checks
prettier --check on the spec: pass. e2e tsc: exit 0. eslint: the web package ignores e2e files (File ignored because outside of base path): not run.

## Rows and changelog
Proposed rows: one per group G1 to G13 in the reply. Changelog: none (no change).

## Owner questions
none.

## PROCESSES
strays: none found

Note from the integration: this branch's commit subjects say D-1021 by mistake; they name a census, not a decision. D-1021 is the kit Modal's focus on open (SEC-AUD-507).

Wave 107 correction: Fable assigned D-1021 to the Modal's focus on open (built in wave 107 by kitjs), so every commit of this branch whose subject says D-1021 names the touch census, not that decision. History is not rewritten; the census itself has no D-number.
