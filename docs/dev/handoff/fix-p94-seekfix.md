# Wave 94, seekfix: SEC-AUD-429 (D-935) fixed, the seek no longer restarts the track

2026-11-17. Clock: start 14:12, one stack round 14:18 to 14:19 (lock 14:17 to 14:19; the player/music vitest ran under `heavy.sh run` first, 14:17). Files: `apps/web/src/player/engine.ts`, `apps/web/src/player/__tests__/engine.test.ts`, `e2e/tests/p94-seek-shots.spec.ts`, this handoff. No CSS, no i18n, no build; the player chunk is lazy (no static import added).

## Cause

Every seek makes the browser fire `waiting`. `onWaiting` set the store status `loading`; the store subscriber (`onStore`) treats "entering `loading`" as a new track and called `load()`, which sets `src` again and `currentTime = 0` (`gapless.ts:80-81`). The seek builder's proposed guard (`audio.seeking`) would have fixed only seeks: a real stall mid-play (`waiting` with `seeking` false) takes the same path and restarted the track too.

## Fix (engine.ts only)

`onWaiting` now reports `loading` inside a flag, `reportingWaiting`, set around the one `engineReport.status('loading')` call (the store notifies subscribers synchronously, so the flag is true exactly while `onStore` sees that change). `onStore` computes `entering = status loading && prev not loading && !reportingWaiting`. So a `loading` that the element itself reported (seek or stall) shows the buffering state but never reloads. A `loading` that comes from the store (play of a new queue, next/previous, retry after an error, a pick) still enters and calls `load()`. The gapless hand-over branch (`handedOver === key`) and the "playing with a different key" branch are untouched; the resume branch needs `el.paused`, so the `playing` that follows a `waiting` does not replay.

## Tests

- `engine.test.ts`, three new cases: a seek's `waiting` (src not written again, currentTime stays 60, one `play()`, status loading then playing); a mid-play stall (no `load()`, currentTime stays 33, one `play()`); a track change after a `waiting` still loads (`ended` then `/t2.mp3` at 0, then `play(track(3))` loads `/t3.mp3`). Run on the unfixed engine: all three fail (3 failed, 17 passed); on the fix: 20 passed.
- `npx vitest run src/player src/music --maxWorkers=2` (under `heavy.sh run`): 23 files, 295 tests, all pass (includes engine, gapless-engine, rate-pan-sleep).
- Web `tsc --noEmit -p .`: exit 0. eslint on the two changed web files: clean. prettier: clean.

## Spec figures (`p94-seek-shots.spec.ts`, my stack, 8727/5727, the sweep world; 1 passed, 45 s)

The spec now asserts, for every player case, `snappedBack === false` and `min > target - 1`.

| case | before (seek builder, SEC-AUD-429 finding) | after the fix (min / max over 2 s) |
|---|---|---|
| a: currentTime = 20 (1280) | snapped back to 0, `set src` + `set currentTime 0`, 2nd request `Range: bytes=0-` | 20.021 / 22.004, 1 write, 0 requests |
| b: Seek input clicked at 50 % (1280) | snapped back | 20.016 / 22.000, 1 write |
| c: 25 % of duration (10 s) | snapped back | 10.028 / 12.001 |
| d: 20 with `waiting` swallowed | 20.009 / 21.992 (no snap) | 20.021 / 21.999 |
| 375 touch Tidepool, direct set to 20 | snapped back | 20.042 / 22.021 |
| 375 touch Tidepool, tap on Seek 50 % | snapped back | 20.035 / 22.004 |

Bare element and tone-route variants are unchanged (no snap, except "no range support at all", which snaps on a bare element and is not the app's route). `measure.json`: `C:/Users/braxt/Desktop/SIDEROOM_wt/p94-seekfix-scratch/out/` (not committed). Not tested: a real file from the server (the world has none).

## Closing text, SEC-AUD-429 (closed)

"After any seek the player restarted the track: the element's `waiting` event set the store status `loading` and `onStore` read that as a new load and called `load()` (`engine.ts`; `gapless.ts:80-81` sets `src` and `currentTime = 0`). A mid-play stall did the same. Fixed in wave 94: a `loading` that the element reports through `waiting` is flagged (`reportingWaiting`) and no longer enters `load()`; store-driven loads, track changes and the gapless hand-over are unchanged. Proven by three engine tests (each fails on the old engine) and by the seek spec (a, b, c and the phone cases stay at the target; before, they went to 0). Real server file not tested."

## Changelog sentence

"Dragging the player's Seek bar, or a short network stall while a track plays, no longer restarts the track from the start."

Specs that assert Seek only by the input's `input` event (D-935's interim rule) may now assert `audio.currentTime` too. Threat-model rows: none. Owner questions: none.

## Cleanup

Stack stopped by PID tree (API, Vite), ports 8727, 5727, 9427 checked free, `sr_p94_seekfix_ui` dropped, `p94-seekfix-bin` deleted (`p92-base-bin`, `p80-base-bin` untouched), lock released through `heavy.sh` (status `free`). No stash, no push, no merge.
