# Wave 50: MUS-SKN-002, the player skins' working extras (branch `feat/p50-skins`)

Builds section 4 of Fable's wave-50 decisions (D-520 to D-527) in Fable's build order: (1) the shared rate,
balance and sleep plumbing, (2) the Pastel Study Deck's ambience mixer, (3) Valve & Walnut's tape speed and
balance, (4) Night Circuit's tempo fader, (5) Tidepool Glass's sleep timer. All five slices are built; nothing
was left for later beyond what D-526 already defers.

## Done

| Commit | What |
|---|---|
| `a15b5d52` | **Plumbing (D-525).** `player/store.ts`: `rate` (0.5..2), `pitchFollowsRate`, `pan` (-1..1), `sleepAt` with `setRate(rate, { pitchFollows })`, `setPitchFollowsRate`, `setPan`, `setSleep(minutes \| null)`, and a `stop()` action (pause, back to 0:00, sleep timer off); `clear()` also clears `sleepAt`. None of it is persisted; a new queue keeps rate and pan. `engine.ts` `applyRate`: `defaultPlaybackRate`, `playbackRate`, `preservesPitch` and `webkitPreservesPitch` on both decks, at change, after each load and at the gapless hand-over (`onAdvance`). `graph.ts`: `master -> StereoPannerNode -> analyser` where `createStereoPanner` exists (else the old chain), `applyPan`. New `player/sleep.ts` (`startSleepWatcher`, started once by `PlayerHost`): one timeout to `sleepAt - 30 s`, a `linearRampToValueAtTime` of the master gain to 0, then pause and the gain restored; clearing cancels the ramp and restores at once. `mediaSession.ts` reports the real rate. `music/queue/gapless.ts`: the hand-over time divides the media seconds left by the deck's `playbackRate` (otherwise a 1.2x deck would cut late). Every stop button (shared `StopButton`, Circuit, Stack, Tidepool) now calls `stop()`. `parts.tsx`: `snapDetent`, `resetHandlers` (double-click or Enter back to the default) and `usePitchPolicy` (the skin in use sets the pitch policy; switching skins keeps the rate). Tests: `player/__tests__/rate-pan-sleep.test.ts` (8). |
| `831f1e8d` | **Study's mixer (D-524).** New `player/ambience.ts` (Study chunk only): its own `AudioContext` through the `ContextFactory` seam; rain = looping white noise through a 1.5 kHz band-pass (Q 0.7) swayed by a 0.1 Hz LFO (depth 0.3) plus a high-shelf hiss layer; crackle = a brown bed at -24 dB plus 2-6 ms noise bursts every 80-250 ms through a 3 kHz low-pass; cafe = brown noise through a 400 Hz low-pass swayed by a 0.05 Hz LFO plus a sine clink (2-6 kHz, 60 ms, -30 dB) every 3-9 s. 50 ms gain ramps; all at zero suspends, any above resumes; `dispose` closes. The tab state (levels, the live mixer) is module-level: created on the first non-zero level, kept across a skin change, closed when the skin goes away with all at zero, and stopped (levels back to 0) when the player's queue empties (the close button, sign-out). `StudySkin.tsx`: the three sliders are real vertical `<input type="range">` (0-100, labels Rain / Crackle / Café, `aria-valuetext` "65%", double-click or Enter to 0) over the painted thumbs; the caption is "Made in your browser, nothing is downloaded." (`p50-skins-1.ts`; the old `mixerNote`/`mixerSoon` keys are gone). Tests: `player/__tests__/ambience.test.ts` (8). |
| `5ebe7090` | **Valve (D-521).** The tape-speed plate is a real slider under its plate (0.75x-1.25x, step 0.01, detent at 1.00x, `aria-valuetext` "1.10x", `setRate(v, { pitchFollows: true })`); the plate reads "Tape speed 1.10x" (`player.skinx.valve.speed` kept, text changed). It shows on phones too (the jack art still steps aside there). A seventh knob "Balance L/R" sits beside Master, built like the EQ knobs (-1..1, step 0.05, snapped to the centre within 0.04, "Center" / "Left 40%" / "Right 60%"), driving `setPan`. The skin applies the tape pitch policy while shown. |
| `e37279d3` | **Circuit (D-523).** A TEMPO fader beside GAIN (-8% to +8%, step 0.5, detent at 0 within 0.25, `setRate(1 + v/100, { pitchFollows: false })`), its hazard-stripe cap with a cyan glow, a detent line, a visible `<label>` "Tempo" and the readout ("+4.0%", "−2.5%", "±0.0%") under it; mute spans both faders. The fader column is 7.25rem (was 4.75) and the console 56rem wide (was 54) so the transport keeps its width. |
| `6050da03` | **Tidepool (D-522).** A pearl "Drift off" pill in the status strip beside Repeat and Stop: off -> 15 -> 30 -> 60 -> off (each click is the next step above the minutes left), reading "Drift off: 28 min" on a one-minute clock (D-501's pattern), `aria-pressed` while it runs. Tests in `backdrop-skins.test.tsx` for the tape speed (rate, pitch policy, 1.00x snap, double-click and Enter reset, the policy dropping when the skin goes), balance, tempo, the Drift off cycle and countdown (and Stop turning it off), and the mixer sliders. |
| `7208bc3a` | D-520 to D-527 copied into `docs/01-decisions-log.md` (after D-501); `e2e/tools/player-skins-preview.mjs` also shoots each extra in use (D-527). |
| this commit | Valve: knobs top-aligned on phones (the two-line "Balance L/R" legend had lifted its knob); the MUS-SKN-002 inventory row ticked; this handoff. |

## Not done / left as decided

- D-526's later items stay later: synced lyrics, a Playlists wing, beat-aware pads, real bitrate/sample-rate cells in
  Steel Stack, an ambience preset saved per account, a favourite heart.
- Without Web Audio (no graph) the sleep timer pauses at the time without the fade; without `createStereoPanner` the
  balance knob moves but does nothing (the old chain is kept).
- While the sleep fade runs, moving the volume sets the master gain directly and can cut the ramp short; the pause
  still comes at the set time.

## Visual check (AGENTS.md section 4, D-527)

- **Re-shot `C:/Users/braxt/Desktop/SIDEROOM/screenshots/player-skins/final/`** (overwritten by
  `node e2e/tools/player-skins-preview.mjs`): the five backdrop skins at 1280x800 and 360x740, player crop and full
  viewport, plus the extras in use at both sizes: `study-*-extra-mixer-up`, `valve-*-extra-tape-1.1x`,
  `valve-*-extra-balance-left-40`, `circuit-*-extra-tempo-plus4`, `tidepool-*-extra-drift-off-30`. The tool's fit and
  axe (with contrast) checks: every skin fits, no sideways scroll, axe clean at both sizes. All looked at. One fix from
  looking: on phones the Balance knob sat higher than the others (its legend wraps to two lines); the knob row is now
  top-aligned and re-shot. `study-real-timer-running.png` is the older wave-34 shot, left as it was.
- **The page around the docked player:** `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p50-skins/`,
  `lounge-home-player-<skin>-<desktop|phone>-<lounge-light|lounge-dark|lounge-editorial>.png` for Valve, Tidepool,
  Circuit and Study (24 shots from a scratch Playwright probe on `/channels/@me` with the theme set on the account;
  not committed). The skins wear their fixed themes; the Lounge around them follows Light, Dark and Editorial. Looked
  at, nothing wrong. The full page sweep was not re-run: no route or page code changed, only the player.
- Flows clicked (in the preview tool and the unit tests): mixer sliders up and back to zero, tape speed, balance,
  tempo, the Drift off cycle, Stop clearing the sleep timer, a skin switch keeping the rate.

## Tests and gates

- `e2e/tests/player-layout.spec.ts` (chromium, own stack on 8225/5225, DB `sr_p50_skins`, dropped after): **7 passed**:
  all eight skins fit at 1280, 360 and 320 px and pass axe with contrast; artwork hidden and still under reduced motion.
- vitest `src/player src/i18n src/music/queue`: 23 files, 280 tests passed (new: `rate-pan-sleep.test.ts` 8,
  `ambience.test.ts` 8, 6 new cases in `backdrop-skins.test.tsx`).
- Web typecheck clean; eslint (the web package's config) and prettier clean on every changed file; `check-css-tokens`
  clean; `i18n-audit` TOTAL 0; `check-doc-links` ok; `playwright test --list` loads all 67 files.
- `node scripts/build-web.mjs`, then `check-entrance-isolation`: every public surface clean. `check-bundle-budget`:
  every row ok, including `player` (no player or music library code in the entrance), **except `music`** ("expected
  one music_bg-*.wasm, found 0"): this worktree's copied `packages/wasm/dist` has no `music` or `tools` build, so I put
  the same one-line `init` stubs there that the audio builder used (gitignored, not committed) to let the app build;
  the row fails on the stub, not on this branch. The Study chunk (`StudySkin-*.js`, now with the mixer) is 6.4 KB gzip
  in all; the mixer code is only there (none in `PlayerHost`).
- No Rust touched: no cargo, OpenAPI or migrations.

## Owner questions (for Fable)

**Answered: D-530 (the mixer stops with the player; closing resets rate, pitch policy and balance: built) and D-531 (no clamp on a skin switch; the readout shows the real rate: built).** Questions below are kept for the record.

1. Closing the player (its queue emptied) stops the ambience mixer and puts the sliders back to zero; a skin change
   keeps it playing. Should the mixer instead survive a close for as long as the tab lives?
2. Rate and balance are kept when the player is closed and reopened in the same tab (like volume, but not persisted).
   Should closing reset them to 1.00x and center?
3. A rate outside a skin's range (Valve's 1.25x seen in Circuit, whose fader stops at 8%) plays at the real rate while
   the fader rests at its end stop. Clamp the rate when switching to a skin with a narrower range?

## Note

- Running `npx eslint` from the repo root (which has no eslint of its own) made npx fetch eslint 10.14.0 into its
  cache before I switched to the web package's own eslint. Nothing was installed into the repo and that run produced
  no result; I mention it because the brief forbids downloads.
