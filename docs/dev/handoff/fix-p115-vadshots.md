# fix-p115-vadshots: handoff (INTERIM: base round done, after round waits for vad)

## Clock
Started 06:09, spec committed 06:15, base shoot done 06:17.

## Done
- 05487c148 `e2e/tests/p115-vad-shots.spec.ts`: five parts (1 settings card; 2 nothing loads early; 3 detector on the fake microphone; 4 fallback; 5 E2EE), written against brief 5c. e2e `tsc` clean (06:14, exit 0), prettier applied.
- Base part 1 (job 0355-vadshots-shoot, rc 0, 10 s): `base-voice-mode-standard-{1280-light,1280-dark,375-light}.png` and `figures-base.json`; base has no "Voice detection" control (recorded).

## Shots seen (base)
- base-voice-mode-standard-1280-light: Input mode card with Voice Activity chosen, auto-sensitivity switch on, sensitivity slider dimmed at -50 dB, hint line whole; nothing cut.
- base-voice-mode-standard-1280-dark: same, dark ground, all text readable.
- base-voice-mode-standard-375-light: card whole at phone width, the switch label wraps to two lines, the hint wraps; no sideways scroll.

## Not done yet
After round (merge fix/p115-vad, build, parts 1 to 5), faults, final handoff. Speech fixture: none given (step skipped and listed as not proven if absent).
Unknowns to fix once vad/assets land: the runtime chunk's name (RUNTIME_RE or SR_P115_VAD_RUNTIME), the fallback notice's role/text (selector in part 4 `notice`), `__srVadStats.frameMs` (spec assumes 32 ms if absent).

## Commands for the re-shoot (one per part; parts 2 to 5 need --server-env SR_VOICE__ENABLED=true)
bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p115-integ/apps/web/dist --server-env SR_VOICE__ENABLED=true --env SR_P115_VAD_ON=1 --env SR_P115_VAD_PHASE=after --env SR_P115_VAD_PART=<1..5> --env SR_P115_VAD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p115-vad -- tests/p115-vad-shots.spec.ts

## Scope
Chromium only (fake microphone flags); Firefox, Safari and a phone's CPU are not covered.
