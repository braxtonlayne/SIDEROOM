# Wave 94, seek: SEC-AUD-429 (D-935), why the audio returns to 0 after a seek (report only, nothing built)

2026-11-17. Clock: start 13:55, lock taken 14:05, one round 14:05 to 14:10 (stack up 14:07, spec run twice: 42 s and 44 s; released 14:10). Commits: `SEC-AUD-429: seek snap-back isolation spec (report only)` and the handoff commit. Files: `e2e/tests/p94-seek-shots.spec.ts` (new) and this handoff. No player file, no support file, no earlier spec touched.

## Cause (found; in the player, not in the harness)

**Every seek on the player makes the browser fire `waiting`; `engine.ts` turns `waiting` into the store status `loading`; the store subscriber then sees "entering loading" and calls `load()`, which sets `src` again and `currentTime = 0` on the deck, so the track restarts from 0.** The tone and the mock are innocent.

Evidence (`measure.json`, the console `FACT` lines):

- Who wrote it: our code. After `currentTime = 20` (case a) or the Seek input clicked at 50 % (case b) the wrapped setters log, 4 to 94 ms later, `set src`, then `set currentTime 0`, then `play()` on the same element, with the stack `GaplessDecks.start (src/music/queue/gapless.ts:80-81)` from `load (src/player/engine.ts)` from the store subscriber `onStore (engine.ts)`. Events in order: `seeking` (currentTime 20), `waiting`, then 3 ms later `abort`, `emptied`, `play`, `loadstart` (currentTime 0), `durationchange`, `loadedmetadata`, `canplay`, `playing`. The browser did not reset it; our `load()` did. The file is requested again (a second `Range: bytes=0-`, answered 206).
- The trigger is the `waiting` event: case d, the same seek with `waiting` swallowed in the capture phase before the page's own listeners (spec init script, `__blockWaiting`), does NOT snap back: min 20.009, max 21.992, one write (the seek only), no new request.
- Ruled out: the tone (40 s, 8 kHz 8-bit mono, 320 044 bytes, header rate 8000, data 320 000 = 40 s, so 20 s is inside; 25 % = 10 s snaps back too); `seekable` and `buffered` are [0, 40] before and after; the mock (the first request is `Range: bytes=0-`, answered 206 with `content-range: bytes 0-320043/320044` and `content-length 320044`). A bare `new Audio('/e2e-tone-1.wav')` on a neutral page with no app code, seeking to 20 and to 10, does not snap back (min 20.016, max 21.949). Content-length on both answers, a 160 s tone, a 206 for the first plain request, and all three together change nothing (none snaps back); only "no range support at all (200, no accept-ranges)" snaps back on a bare element, which `serveTone` already avoids.
- Same on every set-up tried: 1280 x 800 Editorial (default skin, `retro`), cases a, b, c (c = 25 % of `duration`); 375 x 812 touch with Tidepool, a direct set and a real tap on Seek. The bare element also fires `seeking`, `waiting`, `seeked`, `canplay`, `playing` on a seek, so `waiting` is Chromium's normal answer to a seek (readyState drops to 1 while it seeks), not a harness artefact.

## Could members meet it with a real track? Not proven

Proven: in Chromium, with the dev build, any seek on the player restarts the track, whatever the skin or width, with a plain ranged file. Not proven: a real file from the server (not tested: no upload, the world has none), and I did not read the production bundle. The trigger (`waiting` on every seek) is Chromium's own, shown on a bare element, so members likely can meet it. Likely, not proven.

## Proposed fix (not built, not measured; a player file, one owner)

`apps/web/src/player/engine.ts`, `onWaiting` (source lines 464 to 466; the stack's `engine.ts:317` is the dev-transformed line of `void load()` at source line 420):

```diff
   const onWaiting = fromPlaying(() => {
-    if (store.getState().status === 'playing') engineReport.status('loading');
+    // A seek fires `waiting` too; it must not look like a new load (`onStore` would call `load()` and restart at 0).
+    if (playing().seeking) return;
+    if (store.getState().status === 'playing') engineReport.status('loading');
   });
```

Alternative, in `onStore` (line 419): `if ((entering && key !== loadedKey) || ...)`; but a retry after an error may enter `loading` with the same key, so the guard above is the smaller change. Either needs the engine's vitest files (`engine.test.ts`, `gapless-engine.test.ts`, `rate-pan-sleep.test.ts`) run, and a Seek check in the browser: this spec's cases a, b (and d) are that check; with the fix, a and b should stop snapping back with `waiting` NOT swallowed.

`e2e/support/playerAudio.ts` needs NO patch. Specs that import it (`git grep -ln playerAudio e2e`, 15 files): `support/playerMock.ts`, `drawer`, `jukebox`, `p51-overlay-shots`, `p75-player-shots`, `p77-tabbar-shots`, `p78-dock-shots`, `p92-skins-shots`, `p93-circuit-shots`, `p93-dock-shots`, `p93-study-shots`, `p93-tidepool-shots`, `player-layout`, `player`, `watch-together`. None is touched by the fix; once it lands, the specs that assert Seek only by the input's `input` event (D-935's interim rule) can assert `audio.currentTime` as well.

## What the spec does

`e2e/tests/p94-seek-shots.spec.ts` (local only, `SR_P94_SEEK_ON=1`; needs the sweep's world; its header has the commander's command, whole). It wraps the `currentTime` and `src` setters and `load`/`play`/`pause` on `HTMLMediaElement.prototype` (value, time, stack), records every media event with `currentTime`, logs every `e2e-tone-*.wav` request and response, and runs cases a, b, c, d at 1280, a and b at 375 touch with Tidepool, a bare element on a neutral page, and five tone-route variants. Output: `measure.json` (my run: `C:/Users/braxt/Desktop/SIDEROOM_wt/p94-seek-scratch/out/measure.json`, not committed). Shots: none made, none listed.

## Checks (on my tree, after my last spec edit)

- e2e `tsc` (`npx tsc --noEmit -p tsconfig.json` from `e2e/`), 14:09: exit 0.
- The spec, `--project=chromium --reporter=line`: 1 passed (43.6 s), on the final spec at 14:10.
- prettier on the spec: written (before the 14:09 `tsc`).
- `node scripts/check-css-tokens.mjs`, `node scripts/i18n-audit.mjs --max 0`, `node scripts/check-doc-links.mjs`: results in the final report.
- Base table: not applicable (no CSS, no hit test). CSS in my last commit: none.

## Proposed rows and text

- SEC-AUD-429 (stays open, as a player fault, not "harness"): "After any seek the player restarts the track: the browser's `waiting` event during the seek sets the status `loading`, `onStore` calls `load()` (`apps/web/src/player/engine.ts` `onWaiting` at line 464, `load` at line 420; `gapless.ts:80-81` sets `src` and `currentTime = 0`). Proven in the harness (Chromium, every skin and width tried); a real server file not tested. Fix proposed in the seek handoff: guard `onWaiting` on `audio.seeking`."
- Changelog sentence (only if the fix lands): "Dragging the player's Seek bar no longer restarts the track from the start."
- Threat-model rows: none. Owner questions: none.

## Cleanup

Stack stopped by PID tree (API 33944, Vite 43776 and children), ports 8724, 5724 and 9424 checked free, `sr_p94_seek_ui` dropped, `p94-seek-bin` deleted (`p92-base-bin` and `p80-base-bin` not touched), lock released through `heavy.sh` (status `free` after). No stash touched. `p94-seek-scratch/` is outside the worktree.
