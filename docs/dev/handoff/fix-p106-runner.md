# fix-p106-runner (wave 106, tools builder; D-1013, D-1019; SEC-AUD-499)

Work: the clone `C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents-next/` (head `cd62e67`, base `abb2eb0`). Full report: `C:/Users/braxt/Desktop/SIDEROOM_wt/p106-runner-report.md`. No app file changed.

## Clock times
Started 16:30; items 0 to 7 committed by 16:47; full drill finished 16:52.

## Done (commits in the clone)
`0f30b3e` item 0 own folder; `24a72f0` item 2 wait 100; `cb0c132` item 3 Chromium in memwatch; `1f5f3a4` item 1 hold lines; `3cfaddc` item 4 cargo class; `08b4c6a` item 5 `--bin` (and drill checks 0, 13); `cd62e67` item 7 status holds and README. Drill: 13/13 (checks 0 1 2 3 4 5 6 7 9 10 11 12 13).

## Not done
Part 3 of the drill (real cells), by task. Nothing ran on a real cell (see the report, "untested").

## FINDERS (rule 13)
none: no app control changed. `job.sh wait` now defaults to 100 s (was 150); the commands in the brief are unchanged.

## JOBS
None submitted to the live runner (dummy jobs only, in `cli-agents-next/run-test` and the drill's own run folder).

## Figures
No app box moved. memwatch on this machine, Chromium column: old script 0, new 642 to 751 MB with three `chrome-headless-shell.exe` running.

## Shots
none (tools builder).

## Who imports my files
Nobody (scripts). The live `cli-agents/` is unchanged (`abb2eb0`).

## Checks
`bash -n` on every `*.sh` of the clone: ok. `bash runner-drill.sh` from a copy at `08b4c6a`: 13/13, 16:45 to 16:52. Web and e2e checks: not run (no app file).

## Closing text and new rows
D-1013 (the runner's next version): done as above. D-1019 (cargo in a jobs wave): the cargo class is done, untested with the real cargo. SEC-AUD-499: the password filter is unchanged and no file of the run folder holds the password (drill check 7). Proposed row: the first real shoot and the first real cargo job after the version is taken must be watched by the commander.

## Changelog sentence
The job runner now logs why a job waits, runs cargo jobs alone, can serve one job with its own server binaries, counts Chromium in its memory sampler, and a clone of its scripts runs itself.

## Owner questions
none.

## PROCESSES
`strays.sh cli-agents-next p106-runner`: strays: none found. The test runner (pid 48152) was stopped with `runner.sh stop`; the drill's runner was stopped by its teardown. The live runner (pid 46972) is not mine.
